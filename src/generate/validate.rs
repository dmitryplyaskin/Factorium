//! Reconstruct physical topology from entities instead of trusting row labels.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use anyhow::{Context, Result, ensure};

use super::{
    GeneratedFactory, entity,
    power::{covered, distance, tiles},
    stat, supported_recipe,
};
use crate::GameData;

pub fn validate_factory(data: &GameData, f: &GeneratedFactory) -> Result<()> {
    ensure!(f.schema_version == 1, "Unsupported factory schema");
    ensure!(
        f.game_version == data.game_version && f.mods == data.mods,
        "Factory/catalog game version or mods mismatch"
    );
    ensure!(
        f.request.rate.is_finite() && f.request.rate > 0.0,
        "Invalid requested rate"
    );
    let entities = &f.document.blueprint.entities;
    ensure!(!entities.is_empty(), "Empty blueprint");
    let mut ids = BTreeMap::new();
    let mut occupied = HashMap::new();
    let mut poles = BTreeMap::new();
    let mut machines = BTreeSet::new();
    let mut belts = BTreeSet::new();
    let mut arms = BTreeSet::new();
    for e in entities {
        ensure!(
            e.entity_number > 0 && ids.insert(e.entity_number, e).is_none(),
            "Duplicate/invalid entity number"
        );
        let proto = entity(data, &e.name)?;
        for tile in tiles(e, proto)? {
            ensure!(
                occupied.insert(tile, e.entity_number).is_none(),
                "Overlapping entity at {tile:?}"
            );
        }
        if e.name == f.request.machine {
            ensure!(e.direction == 0, "Unsupported machine direction");
            if proto.prototype.prototype_type == "assembling-machine" {
                ensure!(
                    e.recipe.as_deref() == Some(f.recipe.as_str()),
                    "Wrong/missing machine recipe"
                );
            }
            machines.insert(e.entity_number);
        } else if e.name == f.request.belt {
            ensure!(e.direction == 4, "Belt must face east");
            belts.insert(e.entity_number);
        } else if e.name == f.request.inserter {
            ensure!(
                e.direction == 0 && e.override_stack_size == Some(1),
                "Inserter direction or stack-size override changed"
            );
            arms.insert(e.entity_number);
        } else if e.name == f.request.pole {
            ensure!(e.direction == 0, "Unsupported pole direction");
            poles.insert(e.entity_number, e);
        } else {
            anyhow::bail!("Unexpected entity in single-recipe layout: {}", e.name);
        }
    }
    ensure!(
        machines.len() == f.machine_count && !machines.is_empty(),
        "Machine count mismatch"
    );
    ensure!(
        !poles.is_empty() && poles.len() == f.power.pole_count,
        "Pole count mismatch"
    );
    let recipe = data
        .recipes
        .get(&f.recipe)
        .context("Unknown factory recipe")?;
    let (input, a, b) = supported_recipe(data, recipe, entity(data, &f.request.machine)?)?;
    ensure!(
        input == f.input_item && recipe.products[0].name == f.request.product,
        "Input/output item mismatch"
    );
    let mut graph: BTreeMap<u32, Vec<u32>> = poles.keys().map(|&id| (id, Vec::new())).collect();
    let mut wire_set = BTreeSet::new();
    for &[a, ac, b, bc] in &f.document.blueprint.wires {
        ensure!(
            ac == 5 && bc == 5 && a != b,
            "Invalid copper wire connectors"
        );
        let pa = poles.get(&a).context("Wire endpoint is not a pole")?;
        let pb = poles.get(&b).context("Wire endpoint is not a pole")?;
        let reach = stat(entity(data, &pa.name)?, "max_wire_distance")?
            .min(stat(entity(data, &pb.name)?, "max_wire_distance")?);
        ensure!(
            distance(pa.position, pb.position) <= reach + 1e-8,
            "Copper wire exceeds prototype reach"
        );
        ensure!(
            wire_set.insert((a.min(b), a.max(b))),
            "Duplicate copper wire"
        );
        graph.get_mut(&a).unwrap().push(b);
        graph.get_mut(&b).unwrap().push(a);
    }
    ensure!(
        graph.values().all(|edges| edges.len() <= 5),
        "Pole exceeds five copper connections"
    );
    let entry = poles
        .get(&f.power.connection_entity)
        .context("External power connector is not a pole")?;
    ensure!(
        entry.position == f.power.connection_position,
        "Power connector position mismatch"
    );
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([entry.entity_number]);
    while let Some(id) = queue.pop_front() {
        if seen.insert(id) {
            queue.extend(&graph[&id]);
        }
    }
    ensure!(seen.len() == poles.len(), "Disconnected pole network");
    for e in entities {
        let proto = entity(data, &e.name)?;
        if proto
            .prototype
            .properties
            .contains_key("electric_energy_source")
        {
            ensure!(
                poles.values().any(|pole| covered(
                    pole.position,
                    stat(entity(data, &pole.name).unwrap(), "supply_area_distance").unwrap_or(0.0),
                    e,
                    proto
                )),
                "Entity #{} ({}) has no power coverage",
                e.entity_number,
                e.name
            );
        }
    }
    let mut used_belts = BTreeSet::new();
    let mut used_machines = BTreeSet::new();
    let mut used_arms = BTreeSet::new();
    let touches_tile = |id: u32, tile: (i32, i32)| -> Result<bool> {
        let e = ids[&id];
        let box_ = entity(data, &e.name)?.collision_box;
        Ok(e.position.x + box_.right_bottom.x > f64::from(tile.0)
            && e.position.x + box_.left_top.x < f64::from(tile.0 + 1)
            && e.position.y + box_.right_bottom.y > f64::from(tile.1)
            && e.position.y + box_.left_top.y < f64::from(tile.1 + 1))
    };
    let walk = |start: u32| -> Result<Vec<u32>> {
        let mut list = Vec::new();
        let mut current = start;
        loop {
            let e = *ids.get(&current).context("Unknown belt port")?;
            ensure!(e.name == f.request.belt, "Port is not a belt");
            list.push(current);
            let tile = (e.position.x.floor() as i32 + 1, e.position.y.floor() as i32);
            match occupied.get(&tile).and_then(|id| ids.get(id)) {
                Some(next) if next.name == f.request.belt => current = next.entity_number,
                _ => break,
            }
        }
        Ok(list)
    };
    let mut output_rate = 0.0;
    let mut input_rate = 0.0;
    for row in &f.rows {
        ensure!(
            row.input.rate.is_finite()
                && row.output.rate.is_finite()
                && row.input.rate > 0.0
                && row.output.rate > 0.0,
            "Invalid row flow rate"
        );
        ensure!(
            row.input.item == f.input_item && row.output.item == f.request.product,
            "Row item mismatch"
        );
        ensure!(
            (row.input.rate - row.output.rate * a / b).abs() < 1e-6,
            "Row violates recipe material balance"
        );
        ensure!(
            ids.get(&row.input.entity_number)
                .is_some_and(|e| e.position == row.input.position)
                && ids
                    .get(&row.output.entity_number)
                    .is_some_and(|e| e.position == row.output.position),
            "Port coordinates mismatch"
        );
        let input_line = walk(row.input.entity_number)?;
        // Find the westmost belt on the output's row, then walk actual adjacency.
        let last = ids[&row.output.entity_number];
        let first = entities
            .iter()
            .filter(|e| e.name == f.request.belt && e.position.y == last.position.y)
            .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .context("Missing output line")?;
        let output_line = walk(first.entity_number)?;
        ensure!(
            output_line.last() == Some(&row.output.entity_number),
            "Output port is not the end of its belt"
        );
        for &id in input_line.iter().chain(&output_line) {
            ensure!(used_belts.insert(id), "Belt reused by multiple rows");
        }
        let row_machines: BTreeSet<_> = row.machines.iter().copied().collect();
        ensure!(
            row_machines.len() == row.machines.len() && !row_machines.is_empty(),
            "Invalid row machine list"
        );
        let mut inputs: HashMap<u32, usize> = HashMap::new();
        let mut outputs: HashMap<u32, usize> = HashMap::new();
        for &id in &row.machines {
            ensure!(
                machines.contains(&id) && used_machines.insert(id),
                "Machine reused or missing"
            );
        }
        for &id in &arms {
            let arm = ids[&id];
            let source = occupied.get(&(
                arm.position.x.floor() as i32,
                arm.position.y.floor() as i32 - 1,
            ));
            let dest = occupied.get(&(
                arm.position.x.floor() as i32,
                arm.position.y.floor() as i32 + 1,
            ));
            if let (Some(src), Some(dst)) = (source, dest) {
                if input_line.contains(src) && row_machines.contains(dst) {
                    ensure!(
                        touches_tile(
                            *dst,
                            (
                                arm.position.x.floor() as i32,
                                arm.position.y.floor() as i32 + 1
                            )
                        )?,
                        "Inserter cannot reach the machine's collision box"
                    );
                    ensure!(used_arms.insert(id), "Inserter reused");
                    *inputs.entry(*dst).or_default() += 1;
                }
                if row_machines.contains(src) && output_line.contains(dst) {
                    ensure!(
                        touches_tile(
                            *src,
                            (
                                arm.position.x.floor() as i32,
                                arm.position.y.floor() as i32 - 1
                            )
                        )?,
                        "Inserter cannot reach the machine's collision box"
                    );
                    ensure!(used_arms.insert(id), "Inserter reused");
                    *outputs.entry(*src).or_default() += 1;
                }
            }
        }
        for &id in &row.machines {
            ensure!(
                inputs.contains_key(&id) && outputs.contains_key(&id),
                "Machine #{id} lacks physical input/output inserters"
            );
        }
        let belt_speed = entity(data, &f.request.belt)?.prototype.properties["belt_speed"]
            .as_f64()
            .context("Missing belt speed")?
            * 480.0;
        ensure!(
            row.input.rate <= belt_speed + 1e-8 && row.output.rate <= belt_speed * 0.5 + 1e-8,
            "Row exceeds input belt or output-lane capacity"
        );
        let in_arms = inputs.values().sum::<usize>() as f64;
        let out_arms = outputs.values().sum::<usize>() as f64;
        ensure!(
            row.input.rate <= in_arms * f.inserter_items_per_second + 1e-6
                && row.output.rate <= out_arms * f.inserter_items_per_second + 1e-6,
            "Row exceeds modelled inserter capacity"
        );
        output_rate += row.output.rate;
        input_rate += row.input.rate;
    }
    ensure!(
        used_belts == belts && used_arms == arms && used_machines == machines,
        "Disconnected, unused or misdirected transport entity"
    );
    ensure!(
        output_rate >= f.request.rate
            && (output_rate - f.planned_output_per_second).abs() < 1e-6
            && (input_rate - f.supplied_input_per_second).abs() < 1e-6,
        "Factory flow totals do not meet the request"
    );
    Ok(())
}
