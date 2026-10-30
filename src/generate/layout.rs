//! Bounded template search. Every scored candidate includes its actual power network.

use std::collections::BTreeSet;

use anyhow::Result;

use super::{GeneratedFactory, GenerationControl};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct LayoutSpec {
    pub paired: bool,
    pub per_row: usize,
    pub machine_gap: u32,
    /// Empty tile rows between two independent belt/machine bands.
    pub row_gap: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn large_dimensions() -> Dimensions {
        Dimensions {
            count: 739,
            max_per_row: 39,
            legacy_per_row: 8,
            input_belts: 3,
            allow_paired: false,
            machine_limit: None,
            width_limit: None,
            machine: [3, 3],
            pole: [1, 1],
        }
    }

    #[test]
    fn starts_with_compact_templates_and_keeps_legacy_fallback() {
        let dimensions = large_dimensions();
        let legacy = dimensions.legacy();
        let smallest = dimensions.candidates()[0];
        let mut attempted = Vec::new();
        assert!(
            optimize(dimensions, &GenerationControl::default(), |spec, _| {
                attempted.push(spec);
                anyhow::bail!("infeasible candidate")
            })
            .is_err()
        );
        assert_eq!(attempted[0], smallest);
        assert_ne!(attempted[0], legacy);
        assert!(attempted.contains(&legacy));
    }

    #[test]
    fn cancellation_is_not_swallowed_as_an_infeasible_candidate() {
        let control = GenerationControl::default();
        let mut attempts = 0;
        let result = optimize(large_dimensions(), &control, |_, _| {
            attempts += 1;
            control.cancel();
            anyhow::bail!("Generation cancelled")
        });
        assert!(result.unwrap_err().to_string().contains("cancelled"));
        assert_eq!(attempts, 1);
    }
}

pub(super) struct Dimensions {
    pub count: usize,
    pub max_per_row: usize,
    pub legacy_per_row: usize,
    pub input_belts: u32,
    pub allow_paired: bool,
    pub machine_limit: Option<usize>,
    pub width_limit: Option<u32>,
    pub machine: [u32; 2],
    pub pole: [u32; 2],
}

impl Dimensions {
    fn legacy(&self) -> LayoutSpec {
        self.bound(LayoutSpec {
            paired: false,
            per_row: self.legacy_per_row,
            machine_gap: self.pole[0] + 2,
            row_gap: self.pole[1] + 4,
        })
    }

    fn bound(&self, mut spec: LayoutSpec) -> LayoutSpec {
        spec.per_row = spec.per_row.min(self.machine_limit.unwrap_or(64));
        if let Some(width) = self.width_limit {
            let n = (width - 4 + spec.machine_gap) / (self.machine[0] + spec.machine_gap);
            spec.per_row = spec.per_row.min(n as usize);
        }
        spec
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
            if paired && !self.allow_paired {
                continue;
            }
            for &per_row in &lengths {
                let per_row = if paired {
                    per_row.min(self.count.div_ceil(2))
                } else {
                    per_row
                };
                for &machine_gap in &gaps {
                    for &row_gap in &row_gaps {
                        specs.push(self.bound(LayoutSpec {
                            paired,
                            per_row,
                            machine_gap: if self.count == 1 { 0 } else { machine_gap },
                            row_gap: if self.count.div_ceil(per_row) <= if paired { 2 } else { 1 } {
                                0
                            } else {
                                row_gap
                            },
                        }));
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
            let [mut w, mut h] = factory.entity_sizes[&e.name];
            if matches!(e.direction, 4 | 12) {
                std::mem::swap(&mut w, &mut h);
            }
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
    control: &GenerationControl,
    mut build_and_validate: impl FnMut(LayoutSpec, Option<u64>) -> Result<GeneratedFactory>,
) -> Result<GeneratedFactory> {
    let legacy = dimensions.legacy();
    let mut candidates = dimensions.candidates();
    if !candidates.contains(&legacy) {
        candidates.push(legacy);
    }
    candidates.sort_by_key(|&s| (dimensions.production_area(s), s));
    control.update(|p| {
        p.machines = dimensions.count;
        p.candidates = candidates.len();
    })?;
    // Start with compact templates. The legacy layout remains a candidate, but
    // need not build an expensive sprawling power network just to set a bound.
    let mut best: Option<(Score, GeneratedFactory)> = None;
    let mut failure = None;
    for spec in candidates {
        control.check()?;
        control.update(|p| p.candidate += 1)?;
        if best
            .as_ref()
            .is_some_and(|(score, _)| dimensions.production_area(spec) > score.area)
        {
            continue;
        }
        match build_and_validate(spec, best.as_ref().map(|(s, _)| s.area)) {
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
    control.check()?;
    best.map(|(_, factory)| factory).ok_or_else(|| {
        failure
            .expect("at least the legacy candidate was attempted")
            .context("No feasible layout among the candidate templates")
    })
}
