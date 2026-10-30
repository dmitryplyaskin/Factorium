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
    let walk = |start: u32| -> Result<Vec<u32>> {
        let mut path = Vec::new();
        let mut current = start;
        loop {
            let e = *ids.get(&current).context("Unknown belt port")?;
            ensure!(e.name == f.request.belt, "Port is not a belt");
            path.push(current);
            let next = (e.position.x.floor() as i32 + 1, e.position.y.floor() as i32);
            match occupied.get(&next).and_then(|id| ids.get(id)) {
                Some(e) if e.name == f.request.belt => current = e.entity_number,
                _ => break,
            }
        }
        Ok(path)
    };
    let mut used_belts = BTreeSet::new();
    let mut used_machines = BTreeSet::new();
    let mut used_arms = BTreeSet::new();
    let mut output_paths: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut output_lanes = BTreeSet::new();
    let mut output_rate = 0.0;
    let mut input_rates = vec![0.0; requirements.len()];
    for row in &f.rows {
        let ports: Vec<_> = row.input_ports().collect();
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
            let path = walk(port.entity_number)?;
            for &id in &path {
                ensure!(
                    used_belts.insert(id),
                    "Input belt reused by another row, ingredient or output"
                );
            }
            input_lines.push(path);
            input_rates[i] += port.rate;
        }
        let last = ids[&row.output.entity_number];
        let first = ids
            .values()
            .filter(|e| e.name == f.request.belt && e.position.y == last.position.y)
            .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .context("Missing output line")?;
        let output_line = walk(first.entity_number)?;
        ensure!(
            output_line.last() == Some(&row.output.entity_number),
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
                        row.output.lanes == if dy == 1 { "right" } else { "left" },
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
