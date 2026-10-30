//! Coverage and copper-wire reach are separate constraints.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque},
};

use anyhow::{Context, Result, ensure};

use super::{GenerationControl, PowerPlan, ProductionRow, entity, stat};
use crate::{
    GameData,
    blueprint::{BlueprintDocument, BlueprintEntity},
    model::{Entity, Position},
};

pub(crate) fn distance(a: Position, b: Position) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

pub(crate) fn tiles(e: &BlueprintEntity, p: &Entity) -> Result<Vec<(i32, i32)>> {
    ensure!(
        e.position.x.is_finite() && e.position.y.is_finite(),
        "Invalid entity position"
    );
    let (w, h) = if e.direction == 4 || e.direction == 12 {
        (p.tile_height, p.tile_width)
    } else {
        (p.tile_width, p.tile_height)
    };
    let x = e.position.x - f64::from(w) / 2.0;
    let y = e.position.y - f64::from(h) / 2.0;
    ensure!(
        (x - x.round()).abs() < 1e-8 && (y - y.round()).abs() < 1e-8,
        "{} is misaligned with its tile grid",
        e.name
    );
    Ok((0..w as i32)
        .flat_map(|dx| (0..h as i32).map(move |dy| (x as i32 + dx, y as i32 + dy)))
        .collect())
}

pub(crate) fn covered(pole: Position, radius: f64, e: &BlueprintEntity, p: &Entity) -> bool {
    // Power supply uses the square area intersecting the consumer's collision box.
    let box_ = p.collision_box;
    let left = e.position.x + box_.left_top.x;
    let right = e.position.x + box_.right_bottom.x;
    let top = e.position.y + box_.left_top.y;
    let bottom = e.position.y + box_.right_bottom.y;
    pole.x + radius > left
        && pole.x - radius < right
        && pole.y + radius > top
        && pole.y - radius < bottom
}

fn pole_position(tile: (i32, i32), p: &Entity) -> Position {
    Position {
        x: f64::from(tile.0) + f64::from(p.tile_width) / 2.0,
        y: f64::from(tile.1) + f64::from(p.tile_height) / 2.0,
    }
}

fn overlaps(a: (i32, i32), b: (i32, i32), p: &Entity) -> bool {
    (a.0 - b.0).abs() < (p.tile_width as i32) && (a.1 - b.1).abs() < (p.tile_height as i32)
}

/// Prefer the same repeating columns on both inserter lines of a row. Keep
/// edge slots so a shortened last interval never leaves the end hands unpowered.
/// Larger poles and machines without an inserter gap use the general planner.
fn regular_candidates(
    doc: &BlueprintDocument,
    rows: &[ProductionRow],
    machine: &Entity,
    pole: &Entity,
    lookup: &HashMap<(i32, i32), usize>,
    covers: &[Vec<usize>],
    reach: f64,
) -> Vec<bool> {
    let mut preferred = vec![false; covers.len()];
    if pole.tile_width != 1 || pole.tile_height != 1 || machine.tile_width < 3 {
        return preferred;
    }
    let w = f64::from(machine.tile_width);
    let h = f64::from(machine.tile_height);
    for row in rows {
        let first = &doc.blueprint.entities[(row.machines[0] - 1) as usize];
        for y in [first.position.y - h / 2.0 - 1.0, first.position.y + h / 2.0] {
            let slots: Vec<_> = row
                .machines
                .iter()
                .filter_map(|&id| {
                    let e = &doc.blueprint.entities[(id - 1) as usize];
                    let x = (e.position.x - w / 2.0) as i32 + (machine.tile_width as i32 - 1) / 2;
                    lookup.get(&(x, y as i32)).map(|&index| (x, index))
                })
                .collect();
            if slots.is_empty() {
                continue;
            }
            let targets: HashSet<_> = slots
                .iter()
                .flat_map(|&(_, i)| covers[i].iter().copied())
                .collect();
            let pitch = slots
                .windows(2)
                .map(|s| s[1].0 - s[0].0)
                .min()
                .unwrap_or(1)
                .max(1);
            let max_stride = ((reach / f64::from(pitch)).floor() as usize).clamp(1, slots.len());
            let mut best: Vec<_> = (0..slots.len()).collect();
            for stride in 2..=max_stride {
                for phase in 0..stride {
                    let pattern: Vec<_> = (0..slots.len())
                        .filter(|&i| i == 0 || i + 1 == slots.len() || i % stride == phase)
                        .collect();
                    if pattern.len() >= best.len()
                        || pattern
                            .windows(2)
                            .any(|s| f64::from(slots[s[1]].0 - slots[s[0]].0) > reach + 1e-8)
                    {
                        continue;
                    }
                    let covered: HashSet<_> = pattern
                        .iter()
                        .flat_map(|&i| covers[slots[i].1].iter().copied())
                        .collect();
                    if covered.len() == targets.len() {
                        best = pattern;
                    }
                }
            }
            for i in best {
                preferred[slots[i].1] = true;
            }
        }
    }
    preferred
}

