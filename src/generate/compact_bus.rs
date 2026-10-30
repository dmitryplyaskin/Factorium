//! Local balanced branches on shared trunks, with a separate output corridor.
use std::collections::HashSet;

use anyhow::{Context, Result, ensure};

use super::{
    BeltPort, BusLine, FactoryRequest, InputBus, ProductionRow, entity,
    infrastructure::{matching, outputs},
    power::tiles,
};
use crate::{GameData, blueprint::BlueprintDocument};

struct Router<'a> {
    doc: &'a mut BlueprintDocument,
    belt: &'a str,
    underground: &'a str,
    reach: i32,
    occupied: HashSet<(i32, i32)>,
}
impl<'a> Router<'a> {
    fn new(
        data: &GameData,
        doc: &'a mut BlueprintDocument,
        belt: &'a str,
        underground: &'a str,
    ) -> Result<Self> {
        let mut occupied = HashSet::new();
        for e in &doc.blueprint.entities {
            occupied.extend(tiles(e, entity(data, &e.name)?)?);
        }
        let reach = entity(data, underground)?.prototype.properties["max_underground_distance"]
            .as_u64()
            .context("Missing underground reach")? as i32;
        Ok(Self {
            doc,
            belt,
            underground,
            reach,
            occupied,
        })
    }
    fn belt(&mut self, x: i32, y: i32, direction: u8) -> Result<u32> {
        ensure!(
            self.occupied.insert((x, y)),
            "Compact route overlaps a transport entity at ({x}, {y})"
        );
        Ok(self
            .doc
            .add(self.belt, f64::from(x) + 0.5, f64::from(y) + 0.5, direction))
    }
    fn tunnel(&mut self, x: i32, y: i32, direction: u8, kind: &str) -> Result<u32> {
        ensure!(
            self.occupied.insert((x, y)),
            "Compact tunnel endpoint is occupied"
        );
        let id = self.doc.add(
            self.underground,
            f64::from(x) + 0.5,
            f64::from(y) + 0.5,
            direction,
        );
        self.doc.blueprint.entities[(id - 1) as usize].belt_type = Some(kind.into());
        Ok(id)
    }
    /// A straight two-lane path. Tunnels cross occupied runs, never side-load
    /// another conveyor, and always surface before the final turn/port.
    fn run(
        &mut self,
        start: (i32, i32),
        end: (i32, i32),
        direction: u8,
        final_direction: u8,
    ) -> Result<u32> {
        let (dx, dy) = match direction {
            4 => (1, 0),
            8 => (0, 1),
            12 => (-1, 0),
            _ => anyhow::bail!("Unsupported compact route direction"),
        };
        let length = (end.0 - start.0) * dx + (end.1 - start.1) * dy;
        ensure!(
            length >= 0 && (start.0 == end.0 || start.1 == end.1),
            "Invalid compact route segment"
        );
        let mut n = 0;
        let mut last = 0;
        while n <= length {
            let tile = (start.0 + n * dx, start.1 + n * dy);
            if n < length && self.occupied.contains(&(tile.0 + dx, tile.1 + dy)) {
                let distance = (2..=self.reach.min(length - n - 1))
                    .find(|&d| {
                        !self.occupied.contains(&(tile.0 + d * dx, tile.1 + d * dy))
                            && !self
                                .occupied
                                .contains(&(tile.0 + (d + 1) * dx, tile.1 + (d + 1) * dy))
                    })
                    .context("Underground reach cannot cross the compact corridor")?;
                self.tunnel(tile.0, tile.1, direction, "input")?;
                last = self.tunnel(
                    tile.0 + distance * dx,
                    tile.1 + distance * dy,
                    direction,
                    "output",
                )?;
                n += distance + 1;
            } else {
                last = self.belt(
                    tile.0,
                    tile.1,
                    if n == length {
                        final_direction
                    } else {
                        direction
                    },
                )?;
                n += 1;
            }
        }
        Ok(last)
    }
    /// Both halves share the same next column at different heights. The near
    /// half ends before the far half starts, so its tracks can be reused.
    fn branch(
        &mut self,
        x: i32,
        ports: &[BeltPort],
        splitter: &str,
        leaves: &mut Vec<(i32, BeltPort)>,
    ) -> Result<()> {
        let y = ports[0].position.y.floor() as i32;
        self.belt(x, y, 4)?;
        if ports.len() == 1 {
            leaves.push((x, ports[0].clone()));
            return Ok(());
        }
        ensure!(
            self.occupied.insert((x + 1, y)) && self.occupied.insert((x + 1, y + 1)),
            "Compact splitter overlaps another route"
        );
        self.doc
            .add(splitter, f64::from(x) + 1.5, f64::from(y) + 1.0, 4);
        self.belt(x + 2, y, 4)?;
        self.belt(x + 2, y + 1, 4)?;
        let half = ports.len().next_power_of_two() / 2;
        self.branch(x + 3, &ports[..half], splitter, leaves)?;
        let far_y = ports[half].position.y.floor() as i32;
        for ty in y + 1..far_y {
            self.belt(x + 3, ty, 8)?;
        }
        self.branch(x + 3, &ports[half..], splitter, leaves)
    }
}

