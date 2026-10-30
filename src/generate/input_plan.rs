//! Assign ingredients to physical belts and their two independent lanes.

pub(super) struct InputPlan {
    pub belts: Vec<usize>,
    pub reaches: Vec<u32>,
    pub lanes: Vec<&'static str>,
    pub belt_count: usize,
}

impl InputPlan {
    pub fn new(ingredients: &[(String, f64)], planned: f64, output: f64, belt_rate: f64) -> Self {
        if ingredients.len() == 3 {
            // Reserve the full factory's flow (including operating headroom),
            // so a compact row layout cannot hide an overloaded shared inlet.
            let pair = [(0, 1), (0, 2), (1, 2)]
                .into_iter()
                .filter(|&(a, b)| {
                    [a, b].into_iter().all(|i| {
                        planned * ingredients[i].1 / output <= belt_rate * 0.5 * 0.85 + 1e-8
                    })
                })
                .min_by(|&(a, b), &(c, d)| {
                    (ingredients[a].1 + ingredients[b].1)
                        .total_cmp(&(ingredients[c].1 + ingredients[d].1))
                });
            if let Some((a, b)) = pair {
                let mut plan = Self {
                    belts: vec![1; 3],
                    reaches: vec![2; 3],
                    lanes: vec!["both"; 3],
                    belt_count: 2,
                };
                for (i, lane) in [(a, "left"), (b, "right")] {
                    plan.belts[i] = 0;
                    plan.reaches[i] = 1;
                    plan.lanes[i] = lane;
                }
                return plan;
            }
        }
        Self {
            belts: (0..ingredients.len()).collect(),
            reaches: (0..ingredients.len())
                .map(|i| if i == 0 { 1 } else { 2 })
                .collect(),
            lanes: vec!["both"; ingredients.len()],
            belt_count: ingredients.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::InputPlan;

    #[test]
    fn packing_accounts_for_each_lane_headroom_belt_speed_and_product_amount() {
        let ingredients = vec![("a".into(), 2.0), ("b".into(), 4.0), ("c".into(), 2.0)];
        for (rate, output, belt, count) in [
            (3.035, 1.0, 15.0, 2),
            (3.036, 1.0, 15.0, 3),
            (4.0, 1.0, 30.0, 2),
            (4.0, 2.0, 15.0, 2),
        ] {
            let plan = InputPlan::new(&ingredients, rate * 1.05, output, belt);
            assert_eq!(plan.belt_count, count);
        }
    }

    #[test]
    fn pair_selection_is_independent_of_ingredient_order() {
        for amounts in [[8.0, 1.0, 2.0], [1.0, 8.0, 2.0], [1.0, 2.0, 8.0]] {
            let ingredients: Vec<_> = amounts
                .iter()
                .enumerate()
                .map(|(i, &n)| (i.to_string(), n))
                .collect();
            let plan = InputPlan::new(&ingredients, 1.05, 1.0, 15.0);
            assert_eq!(plan.belt_count, 2);
            for (i, amount) in amounts.into_iter().enumerate() {
                assert_eq!(plan.lanes[i] == "both", amount == 8.0);
            }
        }
    }
}
