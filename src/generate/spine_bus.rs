//! Straight ingredient trunks with local taps and bounded, checked compaction.

use anyhow::{Result, ensure};

use super::{
    BeltPort, BusLine, FactoryRequest, GenerationControl, InputBus, ProductionRow,
    compact_bus::Router, entity, infrastructure::matching, infrastructure_check, route_cost,
};
use crate::{GameData, blueprint::BlueprintDocument};

struct Line {
    ports: Vec<BeltPort>,
}

struct Plan<'a> {
    data: &'a GameData,
    request: &'a FactoryRequest,
    rows: &'a [ProductionRow],
    lines: Vec<Line>,
    splitter: &'a str,
    underground: &'a str,
    capacity: f64,
}

impl Plan<'_> {
    fn build(
        &self,
        production: &BlueprintDocument,
        columns: &[i32],
    ) -> Result<(BlueprintDocument, InputBus)> {
        let mut doc = production.clone();
        let top = self
            .rows
            .iter()
            .flat_map(ProductionRow::input_ports)
            .map(|p| p.position.y.floor() as i32)
            .min()
            .unwrap();
        let west = *columns.iter().min().unwrap() - 2;
        let mut router = Router::new(self.data, &mut doc, &self.request.belt, self.underground)?;
        let mut lines = Vec::new();
        let mut outlets = Vec::new();
        for (i, (line, &x)) in self.lines.iter().zip(columns).enumerate() {
            let begin = router.doc.blueprint.entities.len();
            let first_y = line.ports[0].position.y.floor() as i32;
            let header_y = top + i as i32;
            if header_y == first_y {
                router.run((west, header_y), (x - 1, first_y), 4, 4)?;
            } else {
                let direction = if header_y < first_y { 8 } else { 0 };
                let step = if header_y < first_y { 1 } else { -1 };
                router.run((west, header_y), (x - 1, header_y), 4, direction)?;
                router.run((x - 1, header_y + step), (x - 1, first_y), direction, 4)?;
            }
            let mut leaves = Vec::new();
            if line.ports.len() == 1 {
                router.belt(x, first_y, 4)?;
                leaves.push((x, line.ports[0].clone()));
            } else {
                // First tap also turns the header into the straight southbound trunk.
                router.splitter(x, first_y, 4, self.splitter)?;
                router.belt(x + 1, first_y, 4)?;
                leaves.push((x + 1, line.ports[0].clone()));
                let mut previous = first_y + 1;
                for (j, port) in line.ports.iter().enumerate().skip(1) {
                    let y = port.position.y.floor() as i32;
                    if j + 1 == line.ports.len() {
                        router.run((x + 1, previous), (x + 1, y), 8, 4)?;
                        leaves.push((x + 1, port.clone()));
                    } else {
                        ensure!(previous <= y - 2, "Insufficient room between bus taps");
                        router.run((x + 1, previous), (x + 1, y - 2), 8, 8)?;
                        router.splitter(x + 1, y - 1, 8, self.splitter)?;
                        router.belt(x + 2, y, 4)?;
                        leaves.push((x + 2, port.clone()));
                    }
                    previous = y;
                }
            }
            let source = begin as u32 + 1;
            lines.push(BusLine {
                source: BeltPort {
                    entity_number: source,
                    position: router.doc.blueprint.entities[begin].position,
                    item: line.ports[0].item.clone(),
                    rate: line.ports.iter().map(|p| p.rate).sum(),
                    lanes: line.ports[0].lanes.clone(),
                    direction: 4,
                },
                targets: line.ports.iter().map(|p| p.entity_number).collect(),
                entities: (source..=router.doc.blueprint.entities.len() as u32).collect(),
            });
            outlets.push(leaves);
        }
        // Cross the complete set of trunks with underground belts. Every tap
        // approaches its row directly from the west and preserves both lanes.
        for (i, leaves) in outlets.iter().enumerate() {
            for (x, port) in leaves {
                let begin = router.doc.blueprint.entities.len() as u32 + 1;
                let y = port.position.y.floor() as i32;
                let end = port.position.x.floor() as i32 - 1;
                if *x < end {
                    router.run((x + 1, y), (end, y), 4, 4)?;
                }
                lines[i]
                    .entities
                    .extend(begin..=router.doc.blueprint.entities.len() as u32);
            }
        }
        let mut shared = Vec::new();
        for line in &lines {
            if line.source.lanes == "both" {
                continue;
            }
            let peers: Vec<_> = self
                .rows
                .iter()
                .flat_map(ProductionRow::input_ports)
                .filter(|p| line.targets.contains(&p.entity_number) && p.item != line.source.item)
                .collect();
            if !peers.is_empty() {
                let mut other = line.clone();
                other.source.item = peers[0].item.clone();
                other.source.lanes = peers[0].lanes.clone();
                other.source.rate = peers.iter().map(|p| p.rate).sum();
                shared.push(other);
            }
        }
        lines.extend(shared);
        let bus = InputBus {
            compact: true,
            splitter: self.splitter.into(),
            underground: self.underground.into(),
            lines,
            output_entities: Vec::new(),
        };
        infrastructure_check::validate_bus(
            self.data,
            self.request,
            self.rows,
            &doc,
            &bus,
            self.capacity,
        )?;
        Ok((doc, bus))
    }
}

