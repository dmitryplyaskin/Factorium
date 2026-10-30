//! Local balanced branches on shared trunks, with a separate output corridor.
use std::collections::HashSet;

use anyhow::{Context, Result, ensure};

use super::{
    BeltPort, BusLine, FactoryRequest, InputBus, ProductionRow, entity, infrastructure::matching,
    power::tiles,
};
use crate::{GameData, blueprint::BlueprintDocument};

pub(super) struct Router<'a> {
    pub doc: &'a mut BlueprintDocument,
    belt: &'a str,
    underground: &'a str,
    reach: i32,
    occupied: HashSet<(i32, i32)>,
}
impl<'a> Router<'a> {
    pub fn new(
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
    pub fn belt(&mut self, x: i32, y: i32, direction: u8) -> Result<u32> {
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
    pub fn run(
        &mut self,
        start: (i32, i32),
        end: (i32, i32),
        direction: u8,
        final_direction: u8,
    ) -> Result<u32> {
        let (dx, dy) = match direction {
            0 => (0, -1),
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
                    .with_context(|| format!("Underground reach cannot cross the compact corridor from {tile:?} towards {end:?}"))?;
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
    pub fn splitter(&mut self, x: i32, y: i32, direction: u8, name: &str) -> Result<u32> {
        let (dx, dy) = if matches!(direction, 4 | 12) {
            (0, 1)
        } else {
            (1, 0)
        };
        ensure!(
            self.occupied.insert((x, y)) && self.occupied.insert((x + dx, y + dy)),
            "Output splitter overlaps another route"
        );
        Ok(self.doc.add(
            name,
            f64::from(x) + 0.5 + f64::from(dx) / 2.0,
            f64::from(y) + 0.5 + f64::from(dy) / 2.0,
            direction,
        ))
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
        // Lane balancers and collectors stay beside the rows, inside the bus.
        let groups = super::output_bus::groups(rows, belt_rate);
        let balance = groups
            .iter()
            .any(|g| super::output_bus::needs_balance(rows, g, belt_rate));
        if groups.len() == 1 && groups[0].len() == 1 {
            -5
        } else {
            -7 - if balance { 8 } else { 0 } - 4 * groups.len() as i32
        }
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
    let west = groups.iter().map(|g| g.0).min().unwrap() - 3;
    let top = rows
        .iter()
        .flat_map(ProductionRow::input_ports)
        .map(|p| p.position.y.floor() as i32)
        .min()
        .unwrap();
    groups.sort_by_key(|(x, ports)| (ports[0].position.y.floor() as i32, std::cmp::Reverse(*x)));
    // Headers start inside the production height. Reserve row belt levels and
    // the second tile of local splitter branches; cross only their thin trunks.
    let blocked: HashSet<_> = rows
        .iter()
        .flat_map(|row| {
            row.input_ports()
                .flat_map(|p| {
                    let y = p.position.y.floor() as i32;
                    [y, y + 1]
                })
                .chain([row.output.position.y.floor() as i32])
        })
        .collect();
    let mut route = Router::new(data, doc, &request.belt, &underground.prototype.name)?;
    let mut lines = Vec::new();
    let mut outlets = Vec::new();
    let mut y = top;
    for (i, (x, ports)) in groups.iter().enumerate() {
        if i > 0 {
            y += 1;
            while blocked.contains(&y) {
                y += 1;
            }
        }
        let begin = route.doc.blueprint.entities.len();
        let source = begin as u32 + 1;
        let first_y = ports[0].position.y.floor() as i32;
        if y == first_y {
            route.run((west, y), (x - 1, y), 4, 4)?;
        } else {
            let direction = if y < first_y { 8 } else { 0 };
            route.run((west, y), (*x, y), 4, direction)?;
            if y < first_y {
                for ty in y + 1..first_y {
                    route.belt(*x, ty, direction)?;
                }
            } else {
                for ty in (first_y + 1..y).rev() {
                    route.belt(*x, ty, direction)?;
                }
            }
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
        output_entities: Vec::new(),
    })
}
