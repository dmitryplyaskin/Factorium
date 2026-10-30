//! Reconstruct physical topology from entities instead of trusting row labels.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use anyhow::{Context, Result, ensure};

use super::{
    GeneratedFactory, entity,
    power::{covered, distance, tiles},
    stat,
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
    let requirements = f.input_requirements();
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
            ensure!(
                e.direction == 4 || (f.request.common_input && matches!(e.direction, 0 | 8 | 12)),
                "Belt has unsupported direction"
            );
            belts.insert(e.entity_number);
        } else if f
            .bus
            .as_ref()
            .is_some_and(|b| e.name == b.splitter || e.name == b.underground)
        {
            belts.insert(e.entity_number);
        } else if e.name == f.request.inserter || requirements.iter().any(|r| r.inserter == e.name)
        {
            ensure!(
                proto.prototype.prototype_type == "inserter",
                "Input equipment is not an inserter"
            );
            ensure!(
                matches!(e.direction, 0 | 8) && e.override_stack_size == Some(1),
                "Inserter direction or stack-size override changed"
            );
            arms.insert(e.entity_number);
        } else if e.name == f.request.pole {
            ensure!(e.direction == 0, "Unsupported pole direction");
            poles.insert(e.entity_number, e);
        } else if Some(e.name.as_str()) == f.request.output_chest.as_deref()
            || (f.request.input_markers && e.name == "constant-combinator")
        {
            // Exact membership and geometry are checked by infrastructure_check.
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
    ensure!(
        graph[&entry.entity_number].len() < 5,
        "External power entry has no spare copper connection"
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
    super::transport::validate(data, f, &ids, &occupied, &machines, &belts, &arms)
}
