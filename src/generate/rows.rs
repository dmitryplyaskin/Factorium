//! Physical production rows, including mirrored pairs with a shared output belt.

use super::{BeltPort, FactoryRequest, ProductionRow, layout::LayoutSpec};
use crate::{blueprint::BlueprintDocument, model::Entity};

pub(super) struct Production<'a> {
    pub request: &'a FactoryRequest,
    pub machine: &'a Entity,
    pub recipe: &'a str,
    pub inputs: &'a [(String, f64)],
    pub output_amount: f64,
    pub secondary: Option<&'a Entity>,
    pub arm_columns: &'a [u32],
    pub count: usize,
    pub planned: f64,
}

impl Production<'_> {
    fn machines(
        &self,
        doc: &mut BlueprintDocument,
        n: usize,
        y: i32,
        gap: i32,
        mirrored: bool,
    ) -> Vec<u32> {
        let w = self.machine.tile_width as i32;
        let h = self.machine.tile_height as i32;
        let mut ids = Vec::new();
        for i in 0..n {
            let x = i as i32 * (w + gap);
            let id = doc.add(
                &self.request.machine,
                f64::from(x) + f64::from(w) / 2.0,
                f64::from(y) + f64::from(h) / 2.0,
                0,
            );
            if self.machine.prototype.prototype_type == "assembling-machine" {
                doc.blueprint.entities[(id - 1) as usize].recipe = Some(self.recipe.into());
            }
            ids.push(id);
            for column in self.arm_columns {
                for arm_y in [y - 1, y + h] {
                    let input_y = if mirrored { y + h } else { y - 1 };
                    let is_input = arm_y == input_y;
                    let arm_name = match (is_input, self.secondary) {
                        (true, Some(secondary)) if *column != self.arm_columns[0] => {
                            secondary.prototype.name.as_str()
                        }
                        _ => self.request.inserter.as_str(),
                    };
                    let id = doc.add(
                        arm_name,
                        f64::from(x + *column as i32) + 0.5,
                        f64::from(arm_y) + 0.5,
                        if mirrored { 8 } else { 0 },
                    );
                    doc.blueprint.entities[(id - 1) as usize].override_stack_size = Some(1);
                }
            }
        }
        ids
    }

    fn belt(&self, doc: &mut BlueprintDocument, y: f64, width: i32, direction: u8) -> (u32, u32) {
        let mut first = 0;
        let mut last = 0;
        for x in -2..=width + 1 {
            last = doc.add(&self.request.belt, f64::from(x) + 0.5, y, direction);
            if x == -2 {
                first = last;
            }
        }
        (first, last)
    }

    fn row(
        &self,
        doc: &BlueprintDocument,
        machines: Vec<u32>,
        inputs: Vec<u32>,
        output: u32,
        lane: &str,
    ) -> ProductionRow {
        // Spread supply over all machines instead of nearly starving the last
        // row. Each lane then has a meaningful independently measurable target.
        let rate = self.planned * machines.len() as f64 / self.count as f64;
        let mut ports: Vec<_> = inputs
            .iter()
            .zip(self.inputs)
            .map(|(&id, (item, amount))| BeltPort {
                entity_number: id,
                position: doc.blueprint.entities[(id - 1) as usize].position,
                item: item.clone(),
                rate: rate * amount / self.output_amount,
                lanes: "both".into(),
                direction: 4,
            })
            .collect();
        let input = ports.remove(0);
        ProductionRow {
            production_output: None,
            machines,
            input,
            additional_inputs: ports,
            output: BeltPort {
                entity_number: output,
                position: doc.blueprint.entities[(output - 1) as usize].position,
                item: self.request.product.clone(),
                rate,
                lanes: if self.request.reverse_output {
                    if lane == "left" { "right" } else { "left" }
                } else {
                    lane
                }
                .into(),
                direction: if self.request.reverse_output { 12 } else { 4 },
            },
        }
    }

    pub fn place(&self, doc: &mut BlueprintDocument, spec: LayoutSpec) -> Vec<ProductionRow> {
        let w = self.machine.tile_width as i32;
        let h = self.machine.tile_height as i32;
        let gap = spec.machine_gap as i32;
        let mut rows = Vec::new();
        let mut remaining = self.count;
        let mut y = 0;
        while remaining > 0 {
            let upper = remaining.min(spec.per_row);
            remaining -= upper;
            let lower = if spec.paired {
                remaining.min(spec.per_row)
            } else {
                0
            };
            remaining -= lower;
            let width = upper as i32 * (w + gap) - gap;
            let upper_machines = self.machines(doc, upper, y, gap, false);
            let lower_y = y + h + 3;
            let lower_machines = self.machines(doc, lower, lower_y, gap, true);
            let inputs = (0..self.inputs.len())
                .map(|i| self.belt(doc, f64::from(y) - 1.5 - i as f64, width, 4).0)
                .collect();
            let output_line = self.belt(
                doc,
                f64::from(y + h) + 1.5,
                width,
                if self.request.reverse_output { 12 } else { 4 },
            );
            let output = if self.request.reverse_output {
                output_line.0
            } else {
                output_line.1
            };
            rows.push(self.row(doc, upper_machines, inputs, output, "right"));
            if lower > 0 {
                let lower_width = lower as i32 * (w + gap) - gap;
                let inputs = (0..self.inputs.len())
                    .map(|i| {
                        self.belt(doc, f64::from(lower_y + h) + 1.5 + i as f64, lower_width, 4)
                            .0
                    })
                    .collect();
                rows.push(self.row(doc, lower_machines, inputs, output, "left"));
            }
            let input_belts = self.inputs.len() as i32;
            y += if lower > 0 {
                2 * h + 5 + 2 * input_belts
            } else {
                h + 3 + input_belts
            } + spec.row_gap as i32;
        }
        rows
    }
}