pub(super) fn place(
    data: &GameData,
    request: &FactoryRequest,
    rows: &[ProductionRow],
    doc: &mut BlueprintDocument,
    belt_rate: f64,
) -> Result<InputBus> {
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
                    <= belt_rate * if p.lanes == "both" { 0.85 } else { 0.425 } + 1e-8
            })
        });
        if !fits {
            partitions.push(Vec::new());
        }
        partitions.last_mut().unwrap().push(row);
    }
    let ingredients = rows[0].input_ports().count();
    let depth = partitions
        .iter()
        .map(|p| p.len().next_power_of_two().ilog2() as i32)
        .max()
        .unwrap();
    let reach = underground.prototype.properties["max_underground_distance"]
        .as_u64()
        .unwrap_or(0) as i32;
    // A wide two-ingredient bus needs surface gaps between trunks for short
    // underground equipment. Ordinary small bundles remain adjacent.
    let pitch = if ingredients > 1 && partitions.len() as i32 + 2 > reach {
        3
    } else {
        1
    };
    let right = if request.reverse_output {
        -5 - outputs(rows).len() as i32
    } else {
        -5
    };
    let block = 3 * depth + (partitions.len() as i32 - 1) * pitch + 4;
    let mut groups = Vec::new();
    for (i, partition) in partitions.iter().enumerate() {
        for j in 0..ingredients {
            let first = partition[0].input_ports().nth(j).unwrap();
            if partition[0]
                .input_ports()
                .take(j)
                .any(|p| p.entity_number == first.entity_number)
            {
                continue;
            }
            let mut ports: Vec<_> = partition
                .iter()
                .map(|r| r.input_ports().nth(j).unwrap().clone())
                .collect();
            ports.sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
            groups.push((
                right - 3 * depth - j as i32 * block - i as i32 * pitch,
                ports,
            ));
        }
    }
    let collector_space = if request.reverse_output && ingredients == 3 {
        outputs(rows).len() as i32
    } else {
        0
    };
    let west = groups.iter().map(|g| g.0).min().unwrap() - 3 - collector_space;
    let top = rows
        .iter()
        .flat_map(ProductionRow::input_ports)
        .map(|p| p.position.y.floor() as i32)
        .min()
        .unwrap()
        - groups.len() as i32;
    let mut order: Vec<_> = (0..groups.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(groups[i].0));
    let mut source_y = vec![0; groups.len()];
    for (rank, i) in order.into_iter().enumerate() {
        source_y[i] = top + rank as i32;
    }
    let mut route = Router::new(data, doc, &request.belt, &underground.prototype.name)?;
    let mut lines = Vec::new();
    let mut outlets = Vec::new();
    for (i, (x, ports)) in groups.iter().enumerate() {
        let begin = route.doc.blueprint.entities.len();
        let source = begin as u32 + 1;
        let y = source_y[i];
        route.run((west, y), (*x, y), 4, 8)?;
        let first_y = ports[0].position.y.floor() as i32;
        for ty in y + 1..first_y {
            route.belt(*x, ty, 8)?;
        }
        let mut leaves = Vec::new();
        route.branch(*x, ports, &splitter.prototype.name, &mut leaves)?;
        outlets.push(leaves);
        lines.push(BusLine {
            source: BeltPort {
                entity_number: source,
                position: route.doc.blueprint.entities[(source - 1) as usize].position,
                item: ports[0].item.clone(),
                rate: ports.iter().map(|p| p.rate).sum(),
                lanes: ports[0].lanes.clone(),
                direction: 4,
            },
            targets: ports.iter().map(|p| p.entity_number).collect(),
            entities: (begin as u32 + 1..=route.doc.blueprint.entities.len() as u32).collect(),
        });
    }
    // All trunks are known before making crossings, including branches of the
    // other ingredient. Never splice one ingredient into another's conveyor.
    for (i, leaves) in outlets.iter().enumerate() {
        for (x, port) in leaves {
            let begin = route.doc.blueprint.entities.len();
            let y = port.position.y.floor() as i32;
            let end = port.position.x.floor() as i32 - 1;
            if *x < end {
                route.run((x + 1, y), (end, y), 4, 4)?;
            }
            lines[i]
                .entities
                .extend(begin as u32 + 1..=route.doc.blueprint.entities.len() as u32);
        }
    }
    let mut shared_lines = Vec::new();
    for line in &lines {
        if line.source.lanes == "both" {
            continue;
        }
        let peers: Vec<_> = rows
            .iter()
            .flat_map(ProductionRow::input_ports)
            .filter(|p| line.targets.contains(&p.entity_number) && p.item != line.source.item)
            .collect();
        if !peers.is_empty() {
            let mut other = line.clone();
            other.source.item = peers[0].item.clone();
            other.source.lanes = peers[0].lanes.clone();
            other.source.rate = peers.iter().map(|p| p.rate).sum();
            shared_lines.push(other);
        }
    }
    lines.extend(shared_lines);
    Ok(InputBus {
        compact: true,
        splitter: splitter.prototype.name.clone(),
        underground: underground.prototype.name.clone(),
        lines,
    })
}

