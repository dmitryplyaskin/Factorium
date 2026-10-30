//! Reconstruct per-ingredient paths, pickup reach and output lanes from geometry.

use super::{GeneratedFactory, entity, inserter_reach, supported_recipe};
use crate::{GameData, blueprint::BlueprintEntity};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(super) fn validate(
    data: &GameData,
    f: &GeneratedFactory,
    ids: &BTreeMap<u32, &BlueprintEntity>,
    occupied: &HashMap<(i32, i32), u32>,
    machines: &BTreeSet<u32>,
    belts: &BTreeSet<u32>,
    arms: &BTreeSet<u32>,
) -> Result<()> {
    let recipe = data
        .recipes
        .get(&f.recipe)
        .context("Unknown factory recipe")?;
    let flow = supported_recipe(data, recipe, entity(data, &f.request.machine)?)?;
    let requirements = f.input_requirements();
    ensure!(
        requirements.len() == flow.ingredients.len(),
        "Ingredient requirements count mismatch"
    );
    ensure!(
        recipe.products[0].name == f.request.product && flow.ingredients[0].0 == f.input_item,
        "Input/output item mismatch"
    );
    for (i, (r, (item, amount))) in requirements.iter().zip(&flow.ingredients).enumerate() {
        ensure!(
            r.item == *item
                && (r.per_second - f.request.rate * amount / flow.output).abs() < 1e-6
                && (r.supplied_per_second - f.planned_output_per_second * amount / flow.output)
                    .abs()
                    < 1e-6,
            "Ingredient requirement violates recipe material balance"
        );
        ensure!(
            r.inserter_items_per_second.is_finite() && r.inserter_items_per_second > 0.0,
            "Invalid ingredient inserter capacity"
        );
        ensure!(
            inserter_reach(entity(data, &r.inserter)?, if i == 0 { 1 } else { 2 }),
            "Ingredient inserter has incompatible pickup/drop reach"
        );
    }
    ensure!(
        requirements[0].inserter == f.request.inserter
            && (requirements[0].per_second - f.input_per_second).abs() < 1e-6
            && (requirements[0].supplied_per_second - f.supplied_input_per_second).abs() < 1e-6,
        "Primary ingredient compatibility fields mismatch"
    );
    let belt_speed = entity(data, &f.request.belt)?.prototype.properties["belt_speed"]
        .as_f64()
        .context("Missing belt speed")?
        * 480.0;
    let touches_tile = |id: u32, tile: (i32, i32)| -> Result<bool> {
        let e = ids[&id];
        let b = entity(data, &e.name)?.collision_box;
        Ok(e.position.x + b.right_bottom.x > f64::from(tile.0)
            && e.position.x + b.left_top.x < f64::from(tile.0 + 1)
            && e.position.y + b.right_bottom.y > f64::from(tile.1)
            && e.position.y + b.left_top.y < f64::from(tile.1 + 1))
    };
    let walk = |start: u32, stop: Option<u32>, direction: Option<u8>| -> Result<Vec<u32>> {
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = start;
        loop {
            let e = *ids.get(&current).context("Unknown belt port")?;
            ensure!(seen.insert(current), "Production belt forms a cycle");
            let underground = f.bus.as_ref().filter(|b| e.name == b.underground);
            ensure!(
                e.name == f.request.belt || underground.is_some(),
                "Port is not a belt"
            );
            ensure!(
                direction.is_none_or(|direction| e.direction == direction),
                "Production belt has wrong direction"
            );
            path.push(current);
            if stop == Some(current) {
                ensure!(
                    e.name == f.request.belt,
                    "Output port must be a surface belt"
                );
                break;
            }
            let (dx, dy) = match e.direction {
                0 => (0, -1),
                4 => (1, 0),
                8 => (0, 1),
                12 => (-1, 0),
                _ => anyhow::bail!("Invalid belt direction"),
            };
            if underground.is_some() && e.belt_type.as_deref() == Some("input") {
                let reach = entity(data, &e.name)?.prototype.properties["max_underground_distance"]
                    .as_u64()
                    .context("Missing underground reach")? as i32;
                current = (1..=reach)
                    .find_map(|n| {
                        occupied
                            .get(&(
                                e.position.x.floor() as i32 + dx * n,
                                e.position.y.floor() as i32 + dy * n,
                            ))
                            .and_then(|id| ids.get(id))
                            .filter(|other| {
                                other.name == e.name
                                    && other.direction == e.direction
                                    && other.belt_type.as_deref() == Some("output")
                            })
                            .map(|other| other.entity_number)
                    })
                    .context("Disconnected output underground crossing")?;
                continue;
            }
            ensure!(
                underground.is_none() || e.belt_type.as_deref() == Some("output"),
                "Invalid output underground belt type"
            );
            let next = (
                e.position.x.floor() as i32 + dx,
                e.position.y.floor() as i32 + dy,
            );
            match occupied.get(&next).and_then(|id| ids.get(id)) {
                Some(e)
                    if e.name == f.request.belt
                        || f.bus.as_ref().is_some_and(|b| e.name == b.underground) =>
                {
                    let from = ids[&current];
                    ensure!(
                        e.direction != (from.direction + 8) % 16,
                        "Output route enters the front of a belt"
                    );
                    if e.name != f.request.belt {
                        ensure!(
                            e.direction == from.direction
                                && e.belt_type.as_deref() == Some("input"),
                            "Output route enters underground from the wrong side"
                        );
                    }
                    current = e.entity_number
                }
                _ => break,
            }
        }
        Ok(path)
    };
    let extra = super::infrastructure_check::validate(data, f, ids, occupied)?;
    let mut used_belts = extra.belts;
    let mut used_machines = BTreeSet::new();
    let mut used_arms = extra.arms;
    let mut output_paths: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut output_lanes = BTreeSet::new();
    let mut output_rate = 0.0;
    let mut input_rates = vec![0.0; requirements.len()];
    for row in &f.rows {
        let ports: Vec<_> = row.input_ports().collect();
        ensure!(
            f.request
                .max_machines_per_row
                .is_none_or(|n| row.machines.len() <= n),
            "Row exceeds machine-count limit"
        );
        ensure!(
            ports.len() == requirements.len(),
            "Every row needs a separate port for every ingredient"
        );
        ensure!(
            row.output.rate.is_finite()
                && row.output.rate > 0.0
                && row.output.item == f.request.product,
            "Invalid row output"
        );
        ensure!(
            matches!(row.output.lanes.as_str(), "left" | "right"),
            "Each row must declare exactly one output lane"
        );
        ensure!(
            output_lanes.insert((row.output.entity_number, row.output.lanes.as_str())),
            "Output lane reused by multiple rows"
        );
        ensure!(
            ids.get(&row.output.entity_number)
                .is_some_and(|e| e.position == row.output.position),
            "Output port coordinates mismatch"
        );
        let mut input_lines = Vec::new();
        for (i, port) in ports.iter().enumerate() {
            ensure!(
                port.rate.is_finite() && port.rate > 0.0 && port.item == requirements[i].item,
                "Wrong or missing ingredient at input port"
            );
            ensure!(port.lanes == "both", "Input must use both belt lanes");
            ensure!(
                (port.rate - row.output.rate * flow.ingredients[i].1 / flow.output).abs() < 1e-6,
                "Row violates recipe material balance for ingredient {}",
                port.item
            );
            ensure!(
                ids.get(&port.entity_number)
                    .is_some_and(|e| e.position == port.position),
                "Input port coordinates mismatch"
            );
            let path = walk(port.entity_number, None, Some(4))?;
            ensure!(
                f.request
                    .max_row_width
                    .is_none_or(|n| path.len() <= n as usize),
                "Row exceeds width limit in tiles"
            );
            for &id in &path {
                ensure!(
                    used_belts.insert(id),
                    "Input belt reused by another row, ingredient or output"
                );
            }
            input_lines.push(path);
            input_rates[i] += port.rate;
        }
        let production = row.production_output.as_ref().unwrap_or(&row.output);
        if row.production_output.is_some() {
            ensure!(
                f.bus.as_ref().is_some_and(|b| b.compact),
                "Output collector requires a compact bus"
            );
            ensure!(
                production.item == row.output.item
                    && production.lanes == row.output.lanes
                    && production.rate == row.output.rate
                    && production.direction == row.output.direction,
                "Output collector changes material flow or lane assignment"
            );
        }
        let last = ids
            .get(&production.entity_number)
            .context("Missing production output")?;
        ensure!(
            last.position == production.position,
            "Production output coordinates mismatch"
        );
        let direction = if f.request.reverse_output { 12 } else { 4 };
        ensure!(
            row.output.direction == direction
                && last.direction == direction
                && ids[&row.output.entity_number].direction == direction,
            "Output port direction does not match the requested side"
        );
        if f.request.reverse_output {
            let boundary = f.external_inputs();
            ensure!(
                boundary
                    .iter()
                    .all(|p| p.direction == 4 && p.position.x == row.output.position.x),
                "Reversed inputs and outputs must share the west boundary"
            );
        }
        let first = ids
            .values()
            .filter(|e| {
                e.name == f.request.belt
                    && e.direction == direction
                    && e.position.y == last.position.y
            })
            .min_by(|a, b| {
                if direction == 12 {
                    b.position.x.total_cmp(&a.position.x)
                } else {
                    a.position.x.total_cmp(&b.position.x)
                }
            })
            .context("Missing output line")?;
        let output_line = walk(
            first.entity_number,
            Some(production.entity_number),
            Some(direction),
        )?;
        ensure!(
            output_line.last() == Some(&production.entity_number),
            "Output port is not the end of its belt"
        );
        if let Some(path) = output_paths.get(&row.output.entity_number) {
            ensure!(
                *path == output_line,
                "Shared output must use the same complete belt path"
            );
        } else {
            for &id in &output_line {
                ensure!(
                    used_belts.insert(id),
                    "Output belt overlaps another input/output path"
                );
            }
            if row.production_output.is_some() {
                let collector = walk(
                    production.entity_number,
                    Some(row.output.entity_number),
                    None,
                )?;
                ensure!(
                    collector.last() == Some(&row.output.entity_number),
                    "Disconnected output collector"
                );
                for &id in collector.iter().skip(1) {
                    ensure!(
                        used_belts.insert(id),
                        "Output collector overlaps another transport route"
                    );
                }
            }
            output_paths.insert(row.output.entity_number, output_line.clone());
        }
        let row_machines: BTreeSet<_> = row.machines.iter().copied().collect();
        ensure!(
            row_machines.len() == row.machines.len() && !row_machines.is_empty(),
            "Invalid row machine list"
        );
        for &id in &row.machines {
            ensure!(
                machines.contains(&id) && used_machines.insert(id),
                "Machine reused or missing"
            );
        }
        let mut inputs: Vec<HashMap<u32, usize>> = vec![HashMap::new(); requirements.len()];
        let mut outputs: HashMap<u32, usize> = HashMap::new();
        for &id in arms {
            if used_arms.contains(&id) {
                continue;
            }
            let arm = ids[&id];
            let reach = if arm.name == f.request.inserter { 1 } else { 2 };
            let dy = if arm.direction == 0 { reach } else { -reach };
            let src_tile = (
                arm.position.x.floor() as i32,
                arm.position.y.floor() as i32 - dy,
            );
            let dst_tile = (
                arm.position.x.floor() as i32,
                arm.position.y.floor() as i32 + dy,
            );
            if let (Some(src), Some(dst)) = (occupied.get(&src_tile), occupied.get(&dst_tile)) {
                for (i, path) in input_lines.iter().enumerate() {
                    if path.contains(src) && row_machines.contains(dst) {
                        ensure!(
                            arm.name == requirements[i].inserter,
                            "Ingredient is reached by the wrong inserter"
                        );
                        ensure!(
                            touches_tile(*dst, dst_tile)?,
                            "Inserter cannot reach the machine's collision box"
                        );
                        ensure!(used_arms.insert(id), "Inserter reused");
                        *inputs[i].entry(*dst).or_default() += 1;
                    }
                }
                if row_machines.contains(src) && output_line.contains(dst) {
                    ensure!(
                        arm.name == f.request.inserter,
                        "Output needs the primary inserter"
                    );
                    ensure!(
                        row.output.lanes
                            == if (dy == 1) != f.request.reverse_output {
                                "right"
                            } else {
                                "left"
                            },
                        "Output lane does not match the inserter's physical drop side"
                    );
                    ensure!(
                        touches_tile(*src, src_tile)?,
                        "Inserter cannot reach the machine's collision box"
                    );
                    ensure!(used_arms.insert(id), "Inserter reused");
                    *outputs.entry(*src).or_default() += 1;
                }
            }
        }
        for id in &row.machines {
            ensure!(
                inputs.iter().all(|by_machine| by_machine.contains_key(id))
                    && outputs.contains_key(id),
                "Machine #{id} lacks physical inserters for every ingredient/output"
            );
        }
        ensure!(
            row.output.rate <= belt_speed * 0.5 + 1e-8,
            "Row exceeds input belt or output-lane capacity"
        );
        for (i, port) in ports.iter().enumerate() {
            ensure!(
                port.rate <= belt_speed + 1e-8,
                "Ingredient input exceeds belt capacity"
            );
            ensure!(
                port.rate
                    <= inputs[i].values().sum::<usize>() as f64
                        * requirements[i].inserter_items_per_second
                        + 1e-6,
                "Row exceeds modelled inserter capacity for ingredient {}",
                port.item
            );
        }
        ensure!(
            row.output.rate
                <= outputs.values().sum::<usize>() as f64 * f.inserter_items_per_second + 1e-6,
            "Row exceeds modelled output inserter capacity"
        );
        output_rate += row.output.rate;
    }
    ensure!(
        used_belts == *belts && used_arms == *arms && used_machines == *machines,
        "Disconnected, unused or misdirected transport entity"
    );
    for port in f.output_ports() {
        ensure!(
            port.rate <= belt_speed + 1e-8,
            "Shared output exceeds full belt capacity"
        );
    }
    ensure!(
        output_rate >= f.request.rate
            && (output_rate - f.planned_output_per_second).abs() < 1e-6
            && input_rates
                .iter()
                .zip(&requirements)
                .all(|(rate, r)| (rate - r.supplied_per_second).abs() < 1e-6),
        "Factory flow totals do not meet the request for every ingredient"
    );
    Ok(())
}
