//! Independently reconstruct optional infrastructure from physical connections.

use super::{
    FactoryRequest, GeneratedFactory, InputBus, ProductionRow, entity,
    infrastructure::marker_behavior, power::tiles,
};
use crate::{
    GameData,
    blueprint::{BlueprintDocument, BlueprintEntity},
};
use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

#[derive(Default)]
pub(super) struct Used {
    pub belts: BTreeSet<u32>,
    pub arms: BTreeSet<u32>,
}

fn delta(direction: u8) -> Result<(i32, i32)> {
    match direction {
        0 => Ok((0, -1)),
        4 => Ok((1, 0)),
        8 => Ok((0, 1)),
        12 => Ok((-1, 0)),
        _ => bail!("Invalid transport direction"),
    }
}

pub(super) fn validate_bus(
    data: &GameData,
    request: &FactoryRequest,
    rows: &[ProductionRow],
    doc: &BlueprintDocument,
    bus: &InputBus,
    belt_rate: f64,
) -> Result<()> {
    let mut occupied = HashMap::new();
    let mut ids = BTreeMap::new();
    for e in &doc.blueprint.entities {
        ids.insert(e.entity_number, e);
        for tile in tiles(e, entity(data, &e.name)?)? {
            ensure!(
                occupied.insert(tile, e.entity_number).is_none(),
                "Overlapping input route"
            );
        }
    }
    validate_inputs(data, request, rows, Some(bus), belt_rate, &ids, &occupied)?;
    Ok(())
}

