//! Optional boundary bus, output storage and in-game input signs.

use super::{
    BeltPort, BusLine, FactoryRequest, GeneratedFactory, InputBus, InputMarker, OutputStorage,
    ProductionRow, entity, power::tiles,
};
use crate::{GameData, blueprint::BlueprintDocument, model::Entity};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) fn outputs(rows: &[ProductionRow]) -> Vec<BeltPort> {
    let mut ports: Vec<BeltPort> = Vec::new();
    for r in rows {
        if let Some(p) = ports
            .iter_mut()
            .find(|p| p.entity_number == r.output.entity_number)
        {
            p.rate += r.output.rate;
            if p.lanes != r.output.lanes {
                p.lanes = "both".into();
            }
        } else {
            ports.push(r.output.clone());
        }
    }
    ports
}

pub(super) fn matching<'a>(
    data: &'a GameData,
    belt: &Entity,
    kind: &str,
    size: [u32; 2],
) -> Result<&'a Entity> {
    let speed = &belt.prototype.properties["belt_speed"];
    data.entities
        .values()
        .find(|p| {
            !p.prototype.hidden
                && p.prototype.prototype_type == kind
                && [p.tile_width, p.tile_height] == size
                && p.prototype.properties.get("belt_speed") == Some(speed)
        })
        .with_context(|| format!("Common input needs a {kind} matching the selected belt speed"))
}

#[derive(Clone, Copy)]
pub(super) struct Rates {
    pub belt: f64,
    pub arm: f64,
    pub machine_output: f64,
}

pub(super) fn place(
    data: &GameData,
    request: &FactoryRequest,
    rows: &mut [ProductionRow],
    doc: &mut BlueprintDocument,
    rates: Rates,
) -> Result<(Option<InputBus>, Vec<OutputStorage>, Vec<InputMarker>)> {
    let bus = if request.common_input {
        Some(place_bus(data, request, rows, doc, rates.belt)?)
    } else {
        None
    };
    if let Some(bus) = &bus {
        if bus.compact {
            super::compact_bus::collect_outputs(data, request, rows, doc, bus)?;
        } else if request.reverse_output {
            return_outputs(request, rows, doc, bus)?;
        }
    }
    let storage = if let Some(chest) = &request.output_chest {
        place_storage(data, request, rows, doc, chest, rates)?
    } else {
        Vec::new()
    };
    let inputs: Vec<_> = bus
        .as_ref()
        .map(|b| b.lines.iter().map(|l| l.source.clone()).collect())
        .unwrap_or_else(|| {
            rows.iter()
                .flat_map(ProductionRow::input_ports)
                .cloned()
                .collect()
        });
    let markers = if request.input_markers {
        place_markers(data, doc, &inputs)?
    } else {
        Vec::new()
    };
    Ok((bus, storage, markers))
}

