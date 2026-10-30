//! Reconstruct the collector's lane graph and verify its physical capacity.
use super::{GeneratedFactory, entity};
use crate::{GameData, blueprint::BlueprintEntity};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

fn delta(d: u8) -> Result<(i32, i32)> {
    match d {
        0 => Ok((0, -1)),
        4 => Ok((1, 0)),
        8 => Ok((0, 1)),
        12 => Ok((-1, 0)),
        _ => anyhow::bail!("Invalid collector direction"),
    }
}

pub(super) fn validate(
    data: &GameData,
    f: &GeneratedFactory,
    ids: &BTreeMap<u32, &BlueprintEntity>,
    occupied: &HashMap<(i32, i32), u32>,
) -> Result<BTreeSet<u32>> {
    let Some(bus) = f.bus.as_ref().filter(|b| !b.output_entities.is_empty()) else {
        return Ok(BTreeSet::new());
    };
    let used: BTreeSet<_> = bus.output_entities.iter().copied().collect();
    ensure!(
        used.len() == bus.output_entities.len(),
        "Duplicate output collector entity"
    );
    let mut all = used.clone();
    for row in &f.rows {
        all.insert(
            row.production_output
                .as_ref()
                .context("Missing production output")?
                .entity_number,
        );
    }
    let indices: BTreeMap<_, _> = all.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let ports = f.output_ports();
    let sinks: BTreeSet<_> = ports.iter().map(|p| p.entity_number).collect();
    let mut links = BTreeMap::<u32, Vec<u32>>::new();
    let mut incoming = BTreeMap::<u32, BTreeSet<u8>>::new();
    for &id in &all {
        let e = ids.get(&id).context("Missing output collector entity")?;
        ensure!(
            e.name == f.request.belt || e.name == bus.underground || e.name == bus.splitter,
            "Unexpected output collector entity"
        );
        let (dx, dy) = delta(e.direction)?;
        let (x, y) = (e.position.x.floor() as i32, e.position.y.floor() as i32);
        let mut next = Vec::new();
        if !sinks.contains(&id) {
            if e.name == bus.underground && e.belt_type.as_deref() == Some("input") {
                let reach = entity(data, &e.name)?.prototype.properties["max_underground_distance"]
                    .as_u64()
                    .context("Missing underground reach")? as i32;
                let other = (1..=reach).find_map(|n| {
                    occupied
                        .get(&(x + dx * n, y + dy * n))
                        .and_then(|id| ids.get(id))
                        .filter(|o| {
                            o.name == e.name
                                && o.direction == e.direction
                                && o.belt_type.as_deref() == Some("output")
                        })
                });
                next.push(
                    other
                        .context("Disconnected collector underground")?
                        .entity_number,
                );
            } else {
                let tiles = if e.name == bus.splitter {
                    let (x0, y0) = (
                        (e.position.x - if dx == 0 { 1.0 } else { 0.5 }).round() as i32,
                        (e.position.y - if dy == 0 { 1.0 } else { 0.5 }).round() as i32,
                    );
                    vec![
                        (x0 + dx, y0 + dy),
                        (
                            x0 + dx + if dx == 0 { 1 } else { 0 },
                            y0 + dy + if dy == 0 { 1 } else { 0 },
                        ),
                    ]
                } else {
                    vec![(x + dx, y + dy)]
                };
                for tile in tiles {
                    if let Some(&target) = occupied.get(&tile) {
                        ensure!(
                            all.contains(&target),
                            "Output collector leaks into another route"
                        );
                        let to = ids[&target];
                        ensure!(
                            to.direction != (e.direction + 8) % 16,
                            "Output collector enters the front of a belt"
                        );
                        if to.name != f.request.belt {
                            ensure!(
                                to.direction == e.direction
                                    && (to.name == bus.splitter
                                        || to.belt_type.as_deref() == Some("input")),
                                "Collector enters equipment from the wrong side"
                            );
                        }
                        next.push(target);
                    }
                }
            }
            ensure!(!next.is_empty(), "Disconnected output collector");
        }
        for &to in &next {
            incoming.entry(to).or_default().insert(e.direction);
        }
        links.insert(id, next);
    }
    // Every branch must connect to the boundary, and the graph must be acyclic.
    let mut indegree: BTreeMap<_, usize> = all.iter().map(|&id| (id, 0)).collect();
    for next in links.values() {
        for id in next {
            *indegree
                .get_mut(id)
                .context("Collector crosses another route")? += 1;
        }
    }
    let mut queue: VecDeque<_> = indegree
        .iter()
        .filter_map(|(&id, &n)| (n == 0).then_some(id))
        .collect();
    let mut order = Vec::new();
    while let Some(id) = queue.pop_front() {
        order.push(id);
        for to in &links[&id] {
            let n = indegree.get_mut(to).unwrap();
            *n -= 1;
            if *n == 0 {
                queue.push_back(*to);
            }
        }
    }
    ensure!(order.len() == all.len(), "Output collector forms a cycle");
    let mut reachable = BTreeMap::<u32, BTreeSet<u32>>::new();
    let mut balanced = BTreeMap::new();
    for &id in order.iter().rev() {
        let mut targets = BTreeSet::new();
        if sinks.contains(&id) {
            targets.insert(id);
        }
        for to in &links[&id] {
            targets.extend(&reachable[to]);
        }
        ensure!(!targets.is_empty(), "Unused collector branch");
        reachable.insert(id, targets);
        balanced.insert(
            id,
            (ids[&id].name == bus.splitter && matches!(ids[&id].direction, 4 | 12))
                || links[&id].iter().any(|to| balanced[to]),
        );
    }
    let source = 4 * all.len();
    let sink = source + 1;
    let mut network = Network::new(sink + 1);
    for (&id, &i) in &indices {
        let e = ids[&id];
        for lane in 0..2 {
            network.add(
                4 * i + lane,
                4 * i + 2 + lane,
                f.belt_items_per_second * if e.name == bus.splitter { 1.0 } else { 0.5 },
            );
        }
        for &to in &links[&id] {
            let dest = ids[&to];
            let from_delta = delta(e.direction)?;
            let to_delta = delta(dest.direction)?;
            let side = e.direction != dest.direction && incoming[&to].len() > 1;
            for lane in 0..2 {
                let target_lane = if side {
                    usize::from(to_delta.0 * from_delta.1 - to_delta.1 * from_delta.0 < 0)
                } else {
                    lane
                };
                network.add(
                    4 * i + 2 + lane,
                    4 * indices[&to] + target_lane,
                    f.belt_items_per_second * 0.5,
                );
            }
        }
    }
    let mut reached = BTreeSet::new();
    let mut queue = VecDeque::new();
    for row in &f.rows {
        let p = row.production_output.as_ref().unwrap();
        ensure!(
            row.output.lanes
                == if balanced[&p.entity_number] {
                    "both"
                } else {
                    p.lanes.as_str()
                },
            "Output collector lane assignment mismatch"
        );
        ensure!(
            reachable[&p.entity_number].contains(&row.output.entity_number),
            "Row connects to the wrong output collector"
        );
        queue.push_back(p.entity_number);
        let lane = usize::from(p.lanes == "right");
        network.add(source, 4 * indices[&p.entity_number] + lane, p.rate);
    }
    while let Some(id) = queue.pop_front() {
        if reached.insert(id) {
            queue.extend(&links[&id]);
        }
    }
    ensure!(reached == all, "Unreachable output collector transport");
    for p in ports {
        ensure!(
            used.contains(&p.entity_number) && ids[&p.entity_number].name == f.request.belt,
            "Invalid collector outlet"
        );
        for lane in 0..2 {
            let rate = f
                .rows
                .iter()
                .filter(|r| r.output.entity_number == p.entity_number)
                .map(|r| match r.output.lanes.as_str() {
                    "both" => r.output.rate / 2.0,
                    "left" if lane == 0 => r.output.rate,
                    "right" if lane == 1 => r.output.rate,
                    _ => 0.0,
                })
                .sum();
            network.add(4 * indices[&p.entity_number] + 2 + lane, sink, rate);
        }
    }
    ensure!(
        network.flow(source, sink) + 1e-6 >= f.planned_output_per_second,
        "Output collector exceeds physical lane capacity"
    );
    Ok(used)
}