fn validate_inputs(
    data: &GameData,
    request: &FactoryRequest,
    rows: &[ProductionRow],
    bus: Option<&InputBus>,
    belt_rate: f64,
    ids: &BTreeMap<u32, &BlueprintEntity>,
    occupied: &HashMap<(i32, i32), u32>,
) -> Result<BTreeSet<u32>> {
    let mut used = Used::default();
    ensure!(
        request.common_input == bus.is_some(),
        "Common input option/bus mismatch"
    );
    let row_ports: HashMap<_, _> = rows
        .iter()
        .flat_map(ProductionRow::input_ports)
        .map(|p| ((p.entity_number, p.item.as_str()), p))
        .collect();
    let mut claimed = BTreeSet::new();
    if let Some(bus) = bus {
        let split = entity(data, &bus.splitter)?;
        let ug = entity(data, &bus.underground)?;
        ensure!(
            split.prototype.prototype_type == "splitter"
                && [split.tile_width, split.tile_height] == [2, 1],
            "Invalid bus splitter"
        );
        ensure!(
            ug.prototype.prototype_type == "underground-belt"
                && [ug.tile_width, ug.tile_height] == [1, 1],
            "Invalid bus underground belt"
        );
        let speed = entity(data, &request.belt)?
            .prototype
            .properties
            .get("belt_speed");
        ensure!(
            split.prototype.properties.get("belt_speed") == speed
                && ug.prototype.properties.get("belt_speed") == speed,
            "Bus equipment speed mismatch"
        );
        let reach = ug
            .prototype
            .properties
            .get("max_underground_distance")
            .and_then(|v| v.as_u64())
            .context("Missing underground reach")? as i32;
        for line in &bus.lines {
            let route: HashSet<_> = line.entities.iter().copied().collect();
            ensure!(
                route.len() == line.entities.len() && route.contains(&line.source.entity_number),
                "Invalid bus entity list/source"
            );
            ensure!(
                line.source.rate.is_finite()
                    && line.source.rate > 0.0
                    && line.source.rate
                        <= belt_rate
                            * if line.source.lanes == "both" {
                                0.85
                            } else {
                                0.425
                            }
                            + 1e-8,
                "Common input exceeds belt capacity"
            );
            let source = ids
                .get(&line.source.entity_number)
                .context("Missing bus inlet")?;
            ensure!(
                source.name == request.belt
                    && source.direction
                        == if bus.compact || request.reverse_output {
                            4
                        } else {
                            8
                        }
                    && line.source.direction == source.direction
                    && source.position == line.source.position
                    && matches!(line.source.lanes.as_str(), "both" | "left" | "right"),
                "Wrong common input position/direction"
            );
            let mut demand = 0.0;
            let targets: HashSet<_> = line.targets.iter().copied().collect();
            ensure!(
                targets.len() == line.targets.len() && !targets.is_empty(),
                "Invalid bus target list"
            );
            for target in &line.targets {
                let p = row_ports
                    .get(&(*target, line.source.item.as_str()))
                    .context("Bus target is not a row input")?;
                ensure!(
                    claimed.insert((*target, p.item.as_str()))
                        && p.item == line.source.item
                        && p.lanes == line.source.lanes,
                    "Bus duplicates a row input or mixes ingredients"
                );
                demand += p.rate;
            }
            ensure!(
                (demand - line.source.rate).abs() < 1e-6,
                "Bus flow does not match row demands"
            );
            let mut queue = VecDeque::from([line.source.entity_number]);
            let mut seen = HashSet::new();
            let mut incoming = HashMap::new();
            while let Some(id) = queue.pop_front() {
                if !seen.insert(id) {
                    continue;
                }
                if targets.contains(&id) {
                    continue;
                }
                ensure!(route.contains(&id), "Bus crosses another input line");
                let e = *ids.get(&id).context("Missing bus entity")?;
                let mut next = Vec::new();
                let mut paired = false;
                if e.name == bus.underground && e.belt_type.as_deref() == Some("input") {
                    let (dx, dy) = delta(e.direction)?;
                    for n in 1..=reach {
                        if let Some(other) = occupied
                            .get(&(
                                e.position.x.floor() as i32 + n * dx,
                                e.position.y.floor() as i32 + n * dy,
                            ))
                            .and_then(|id| ids.get(id))
                        {
                            if other.name == bus.underground
                                && other.direction == e.direction
                                && other.belt_type.as_deref() == Some("output")
                            {
                                next.push(other.entity_number);
                                break;
                            }
                        }
                    }
                    ensure!(!next.is_empty(), "Disconnected underground bus crossing");
                    paired = true;
                } else {
                    let positions = if e.name == bus.splitter {
                        if bus.compact {
                            ensure!(
                                matches!(e.direction, 4 | 8),
                                "Unsupported compact bus splitter direction"
                            );
                            if e.direction == 4 {
                                let x = e.position.x.floor() as i32;
                                let y = (e.position.y - 1.0).round() as i32;
                                vec![(x + 1, y), (x + 1, y + 1)]
                            } else {
                                let x = (e.position.x - 1.0).round() as i32;
                                let y = e.position.y.floor() as i32;
                                vec![(x, y + 1), (x + 1, y + 1)]
                            }
                        } else {
                            ensure!(e.direction == 8, "Bus splitter must face south");
                            let x = (e.position.x - 1.0).round() as i32;
                            let y = e.position.y.floor() as i32;
                            vec![(x, y + 1), (x + 1, y + 1)]
                        }
                    } else {
                        ensure!(
                            e.name == request.belt
                                || (e.name == bus.underground
                                    && e.belt_type.as_deref() == Some("output")),
                            "Unexpected entity on input bus"
                        );
                        let (dx, dy) = delta(e.direction)?;
                        vec![(
                            e.position.x.floor() as i32 + dx,
                            e.position.y.floor() as i32 + dy,
                        )]
                    };
                    for tile in positions {
                        let target = *occupied.get(&tile).context("Disconnected input bus belt")?;
                        let to = ids[&target];
                        ensure!(
                            route.contains(&target) || targets.contains(&target),
                            "Input bus leaks into another belt/entity"
                        );
                        if to.name == request.belt {
                            ensure!(
                                to.direction != (e.direction + 8) % 16,
                                "Input bus enters the front of a belt"
                            );
                            if targets.contains(&target) {
                                ensure!(
                                    e.direction == 4 && to.direction == 4,
                                    "Row inlet is not fed from the west"
                                );
                            }
                        } else {
                            ensure!(
                                to.direction == e.direction,
                                "Bus feeds splitter/underground from the wrong side"
                            );
                        }
                        next.push(target);
                    }
                }
                for target in next {
                    if paired {
                        ensure!(
                            route.contains(&target),
                            "Underground belt pairs with another bus line"
                        );
                    }
                    ensure!(
                        incoming.insert(target, id).is_none(),
                        "Bus merges lanes/lines or forms a cycle"
                    );
                    queue.push_back(target);
                }
            }
            ensure!(
                route.iter().all(|id| seen.contains(id))
                    && targets.iter().all(|id| seen.contains(id)),
                "Unreachable bus branches or unused transport entities"
            );
            let shared = bus
                .lines
                .iter()
                .take_while(|other| !std::ptr::eq(*other, line))
                .find(|other| other.source.entity_number == line.source.entity_number);
            if let Some(other) = shared {
                ensure!(
                    other.source.item != line.source.item
                        && matches!(
                            (other.source.lanes.as_str(), line.source.lanes.as_str()),
                            ("left", "right") | ("right", "left")
                        )
                        && other.entities == line.entities
                        && other.targets == line.targets,
                    "Shared bus must preserve two distinct lanes over the same complete route"
                );
            } else {
                for id in route {
                    ensure!(used.belts.insert(id), "Bus entity reused by another line");
                }
            }
        }
        ensure!(
            claimed.len() == row_ports.len(),
            "Common input does not supply every ingredient/row"
        );
    }
    Ok(used.belts)
}