fn place_bus(
    data: &GameData,
    request: &FactoryRequest,
    rows: &[ProductionRow],
    doc: &mut BlueprintDocument,
    belt_rate: f64,
) -> Result<InputBus> {
    // Retain the previous router for catalogs whose underground equipment
    // cannot cross the compact corridors without collisions.
    let mut compact = doc.clone();
    if let Ok(bus) = super::compact_bus::place(data, request, rows, &mut compact, belt_rate) {
        *doc = compact;
        return Ok(bus);
    }
    let belt = entity(data, &request.belt)?;
    let splitter = matching(data, belt, "splitter", [2, 1])?;
    let underground = matching(data, belt, "underground-belt", [1, 1])?;
    ensure!(
        underground
            .prototype
            .properties
            .get("max_underground_distance")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 3,
        "Common input needs underground reach of at least three tiles"
    );
    // Keep the same row partitions for every ingredient. Independent packing
    // would give the two ingredients different splitter trees and could starve
    // a row even with enough aggregate supply.
    let mut partitions: Vec<Vec<&ProductionRow>> = Vec::new();
    for row in rows {
        let fits = partitions.last().is_some_and(|group| {
            row.input_ports().enumerate().all(|(j, p)| {
                group
                    .iter()
                    .map(|r| r.input_ports().nth(j).unwrap().rate)
                    .sum::<f64>()
                    + p.rate
                    <= belt_rate * 0.85 + 1e-8
            })
        });
        if !fits {
            partitions.push(Vec::new());
        }
        partitions.last_mut().unwrap().push(row);
    }
    let mut groups: Vec<Vec<BeltPort>> = Vec::new();
    for partition in partitions {
        for j in 0..rows[0].input_ports().count() {
            groups.push(
                partition
                    .iter()
                    .map(|r| r.input_ports().nth(j).unwrap().clone())
                    .collect(),
            );
        }
    }
    let depth = (groups
        .iter()
        .map(Vec::len)
        .max()
        .unwrap()
        .next_power_of_two())
    .ilog2() as i32;
    let start_y = groups
        .iter()
        .flatten()
        .map(|p| p.position.y.floor() as i32)
        .min()
        .unwrap()
        - 2 * depth
        - 4;
    let leaf_count: usize = groups.iter().map(Vec::len).sum();
    let all_columns: Vec<_> = (0..leaf_count).map(|i| -8 - 4 * i as i32).collect();
    let mut lines = Vec::new();
    let mut assigned = 0;
    for group in &mut groups {
        group.sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
        let x = -8 - 4 * (assigned + group.len() - 1) as i32;
        assigned += group.len();
        let source = doc.blueprint.entities.len() as u32 + 1;
        let mut route = BusTree {
            doc,
            belt: &request.belt,
            splitter: &splitter.prototype.name,
            underground: &underground.prototype.name,
            columns: &all_columns,
            added: Vec::new(),
        };
        route.branch(x, start_y, group);
        for (j, p) in group.iter().enumerate() {
            route.outlet(x + 4 * j as i32, p);
        }
        let added = route.added;
        lines.push(BusLine {
            source: BeltPort {
                entity_number: source,
                position: doc.blueprint.entities[(source - 1) as usize].position,
                item: group[0].item.clone(),
                rate: group.iter().map(|p| p.rate).sum(),
                lanes: "both".into(),
                direction: 8,
            },
            targets: group.iter().map(|p| p.entity_number).collect(),
            entities: added,
        });
    }
    if request.reverse_output {
        // The former north-facing sources become internal junctions. Nested
        // approaches above the tree expose every ingredient on the west edge.
        let west = all_columns.iter().min().unwrap() - 4;
        let count = lines.len() as i32;
        for (i, line) in lines.iter_mut().enumerate() {
            let x = line.source.position.x.floor() as i32;
            let y = start_y - 3 * (count - i as i32);
            let source = doc.blueprint.entities.len() as u32 + 1;
            for tx in west..=x {
                line.entities.push(doc.add(
                    &request.belt,
                    f64::from(tx) + 0.5,
                    f64::from(y) + 0.5,
                    if tx == x { 8 } else { 4 },
                ));
            }
            for ty in y + 1..start_y {
                line.entities.push(doc.add(
                    &request.belt,
                    f64::from(x) + 0.5,
                    f64::from(ty) + 0.5,
                    8,
                ));
            }
            line.source.entity_number = source;
            line.source.position = doc.blueprint.entities[(source - 1) as usize].position;
            line.source.direction = 4;
        }
    }
    Ok(InputBus {
        compact: false,
        splitter: splitter.prototype.name.clone(),
        underground: underground.prototype.name.clone(),
        lines,
    })
}

/// Cross input trunks underground so both lanes remain independent all the
/// way from the production rows to the west boundary.
fn return_outputs(
    request: &FactoryRequest,
    rows: &mut [ProductionRow],
    doc: &mut BlueprintDocument,
    bus: &InputBus,
) -> Result<()> {
    let west = bus.lines[0].source.position.x.floor() as i32;
    let occupied: HashSet<_> = doc
        .blueprint
        .entities
        .iter()
        .map(|e| (e.position.x.floor() as i32, e.position.y.floor() as i32))
        .collect();
    for port in outputs(rows) {
        let y = port.position.y.floor() as i32;
        let mut x = port.position.x.floor() as i32 - 1;
        let mut last = port.entity_number;
        while x >= west {
            if occupied.contains(&(x - 1, y)) {
                ensure!(x - 3 >= west, "Not enough room for an output crossing");
                for (tx, kind) in [(x, "input"), (x - 3, "output")] {
                    ensure!(
                        !occupied.contains(&(tx, y)),
                        "Output crossing overlaps an input"
                    );
                    last = doc.add(
                        &bus.underground,
                        f64::from(tx) + 0.5,
                        f64::from(y) + 0.5,
                        12,
                    );
                    doc.blueprint.entities[(last - 1) as usize].belt_type = Some(kind.into());
                }
                x -= 4;
            } else {
                ensure!(!occupied.contains(&(x, y)), "Output overlaps an input");
                last = doc.add(&request.belt, f64::from(x) + 0.5, f64::from(y) + 0.5, 12);
                x -= 1;
            }
        }
        for row in rows
            .iter_mut()
            .filter(|r| r.output.entity_number == port.entity_number)
        {
            row.output.entity_number = last;
            row.output.position = doc.blueprint.entities[(last - 1) as usize].position;
        }
    }
    Ok(())
}