struct Network {
    edges: Vec<Vec<(usize, usize, f64)>>,
}
impl Network {
    fn new(n: usize) -> Self {
        Self {
            edges: vec![Vec::new(); n],
        }
    }
    fn add(&mut self, a: usize, b: usize, cap: f64) {
        let (ai, bi) = (self.edges[a].len(), self.edges[b].len());
        self.edges[a].push((b, bi, cap));
        self.edges[b].push((a, ai, 0.0));
    }
    fn flow(&mut self, source: usize, sink: usize) -> f64 {
        let mut total = 0.0;
        loop {
            let mut prev = vec![None; self.edges.len()];
            let mut queue = VecDeque::from([source]);
            prev[source] = Some((source, 0));
            while let Some(a) = queue.pop_front() {
                for (i, &(b, _, cap)) in self.edges[a].iter().enumerate() {
                    if cap > 1e-9 && prev[b].is_none() {
                        prev[b] = Some((a, i));
                        queue.push_back(b);
                    }
                }
                if prev[sink].is_some() {
                    break;
                }
            }
            if prev[sink].is_none() {
                break;
            }
            let mut amount = f64::INFINITY;
            let mut b = sink;
            while b != source {
                let (a, i) = prev[b].unwrap();
                amount = amount.min(self.edges[a][i].2);
                b = a;
            }
            b = sink;
            while b != source {
                let (a, i) = prev[b].unwrap();
                let reverse = self.edges[a][i].1;
                self.edges[a][i].2 -= amount;
                self.edges[b][reverse].2 += amount;
                b = a;
            }
            total += amount;
        }
        total
    }
}
