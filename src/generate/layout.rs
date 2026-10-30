//! Bounded template search. Every scored candidate includes its actual power network.

use std::collections::BTreeSet;

use anyhow::Result;

use super::GeneratedFactory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LayoutSpec {
    pub paired: bool,
    pub per_row: usize,
    pub machine_gap: u32,
    /// Empty tile rows between two independent belt/machine bands.
    pub row_gap: u32,
}

pub(super) struct Dimensions {
    pub count: usize,
    pub max_per_row: usize,
    pub legacy_per_row: usize,
    pub input_belts: u32,
    pub machine: [u32; 2],
    pub pole: [u32; 2],
}

impl Dimensions {
    fn legacy(&self) -> LayoutSpec {
        LayoutSpec {
            paired: false,
            per_row: self.legacy_per_row,
            machine_gap: self.pole[0] + 2,
            row_gap: self.pole[1] + 4,
        }
    }

    fn production_area(&self, spec: LayoutSpec) -> u64 {
        let n = self.count.min(spec.per_row) as u64;
        let rows = self.count.div_ceil(spec.per_row) as u64;
        // Two exposed belt tiles at each end.
        let width = n * u64::from(self.machine[0]) + (n - 1) * u64::from(spec.machine_gap) + 4;
        let pairs = if spec.paired { rows / 2 } else { 0 };
        let groups = rows - pairs;
        let height = rows * u64::from(self.machine[1] + 3 + self.input_belts) - pairs
            + (groups - 1) * u64::from(spec.row_gap);
        width * height
    }

    fn candidates(&self) -> Vec<LayoutSpec> {
        let max = self.max_per_row.min(self.count);
        // At most eight row lengths, four horizontal gaps and three vertical
        // gaps and two orientations: <=192 candidates, independent of count.
        let balanced = self.count.div_ceil(self.count.div_ceil(max));
        let lengths: BTreeSet<_> = [
            1,
            2,
            4,
            8,
            max,
            max.div_ceil(2),
            balanced,
            (self.count as f64).sqrt().ceil() as usize,
        ]
        .into_iter()
        .map(|n| n.clamp(1, max))
        .collect();
        let gaps: BTreeSet<_> = [0, 1, self.pole[0], self.pole[0] + 2].into_iter().collect();
        let row_gaps: BTreeSet<_> = [0, self.pole[1], self.pole[1] + 4].into_iter().collect();
        let mut specs = Vec::new();
        for paired in [true, false] {
            for &per_row in &lengths {
                let per_row = if paired {
                    per_row.min(self.count.div_ceil(2))
                } else {
                    per_row
                };
                for &machine_gap in &gaps {
                    for &row_gap in &row_gaps {
                        specs.push(LayoutSpec {
                            paired,
                            per_row,
                            machine_gap: if self.count == 1 { 0 } else { machine_gap },
                            row_gap: if self.count.div_ceil(per_row) <= if paired { 2 } else { 1 } {
                                0
                            } else {
                                row_gap
                            },
                        });
                    }
                }
            }
        }
        specs.sort_by_key(|&s| (self.production_area(s), s));
        specs.dedup();
        specs
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Score {
    area: u64,
    entities: usize,
    poles: usize,
    perimeter: u64,
    wire_millitiles: u64,
}

impl Score {
    fn of(factory: &GeneratedFactory) -> Self {
        let entities = &factory.document.blueprint.entities;
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for e in entities {
            let [w, h] = factory.entity_sizes[&e.name];
            x0 = x0.min(e.position.x - f64::from(w) / 2.0);
            x1 = x1.max(e.position.x + f64::from(w) / 2.0);
            y0 = y0.min(e.position.y - f64::from(h) / 2.0);
            y1 = y1.max(e.position.y + f64::from(h) / 2.0);
        }
        let width = (x1 - x0).round() as u64;
        let height = (y1 - y0).round() as u64;
        let wire_length: f64 = factory
            .document
            .blueprint
            .wires
            .iter()
            .map(|w| {
                super::power::distance(
                    entities[(w[0] - 1) as usize].position,
                    entities[(w[2] - 1) as usize].position,
                )
            })
            .sum();
        Self {
            area: width * height,
            entities: entities.len(),
            poles: factory.power.pole_count,
            perimeter: 2 * (width + height),
            wire_millitiles: (wire_length * 1000.0).round() as u64,
        }
    }
}

pub(super) fn optimize(
    dimensions: Dimensions,
    mut build_and_validate: impl FnMut(LayoutSpec) -> Result<GeneratedFactory>,
) -> Result<GeneratedFactory> {
    let legacy = dimensions.legacy();
    // Preserve the former template as a fallback and a quality upper bound.
    // Failure of that template must not exclude a feasible compact candidate.
    let (mut best, mut failure) = match build_and_validate(legacy) {
        Ok(factory) => (Some((Score::of(&factory), factory)), None),
        Err(error) => (None, Some(error)),
    };
    for spec in dimensions.candidates() {
        if spec == legacy
            || best
                .as_ref()
                .is_some_and(|(score, _)| dimensions.production_area(spec) > score.area)
        {
            continue;
        }
        match build_and_validate(spec) {
            Ok(factory) => {
                let score = Score::of(&factory);
                if best.as_ref().is_none_or(|(old, _)| score < *old) {
                    best = Some((score, factory));
                }
            }
            Err(error) => {
                if failure.is_none() {
                    failure = Some(error);
                }
            }
        }
    }
    best.map(|(_, factory)| factory).ok_or_else(|| {
        failure
            .expect("at least the legacy candidate was attempted")
            .context("No feasible layout among the candidate templates")
    })
}