pub(super) fn collect_outputs(
    data: &GameData,
    request: &FactoryRequest,
    rows: &mut [ProductionRow],
    doc: &mut BlueprintDocument,
    bus: &InputBus,
) -> Result<()> {
    let ports = outputs(rows);
    if ports.len() == 1 && !request.reverse_output {
        return Ok(());
    }
    let mut route = Router::new(data, doc, &request.belt, &bus.underground)?;
    let step = if request.reverse_output { -1 } else { 1 };
    let west = bus
        .lines
        .iter()
        .map(|l| l.source.position.x.floor() as i32)
        .min()
        .unwrap();
    let right = route.occupied.iter().map(|p| p.0).max().unwrap();
    let bottom = route.occupied.iter().map(|p| p.1).max().unwrap() + 2;
    // Keep an empty tile after east-facing input belt ends so raw ingredients
    // cannot side-load onto the output collector of a full-width lower row.
    let column = if request.reverse_output {
        if rows[0].input_ports().count() == 3 {
            west + ports.len() as i32 + 1
        } else {
            -3
        }
    } else {
        right + 2
    };
    let boundary = if request.reverse_output {
        west
    } else {
        column + ports.len() as i32 + 2
    };
    let spacing = if request.output_chest.is_some() { 6 } else { 1 };
    for (i, port) in ports.iter().enumerate() {
        let x = port.position.x.floor() as i32;
        let y = port.position.y.floor() as i32;
        let end_x = column + step * (ports.len() - 1 - i) as i32;
        let end_y = bottom + i as i32 * spacing;
        let last = if ports.len() == 1 && request.reverse_output {
            route.run((x - 1, y), (boundary, y), 12, 12)?
        } else {
            route.run((x + step, y), (end_x, y), port.direction, 8)?;
            route.run((end_x, y + 1), (end_x, end_y), 8, port.direction)?;
            route.run(
                (end_x + step, end_y),
                (boundary, end_y),
                port.direction,
                port.direction,
            )?
        };
        for row in rows
            .iter_mut()
            .filter(|r| r.output.entity_number == port.entity_number)
        {
            row.production_output = Some(row.output.clone());
            row.output.entity_number = last;
            row.output.position = route.doc.blueprint.entities[(last - 1) as usize].position;
        }
    }
    Ok(())
}