/// A balanced splitter tree above the rows, with isolated vertical leaf trunks.
/// Every split halves a subtree instead of repeatedly halving the last rows.
struct BusTree<'a> {
    doc: &'a mut BlueprintDocument,
    belt: &'a str,
    splitter: &'a str,
    underground: &'a str,
    columns: &'a [i32],
    added: Vec<u32>,
}
impl BusTree<'_> {
    fn belt(&mut self, x: i32, y: i32, direction: u8) {
        self.added.push(
            self.doc
                .add(self.belt, f64::from(x) + 0.5, f64::from(y) + 0.5, direction),
        );
    }
    fn branch(&mut self, x: i32, y: i32, ports: &[BeltPort]) {
        self.belt(x, y, 8);
        if ports.len() == 1 {
            let end = ports[0].position.y.floor() as i32;
            for ty in y + 1..=end {
                self.belt(x, ty, if ty == end { 4 } else { 8 });
            }
            return;
        }
        let split_y = y + 1;
        self.added.push(self.doc.add(
            self.splitter,
            f64::from(x) + 1.0,
            f64::from(split_y) + 0.5,
            8,
        ));
        let half = ports.len() / 2;
        let right_x = x + 4 * half as i32;
        for tx in x + 1..right_x {
            self.belt(tx, split_y + 1, 4);
        }
        self.branch(x, split_y + 1, &ports[..half]);
        self.branch(right_x, split_y + 1, &ports[half..]);
    }
    fn outlet(&mut self, x: i32, port: &BeltPort) {
        let y = port.position.y.floor() as i32;
        let mut tx = x + 1;
        while tx < port.position.x.floor() as i32 {
            if self.columns.contains(&(tx + 1)) {
                for (px, kind) in [(tx, "input"), (tx + 3, "output")] {
                    let id =
                        self.doc
                            .add(self.underground, f64::from(px) + 0.5, f64::from(y) + 0.5, 4);
                    self.doc.blueprint.entities[(id - 1) as usize].belt_type = Some(kind.into());
                    self.added.push(id);
                }
                tx += 4;
            } else {
                self.belt(tx, y, 4);
                tx += 1;
            }
        }
    }
}

pub(super) fn storage_rate(
    request: &FactoryRequest,
    rows: &[ProductionRow],
    port: &BeltPort,
    belt: f64,
    machine_output: f64,
) -> f64 {
    if request.common_input {
        rows.iter()
            .filter(|r| r.output.entity_number == port.entity_number)
            .map(|r| r.machines.len() as f64 * machine_output)
            .sum::<f64>()
            .min(belt)
            .max(port.rate)
    } else {
        port.rate
    }
}

fn place_storage(
    data: &GameData,
    request: &FactoryRequest,
    rows: &[ProductionRow],
    doc: &mut BlueprintDocument,
    chest: &str,
    rates: Rates,
) -> Result<Vec<OutputStorage>> {
    let p = entity(data, chest)?;
    ensure!(
        p.prototype.prototype_type == "container" && p.tile_width == 1 && p.tile_height == 1,
        "Output storage needs a 1x1 ordinary chest"
    );
    let step = if request.reverse_output { -1 } else { 1 };
    let start_x = doc
        .blueprint
        .entities
        .iter()
        .filter(|e| e.name == request.belt)
        .map(|e| e.position.x.floor() as i32)
        .map(|x| x * step)
        .max()
        .unwrap()
        + 3;
    let mut storage = Vec::new();
    for port in outputs(rows) {
        let n = (storage_rate(request, rows, &port, rates.belt, rates.machine_output) / rates.arm)
            .ceil() as usize;
        ensure!(n <= 4096, "Output storage exceeds inserter budget");
        let y = port.position.y;
        let mut belts = Vec::new();
        let mut chests = Vec::new();
        let mut inserters = Vec::new();
        for x in port.position.x.floor() as i32 * step + 1..start_x + n as i32 {
            belts.push(doc.add(&request.belt, f64::from(x * step) + 0.5, y, port.direction));
        }
        for i in 0..n {
            let x = f64::from((start_x + i as i32) * step) + 0.5;
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            chests.push(doc.add(chest, x, y + 2.0 * side, 0));
            let arm = doc.add(
                &request.inserter,
                x,
                y + side,
                if side > 0.0 { 0 } else { 8 },
            );
            doc.blueprint.entities[(arm - 1) as usize].override_stack_size = Some(1);
            inserters.push(arm);
        }
        storage.push(OutputStorage {
            output_entity: port.entity_number,
            belts,
            inserters,
            chests,
        });
    }
    Ok(storage)
}

