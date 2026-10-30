//! Capacity-limited output collectors alongside the production rows.
use anyhow::Result;

use super::{
    BeltPort, FactoryRequest, InputBus, ProductionRow, compact_bus::Router, infrastructure::outputs,
};
use crate::{GameData, blueprint::BlueprintDocument};

pub(super) fn place(
    data: &GameData,
    request: &FactoryRequest,
    rows: &mut [ProductionRow],
    doc: &mut BlueprintDocument,
    bus: &mut InputBus,
    capacity: f64,
) -> Result<()> {
    let groups = groups(rows, capacity);
    if groups.len() == 1 && groups[0].len() == 1 && !request.reverse_output {
        return Ok(());
    }
    let balance_groups: Vec<_> = groups
        .iter()
        .map(|g| needs_balance(rows, g, capacity))
        .collect();
    let corridor = if balance_groups.iter().any(|&b| b) {
        7
    } else {
        3
    };
    let west = bus
        .lines
        .iter()
        .map(|l| l.source.position.x.floor() as i32)
        .min()
        .unwrap();
    let step = if request.reverse_output { -1 } else { 1 };
    let direction = if request.reverse_output { 12 } else { 4 };
    let edge = doc
        .blueprint
        .entities
        .iter()
        .map(|e| e.position.x.floor() as i32 * step)
        .max()
        .unwrap();
    let feed = if request.reverse_output { 3 } else { edge + 2 };
    let boundary = if request.reverse_output {
        -west
    } else {
        feed + corridor + 2 + 4 * groups.len() as i32
    };
    let begin = doc.blueprint.entities.len() as u32 + 1;
    let mut route = Router::new(data, doc, &request.belt, &bus.underground)?;
    for (i, group) in groups.iter().enumerate() {
        let column = feed + corridor + 4 * (groups.len() - 1 - i) as i32;
        let mut previous_y = None;
        let mut last = 0;
        for (j, port) in group.iter().enumerate() {
            let x = port.position.x.floor() as i32;
            let y = port.position.y.floor() as i32;
            if group.len() == 1 {
                last = route.run((x + step, y), (boundary * step, y), direction, direction)?;
                continue;
            }
            route.run((x + step, y), (feed * step, y), direction, 0)?;
            route.belt(feed * step, y - 1, 0)?;
            route.belt(feed * step, y - 2, direction)?;
            let start = if balance_groups[i] {
                // Split evenly and side-load from opposite sides to use both lanes.
                route.splitter((feed + 1) * step, y - 2, direction, &bus.splitter)?;
                for (u, ty, d) in [
                    (feed + 2, y - 2, 0),
                    (feed + 2, y - 3, direction),
                    (feed + 3, y - 3, direction),
                    (feed + 4, y - 3, 8),
                    (feed + 2, y - 1, 8),
                    (feed + 2, y, direction),
                    (feed + 3, y, direction),
                    (feed + 4, y, 0),
                    (feed + 4, y - 1, 0),
                    (feed + 4, y - 2, direction),
                ] {
                    route.belt(u * step, ty, d)?;
                }
                feed + 5
            } else {
                feed + 1
            };
            route.run(
                (start * step, y - 2),
                ((column - 1) * step, y - 2),
                direction,
                8,
            )?;
            if let Some(prev) = previous_y {
                route.run((column * step, prev), (column * step, y - 2), 8, 8)?;
            }
            route.splitter(
                (column * step).min((column - 1) * step),
                y - 1,
                8,
                &bus.splitter,
            )?;
            if j + 1 == group.len() {
                route.belt(column * step, y, direction)?;
                last = route.run(
                    ((column + 1) * step, y),
                    (boundary * step, y),
                    direction,
                    direction,
                )?;
            }
            previous_y = Some(y);
        }
        for row in rows.iter_mut().filter(|r| {
            group
                .iter()
                .any(|p| p.entity_number == r.output.entity_number)
        }) {
            row.production_output = Some(row.output.clone());
            row.output.entity_number = last;
            row.output.position = route.doc.blueprint.entities[(last - 1) as usize].position;
            if balance_groups[i] {
                row.output.lanes = "both".into();
            }
        }
    }
    bus.output_entities = (begin..=route.doc.blueprint.entities.len() as u32).collect();
    Ok(())
}

pub(super) fn groups(rows: &[ProductionRow], capacity: f64) -> Vec<Vec<BeltPort>> {
    // Pack by supplied flow, including operating headroom. Share this plan
    // with the input router so it reserves only the necessary collector space.
    let mut ports = outputs(rows);
    ports.sort_by(|a, b| b.rate.total_cmp(&a.rate));
    let mut groups: Vec<Vec<BeltPort>> = Vec::new();
    for port in ports {
        if let Some(group) = groups
            .iter_mut()
            .find(|g| g.iter().map(|p| p.rate).sum::<f64>() + port.rate <= capacity + 1e-8)
        {
            group.push(port);
        } else {
            groups.push(vec![port]);
        }
    }
    for group in &mut groups {
        group.sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
    }
    groups.sort_by(|a, b| {
        a.last()
            .unwrap()
            .position
            .y
            .total_cmp(&b.last().unwrap().position.y)
    });
    groups
}

pub(super) fn needs_balance(rows: &[ProductionRow], group: &[BeltPort], capacity: f64) -> bool {
    if group.len() == 1 {
        return false;
    }
    let mut rates = [0.0; 2];
    for row in rows.iter().filter(|r| {
        group
            .iter()
            .any(|p| p.entity_number == r.output.entity_number)
    }) {
        rates[usize::from(row.output.lanes == "right")] += row.output.rate;
    }
    rates.into_iter().any(|r| r > capacity / 2.0 + 1e-8)
}