pub(super) fn place(
    data: &GameData,
    request: &FactoryRequest,
    rows: &[ProductionRow],
    doc: &mut BlueprintDocument,
    capacity: f64,
    control: &GenerationControl,
    refine: bool,
) -> Result<InputBus> {
    control.check()?;
    let belt = entity(data, &request.belt)?;
    let splitter = matching(data, belt, "splitter", [2, 1])?;
    let underground = matching(data, belt, "underground-belt", [1, 1])?;
    let mut partitions: Vec<Vec<&ProductionRow>> = Vec::new();
    for row in rows {
        let fits = partitions.last().is_some_and(|group| {
            row.input_ports().enumerate().all(|(j, p)| {
                group
                    .iter()
                    .map(|r| r.input_ports().nth(j).unwrap().rate)
                    .sum::<f64>()
                    + p.rate
                    <= capacity * if p.lanes == "both" { 0.85 } else { 0.425 } + 1e-8
            })
        });
        if !fits {
            partitions.push(Vec::new());
        }
        partitions.last_mut().unwrap().push(row);
    }
    let mut lines = Vec::new();
    for partition in partitions {
        let mut ports: Vec<_> = partition[0].input_ports().collect();
        ports.sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
        ports.dedup_by_key(|p| p.entity_number);
        for first in ports {
            let mut targets: Vec<_> = partition
                .iter()
                .map(|r| {
                    r.input_ports()
                        .find(|p| p.item == first.item)
                        .unwrap()
                        .clone()
                })
                .collect();
            targets.sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
            lines.push(Line { ports: targets });
        }
    }
    lines.sort_by(|a, b| a.ports[0].position.y.total_cmp(&b.ports[0].position.y));
    let plan = Plan {
        data,
        request,
        rows,
        lines,
        splitter: &splitter.prototype.name,
        underground: &underground.prototype.name,
        capacity,
    };
    let right = super::compact_bus::right_boundary(request, rows, capacity) - 2;
    let production_len = doc.blueprint.entities.len();
    // Start with enough room to cross each trunk separately. Then try removing
    // gaps, rebuilding and independently checking every candidate before use.
    let mut columns: Vec<_> = (0..plan.lines.len())
        .map(|i| right - 4 * i as i32)
        .collect();
    let mut best = plan.build(doc, &columns)?;
    if !refine || columns.len() == 1 {
        control.check()?;
        *doc = best.0;
        return Ok(best.1);
    }
    let mut score = route_cost::measure(data, &best.0.blueprint.entities[production_len..])?;
    // Keep reconstruction work bounded on large factories as well as bounding
    // the number of passes. Global spacing trials compact many trunks at once.
    let budget = (100_000 / best.0.blueprint.entities.len().max(1)).clamp(3, 24);
    let mut attempts = 0;
    for pitch in [3, 2, 1] {
        control.check()?;
        let candidate: Vec<_> = (0..columns.len())
            .map(|i| right - pitch * i as i32)
            .collect();
        attempts += 1;
        if let Ok(result) = plan.build(doc, &candidate) {
            let next = route_cost::measure(data, &result.0.blueprint.entities[production_len..])?;
            if next < score {
                columns = candidate;
                score = next;
                best = result;
            }
        }
    }
    for _ in 0..3 {
        let mut improved = false;
        for gap in 1..columns.len() {
            if attempts >= budget {
                break;
            }
            control.check()?;
            if columns[gap - 1] - columns[gap] <= 1 {
                continue;
            }
            let mut candidate = columns.clone();
            for x in &mut candidate[gap..] {
                *x += 1;
            }
            attempts += 1;
            if let Ok(result) = plan.build(doc, &candidate) {
                let next =
                    route_cost::measure(data, &result.0.blueprint.entities[production_len..])?;
                if next < score {
                    columns = candidate;
                    score = next;
                    best = result;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
    control.check()?;
    *doc = best.0;
    Ok(best.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Result<(
        GameData,
        FactoryRequest,
        Vec<ProductionRow>,
        BlueprintDocument,
    )> {
        let mut data: GameData =
            serde_json::from_str(include_str!("../../tests/fixtures/catalog.json"))?;
        for (name, kind, width) in [
            ("transport-belt", "transport-belt", 1),
            ("splitter", "splitter", 2),
            ("underground-belt", "underground-belt", 1),
        ] {
            let box_ = json!({"left_top":{"x":-0.4,"y":-0.4},"right_bottom":{"x":0.4,"y":0.4}});
            data.entities.insert(
                name.into(),
                serde_json::from_value(json!({
                    "name":name,"type":kind,"hidden":false,"tile_width":width,"tile_height":1,
                    "collision_box":box_,"selection_box":box_,"fluid_boxes":[],"quality_stats":{},
                    "belt_speed":0.03125,"max_underground_distance":5
                }))?,
            );
        }
        let mut request = FactoryRequest::new("product", 1.0);
        request.common_input = true;
        let mut doc = BlueprintDocument::new("2.1.24", "test".into())?;
        let mut rows = Vec::new();
        for y in [0.5, 8.5, 16.5] {
            let mut ports = Vec::new();
            for (item, offset) in [("a", 0.0), ("b", 1.0)] {
                let id = doc.add("transport-belt", -1.5, y + offset, 4);
                ports.push(BeltPort {
                    entity_number: id,
                    position: doc.blueprint.entities[(id - 1) as usize].position,
                    item: item.into(),
                    rate: 1.0,
                    lanes: "both".into(),
                    direction: 4,
                });
            }
            rows.push(ProductionRow {
                machines: Vec::new(),
                input: ports[0].clone(),
                additional_inputs: vec![ports[1].clone()],
                output: ports[0].clone(),
                production_output: None,
            });
        }
        Ok((data, request, rows, doc))
    }

    #[test]
    fn compaction_reduces_transport_cost_and_preserves_independent_routes() -> Result<()> {
        let (data, request, rows, mut doc) = fixture()?;
        let lines = (0..2)
            .map(|j| Line {
                ports: rows
                    .iter()
                    .map(|r| r.input_ports().nth(j).unwrap().clone())
                    .collect(),
            })
            .collect();
        let plan = Plan {
            data: &data,
            request: &request,
            rows: &rows,
            lines,
            splitter: "splitter",
            underground: "underground-belt",
            capacity: 15.0,
        };
        let production_len = doc.blueprint.entities.len();
        let initial = plan.build(&doc, &[-7, -11])?;
        let before = route_cost::measure(&data, &initial.0.blueprint.entities[production_len..])?;
        assert!(
            plan.build(&doc, &[-7, -8]).is_err(),
            "unsafe gap must be rejected"
        );
        let bus = place(
            &data,
            &request,
            &rows,
            &mut doc,
            15.0,
            &GenerationControl::default(),
            true,
        )?;
        let after = route_cost::measure(&data, &doc.blueprint.entities[production_len..])?;
        assert!(after.cost < before.cost, "{before:?} -> {after:?}");
        assert!(after.area < before.area);
        infrastructure_check::validate_bus(&data, &request, &rows, &doc, &bus, 15.0)?;
        Ok(())
    }

    #[test]
    fn cancelled_optimization_leaves_the_production_document_unchanged() -> Result<()> {
        let (data, request, rows, mut doc) = fixture()?;
        let before = doc.encode()?;
        let control = GenerationControl::default();
        control.cancel();
        assert!(place(&data, &request, &rows, &mut doc, 15.0, &control, true).is_err());
        assert_eq!(doc.encode()?, before);
        Ok(())
    }

    #[test]
    fn independent_capacity_groups_pack_together_without_empty_columns() -> Result<()> {
        let (data, request, rows, mut doc) = fixture()?;
        let bus = place(
            &data,
            &request,
            &rows,
            &mut doc,
            1.5,
            &GenerationControl::default(),
            true,
        )?;
        assert_eq!(bus.lines.len(), 6);
        let west = bus.lines[0].source.position.x;
        assert!(bus.lines.iter().all(|line| line.source.position.x == west));
        assert!(rows[0].input.position.x - west <= 12.0);
        infrastructure_check::validate_bus(&data, &request, &rows, &doc, &bus, 1.5)?;
        Ok(())
    }
}