pub(super) fn marker_behavior(item: &str, rate: f64) -> Value {
    json!({"sections":{"sections":[{"index":1,"filters":[{"index":1,"type":"item","name":item,"quality":"normal","comparator":"=","count":rate.ceil() as i64}]}]}})
}

fn place_markers(
    data: &GameData,
    doc: &mut BlueprintDocument,
    inputs: &[BeltPort],
) -> Result<Vec<InputMarker>> {
    let sign = entity(data, "constant-combinator")?;
    ensure!(
        sign.tile_width == 1 && sign.tile_height == 1,
        "Input markers need 1x1 constant combinators"
    );
    let mut occupied = HashSet::new();
    for e in &doc.blueprint.entities {
        occupied.extend(tiles(e, entity(data, &e.name)?)?);
    }
    let mut markers = Vec::new();
    for p in inputs {
        let (px, py) = (p.position.x.floor() as i32, p.position.y.floor() as i32);
        let pos = [
            (-2, 0),
            (-2, -1),
            (-1, -2),
            (1, -2),
            (2, 0),
            (-3, 0),
            (0, -3),
        ]
        .into_iter()
        .map(|(x, y)| (px + x, py + y))
        .find(|p| !occupied.contains(p))
        .context("No free position for an input marker")?;
        occupied.insert(pos);
        let id = doc.add(
            "constant-combinator",
            f64::from(pos.0) + 0.5,
            f64::from(pos.1) + 0.5,
            0,
        );
        let e = &mut doc.blueprint.entities[(id - 1) as usize];
        e.control_behavior = Some(marker_behavior(&p.item, p.rate));
        e.player_description = Some(format!(
            "{}: {:.3} items/s; {} lane(s) relative to belt direction",
            p.item, p.rate, p.lanes
        ));
        markers.push(InputMarker {
            entity_number: id,
            target_entity: p.entity_number,
            item: p.item.clone(),
            rate: p.rate,
        });
    }
    Ok(markers)
}

/// Link actual inserter hand transitions to individual production rows. Storage
/// hands are deliberately excluded: they have no machine at either endpoint.
pub(super) fn observed_inserters(data: &GameData, f: &GeneratedFactory) -> Result<Vec<Value>> {
    let mut occupied = std::collections::HashMap::new();
    for e in &f.document.blueprint.entities {
        for tile in tiles(e, entity(data, &e.name)?)? {
            occupied.insert(tile, e.entity_number);
        }
    }
    let mut result = Vec::new();
    for e in &f.document.blueprint.entities {
        let p = entity(data, &e.name)?;
        if p.prototype.prototype_type != "inserter" {
            continue;
        }
        let reach = if e.name == f.request.inserter { 1 } else { 2 };
        let dy = if e.direction == 0 { reach } else { -reach };
        let tile = (e.position.x.floor() as i32, e.position.y.floor() as i32);
        let src = occupied.get(&(tile.0, tile.1 - dy));
        let dst = occupied.get(&(tile.0, tile.1 + dy));
        for (i, row) in f.rows.iter().enumerate() {
            if src.is_some_and(|id| row.machines.contains(id)) {
                result
                    .push(json!({"entity_number":e.entity_number,"row_index":i+1,"kind":"output"}));
            } else if dst.is_some_and(|id| row.machines.contains(id)) {
                let source = src.and_then(|id| {
                    f.document
                        .blueprint
                        .entities
                        .iter()
                        .find(|e| e.entity_number == *id)
                });
                if let Some((j, _)) = row
                    .input_ports()
                    .enumerate()
                    .find(|(_, port)| source.is_some_and(|e| e.position.y == port.position.y))
                {
                    result.push(json!({"entity_number":e.entity_number,"row_index":i+1,"kind":"input","input_index":j+1}));
                }
            }
        }
    }
    Ok(result)
}