pub(super) fn validate(
    data: &GameData,
    f: &GeneratedFactory,
    ids: &BTreeMap<u32, &BlueprintEntity>,
    occupied: &HashMap<(i32, i32), u32>,
) -> Result<Used> {
    let mut used = Used {
        belts: validate_inputs(
            data,
            &f.request,
            &f.rows,
            f.bus.as_ref(),
            f.belt_items_per_second,
            ids,
            occupied,
        )?,
        ..Used::default()
    };
    let outputs = f.output_ports();
    let mut stored = BTreeSet::new();
    let mut chest_ids = BTreeSet::new();
    ensure!(
        f.storage.is_empty() == f.request.output_chest.is_none(),
        "Output chest option/storage mismatch"
    );
    for bank in &f.storage {
        let port = outputs
            .iter()
            .find(|p| p.entity_number == bank.output_entity)
            .context("Storage source is not an output port")?;
        ensure!(stored.insert(port.entity_number), "Duplicate storage bank");
        ensure!(
            !bank.chests.is_empty() && bank.chests.len() == bank.inserters.len(),
            "Storage needs chests and inserters"
        );
        ensure!(
            bank.inserters.len() as f64
                * super::inserter::rates(
                    entity(data, &f.request.inserter)?,
                    entity(data, &f.request.belt)?,
                )?
                .input(&port.lanes)
                + 1e-8
                >= super::infrastructure::storage_rate(
                    &f.request,
                    &f.rows,
                    port,
                    f.belt_items_per_second,
                    f.machine_crafts_per_second
                        * super::supported_recipe(
                            data,
                            &data.recipes[&f.recipe],
                            entity(data, &f.request.machine)?
                        )?
                        .output
                ),
            "Output chest inserters cannot handle requested throughput"
        );
        let mut last = port.position;
        let step = if f.request.reverse_output { -1.0 } else { 1.0 };
        let bank_belts: HashSet<_> = bank.belts.iter().copied().collect();
        ensure!(
            bank_belts.len() == bank.belts.len(),
            "Duplicate storage belt"
        );
        for &id in &bank.belts {
            let e = ids.get(&id).context("Missing storage belt")?;
            ensure!(
                e.name == f.request.belt
                    && e.direction == port.direction
                    && e.position.y == last.y
                    && e.position.x == last.x + step,
                "Broken storage conveyor"
            );
            ensure!(used.belts.insert(id), "Storage belt overlaps another route");
            last = e.position;
        }
        let mut targets = BTreeSet::new();
        for (&arm_id, &chest_id) in bank.inserters.iter().zip(&bank.chests) {
            let arm = ids.get(&arm_id).context("Missing storage inserter")?;
            let chest = ids.get(&chest_id).context("Missing output chest")?;
            ensure!(
                Some(chest.name.as_str()) == f.request.output_chest.as_deref()
                    && chest_ids.insert(chest_id),
                "Wrong or reused output chest"
            );
            let p = entity(data, &chest.name)?;
            ensure!(
                p.prototype.prototype_type == "container"
                    && p.tile_width == 1
                    && p.tile_height == 1,
                "Unsupported output chest"
            );
            ensure!(
                arm.name == f.request.inserter
                    && matches!(arm.direction, 0 | 8)
                    && arm.override_stack_size == Some(1),
                "Invalid storage inserter"
            );
            let dy = if arm.direction == 0 { 1.0 } else { -1.0 };
            ensure!(
                chest.position.x == arm.position.x && chest.position.y == arm.position.y + dy,
                "Storage inserter misses its chest"
            );
            let source = occupied.get(&(
                arm.position.x.floor() as i32,
                (arm.position.y - dy).floor() as i32,
            ));
            ensure!(
                source.is_some_and(|id| bank_belts.contains(id)),
                "Storage inserter cannot pick from its output belt"
            );
            ensure!(
                used.arms.insert(arm_id) && targets.insert(chest_id),
                "Storage inserter/chest reused"
            );
        }
    }
    if f.request.output_chest.is_some() {
        ensure!(
            stored.len() == outputs.len(),
            "Some output belts have no chest bank"
        );
    }
    let all_chests: BTreeSet<_> = ids
        .values()
        .filter(|e| Some(e.name.as_str()) == f.request.output_chest.as_deref())
        .map(|e| e.entity_number)
        .collect();
    ensure!(all_chests == chest_ids, "Unused output chest");
    let boundary = f.external_inputs();
    let mut marked = BTreeSet::new();
    let mut signs = BTreeSet::new();
    for sign in &f.markers {
        let port = boundary
            .iter()
            .find(|p| p.entity_number == sign.target_entity && p.item == sign.item)
            .context("Marker targets an internal/unknown port")?;
        let e = ids
            .get(&sign.entity_number)
            .context("Missing input marker")?;
        ensure!(
            f.request.input_markers
                && marked.insert((port.entity_number, port.item.as_str()))
                && signs.insert(e.entity_number),
            "Duplicate or unexpected input marker"
        );
        ensure!(
            sign.item == port.item
                && (sign.rate - port.rate).abs() < 1e-8
                && e.name == "constant-combinator"
                && e.control_behavior.as_ref() == Some(&marker_behavior(&sign.item, sign.rate)),
            "Input marker signal differs from requested item/rate"
        );
        ensure!(
            (e.position.x - port.position.x).abs() <= 3.0
                && (e.position.y - port.position.y).abs() <= 3.0,
            "Marker is too far from its input"
        );
    }
    ensure!(
        marked.len()
            == if f.request.input_markers {
                boundary.len()
            } else {
                0
            },
        "Not every inlet has a marker"
    );
    ensure!(
        ids.values()
            .filter(|e| e.name == "constant-combinator")
            .count()
            == signs.len(),
        "Unused constant combinator"
    );
    Ok(used)
}