struct Components {
    parent: Vec<usize>,
}
impl Components {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn root(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.root(self.parent[x]);
        }
        self.parent[x]
    }
    fn join(&mut self, a: usize, b: usize) -> bool {
        let a = self.root(a);
        let b = self.root(b);
        if a == b {
            false
        } else {
            self.parent[b] = a;
            true
        }
    }
}

/// Generate a degree-limited minimum-spanning forest on physically reachable poles.
fn wire_forest(
    selected: &[(i32, i32)],
    p: &Entity,
    reach: f64,
) -> (Components, Vec<(usize, usize)>) {
    let mut candidates = Vec::new();
    let mut buckets: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    let bucket_size = reach + 1e-8;
    for i in 0..selected.len() {
        let tile = selected[i];
        let bucket = (
            (f64::from(tile.0) / bucket_size).floor() as i32,
            (f64::from(tile.1) / bucket_size).floor() as i32,
        );
        for dy in -1..=1 {
            for dx in -1..=1 {
                if let Some(neighbors) = buckets.get(&(bucket.0 + dx, bucket.1 + dy)) {
                    for &j in neighbors {
                        let d = distance(pole_position(tile, p), pole_position(selected[j], p));
                        if d <= reach + 1e-8 {
                            candidates.push((d, j, i));
                        }
                    }
                }
            }
        }
        buckets.entry(bucket).or_default().push(i);
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut components = Components::new(selected.len());
    let mut degree = vec![0; selected.len()];
    let mut edges = Vec::new();
    for (_, i, j) in candidates {
        if degree[i] < 5 && degree[j] < 5 && components.join(i, j) {
            degree[i] += 1;
            degree[j] += 1;
            edges.push((i, j));
        }
    }
    (components, edges)
}

pub(crate) fn place_and_wire(
    data: &GameData,
    doc: &mut BlueprintDocument,
    name: &str,
    rows: &[ProductionRow],
    control: &GenerationControl,
) -> Result<PowerPlan> {
    control.check()?;
    let p = entity(data, name)?;
    ensure!(
        p.prototype.prototype_type == "electric-pole",
        "Select an electric pole"
    );
    ensure!(
        (1..=4).contains(&p.tile_width) && (1..=4).contains(&p.tile_height),
        "Unsupported pole footprint"
    );
    let radius = stat(p, "supply_area_distance")?;
    let reach = stat(p, "max_wire_distance")?;
    ensure!(
        radius > 0.0 && reach >= f64::from(p.tile_width.max(p.tile_height)) && reach <= 64.0,
        "Unsupported pole supply area or wire reach"
    );
    let mut occupied = HashSet::new();
    let mut consumers = Vec::new();
    for e in &doc.blueprint.entities {
        let proto = entity(data, &e.name)?;
        occupied.extend(tiles(e, proto)?);
        if proto
            .prototype
            .properties
            .contains_key("electric_energy_source")
        {
            consumers.push((e, proto));
        }
    }
    ensure!(!consumers.is_empty(), "No electric consumers in the layout");
    let min_x = occupied.iter().map(|v| v.0).min().unwrap() - p.tile_width as i32 - 2;
    let max_x = occupied.iter().map(|v| v.0).max().unwrap() + 3;
    let min_y = occupied.iter().map(|v| v.1).min().unwrap() - p.tile_height as i32 - 2;
    let max_y = occupied.iter().map(|v| v.1).max().unwrap() + 3;
    ensure!(
        (max_x - min_x) as i64 * (max_y - min_y) as i64 <= 200_000,
        "Power-planning area exceeds prototype budget"
    );
    let mut candidates = Vec::new();
    for y in min_y..=max_y {
        control.check()?;
        for x in min_x..=max_x {
            if (0..p.tile_width as i32)
                .all(|dx| (0..p.tile_height as i32).all(|dy| !occupied.contains(&(x + dx, y + dy))))
            {
                candidates.push((x, y));
            }
        }
    }
    let lookup: HashMap<_, _> = candidates
        .iter()
        .enumerate()
        .map(|(i, &p)| (p, i))
        .collect();
    // Index local coverage once. Rescanning every consumer for every candidate
    // after every pole would become cubic on large requests.
    let mut covers = vec![Vec::new(); candidates.len()];
    let mut consumer_candidates = vec![Vec::new(); consumers.len()];
    for (i, (e, proto)) in consumers.iter().enumerate() {
        control.check()?;
        let box_ = proto.collision_box;
        let x0 = (e.position.x + box_.left_top.x - radius - f64::from(p.tile_width) / 2.0).floor()
            as i32;
        let x1 = (e.position.x + box_.right_bottom.x + radius - f64::from(p.tile_width) / 2.0)
            .ceil() as i32;
        let y0 = (e.position.y + box_.left_top.y - radius - f64::from(p.tile_height) / 2.0).floor()
            as i32;
        let y1 = (e.position.y + box_.right_bottom.y + radius - f64::from(p.tile_height) / 2.0)
            .ceil() as i32;
        for y in y0.max(min_y)..=y1.min(max_y) {
            for x in x0.max(min_x)..=x1.min(max_x) {
                if let Some(&index) = lookup.get(&(x, y)) {
                    if covered(pole_position((x, y), p), radius, e, proto) {
                        covers[index].push(i);
                        consumer_candidates[i].push(index);
                    }
                }
            }
        }
    }
    let machine = entity(
        data,
        &doc.blueprint.entities[(rows[0].machines[0] - 1) as usize].name,
    )?;
    let preferred = regular_candidates(doc, rows, machine, p, &lookup, &covers, reach);
    let mut scores: Vec<usize> = covers.iter().map(Vec::len).collect();
    let mut heap: BinaryHeap<_> = scores
        .iter()
        .enumerate()
        .filter(|&(_, score)| *score > 0)
        .map(|(i, &score)| (preferred[i], score, Reverse(i)))
        .collect();
    let mut selected = Vec::new();
    let mut uncovered = vec![true; consumers.len()];
    let mut remaining = consumers.len();
    while remaining > 0 {
        control.check()?;
        let (_, score, Reverse(index)) = loop {
            let entry=heap.pop().context("This pole cannot cover all consumers in the row template; select a wider supply area")?;
            if entry.1 > 0 && scores[entry.2.0] == entry.1 {
                break entry;
            }
        };
        debug_assert!(score > 0);
        let tile = candidates[index];
        selected.push(tile);
        for dy in -(p.tile_height as i32 - 1)..p.tile_height as i32 {
            for dx in -(p.tile_width as i32 - 1)..p.tile_width as i32 {
                if let Some(&blocked) = lookup.get(&(tile.0 + dx, tile.1 + dy)) {
                    scores[blocked] = 0;
                }
            }
        }
        for &consumer in &covers[index] {
            if !uncovered[consumer] {
                continue;
            }
            uncovered[consumer] = false;
            remaining -= 1;
            for &other in &consumer_candidates[consumer] {
                if scores[other] > 0 {
                    scores[other] -= 1;
                    if scores[other] > 0 {
                        heap.push((preferred[other], scores[other], Reverse(other)));
                    }
                }
            }
        }
    }
    // Join disconnected coverage islands by searching for relay positions, rather
    // than assuming that overlapping supply areas imply a wired network.
    let final_edges = loop {
        control.check()?;
        let (mut components, edges) = wire_forest(&selected, p, reach);
        let root = components.root(0);
        if (0..selected.len()).all(|i| components.root(i) == root) {
            break edges;
        }
        let mut queue = VecDeque::new();
        let mut parents = HashMap::new();
        let mut goals = HashSet::new();
        for (i, &tile) in selected.iter().enumerate() {
            let index = lookup[&tile];
            if components.root(i) == root {
                queue.push_back(index);
                parents.insert(index, index);
            } else {
                goals.insert(index);
            }
        }
        let r = reach.floor() as i32;
        // Spatially mark overlapping placements once, rather than scanning all
        // selected poles for every edge explored by the relay search.
        let selected_tiles: HashSet<_> = selected.iter().copied().collect();
        let mut blocked = HashSet::new();
        for &(x, y) in &selected {
            for dy in -(p.tile_height as i32 - 1)..p.tile_height as i32 {
                for dx in -(p.tile_width as i32 - 1)..p.tile_width as i32 {
                    if dx != 0 || dy != 0 {
                        blocked.insert((x + dx, y + dy));
                    }
                }
            }
        }
        let mut found = None;
        while let Some(index) = queue.pop_front() {
            control.check()?;
            if goals.contains(&index) {
                found = Some(index);
                break;
            }
            let tile = candidates[index];
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx == 0 && dy == 0 || f64::from(dx).hypot(f64::from(dy)) > reach + 1e-8 {
                        continue;
                    }
                    let next = (tile.0 + dx, tile.1 + dy);
                    if overlaps(tile, next, p) {
                        continue;
                    }
                    let Some(&next_index) = lookup.get(&next) else {
                        continue;
                    };
                    if parents.contains_key(&next_index) || blocked.contains(&next) {
                        continue;
                    }
                    parents.insert(next_index, index);
                    queue.push_back(next_index);
                }
            }
        }
        let mut index = found
            .context("Cannot connect all power poles within wire reach; select a different pole")?;
        let mut added = 0;
        while parents[&index] != index {
            let tile = candidates[index];
            if !selected_tiles.contains(&tile) {
                ensure!(
                    !selected.iter().any(|&s| overlaps(s, tile, p)),
                    "Relay pole would overlap another pole"
                );
                selected.push(tile);
                added += 1;
            }
            index = parents[&index];
        }
        ensure!(
            added > 0,
            "Cannot connect the pole network within the five-wire limit"
        );
        ensure!(
            selected.len() <= 4096,
            "Power network exceeds prototype pole budget"
        );
    };
    let mut ids = BTreeMap::new();
    for (i, &tile) in selected.iter().enumerate() {
        let pos = pole_position(tile, p);
        ids.insert(i, doc.add(name, pos.x, pos.y, 0));
    }
    let mut degrees = vec![0; selected.len()];
    for (i, j) in final_edges {
        doc.blueprint.wires.push([ids[&i], 5, ids[&j], 5]);
        degrees[i] += 1;
        degrees[j] += 1;
    }
    // The first coverage pole can be deep inside an expanded bus/storage
    // footprint. Expose a pole near an outer edge with a spare copper socket.
    let x0 = occupied
        .iter()
        .map(|t| t.0)
        .chain(selected.iter().map(|t| t.0))
        .min()
        .unwrap();
    let y0 = occupied
        .iter()
        .map(|t| t.1)
        .chain(selected.iter().map(|t| t.1))
        .min()
        .unwrap();
    let x1 = occupied
        .iter()
        .map(|t| t.0 + 1)
        .chain(selected.iter().map(|t| t.0 + p.tile_width as i32))
        .max()
        .unwrap();
    let y1 = occupied
        .iter()
        .map(|t| t.1 + 1)
        .chain(selected.iter().map(|t| t.1 + p.tile_height as i32))
        .max()
        .unwrap();
    let exit_distance = |i: usize| {
        let pos = pole_position(selected[i], p);
        [
            (pos.x - f64::from(x0)).min(f64::from(x1) - pos.x)
                + f64::from(p.tile_width) / 2.0
                + 1.0,
            (pos.y - f64::from(y0)).min(f64::from(y1) - pos.y)
                + f64::from(p.tile_height) / 2.0
                + 1.0,
        ]
        .into_iter()
        .fold(f64::INFINITY, f64::min)
    };
    let connection = (0..selected.len())
        .filter(|&i| degrees[i] < 5)
        .min_by(|&a, &b| exit_distance(a).total_cmp(&exit_distance(b)))
        .context("No spare copper connection for external power")?;
    ensure!(
        exit_distance(connection) <= reach + 1e-8,
        "No pole can accept external power outside the blueprint footprint"
    );
    Ok(PowerPlan {
        pole: name.into(),
        supply_area_distance: radius,
        max_wire_distance: reach,
        pole_count: selected.len(),
        connection_entity: ids[&connection],
        connection_position: pole_position(selected[connection], p),
        installed_power_watts: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_wire_search_preserves_brute_force_edges_at_negative_coordinates() {
        let box_ =
            serde_json::json!({"left_top":{"x":-0.4,"y":-0.4},"right_bottom":{"x":0.4,"y":0.4}});
        let pole: Entity = serde_json::from_value(serde_json::json!({
            "name":"pole","type":"electric-pole","hidden":false,
            "tile_width":1,"tile_height":1,"collision_box":box_,"selection_box":box_,
            "fluid_boxes":[],"quality_stats":{}
        }))
        .unwrap();
        let selected: Vec<_> = (-5..=5)
            .flat_map(|x| (-4..=4).map(move |y| (x * 3, y * 4)))
            .collect();
        for reach in [3.0 - 1e-9, 7.5, 12.0] {
            let mut candidates = Vec::new();
            for i in 0..selected.len() {
                for j in i + 1..selected.len() {
                    let d = distance(
                        pole_position(selected[i], &pole),
                        pole_position(selected[j], &pole),
                    );
                    if d <= reach + 1e-8 {
                        candidates.push((d, i, j));
                    }
                }
            }
            candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
            let mut components = Components::new(selected.len());
            let mut degree = vec![0; selected.len()];
            let mut expected = Vec::new();
            for (_, i, j) in candidates {
                if degree[i] < 5 && degree[j] < 5 && components.join(i, j) {
                    degree[i] += 1;
                    degree[j] += 1;
                    expected.push((i, j));
                }
            }
            assert_eq!(wire_forest(&selected, &pole, reach).1, expected);
        }
    }
}
