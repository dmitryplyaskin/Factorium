//! Single-recipe solid production lines. Layout and power use game prototypes.

mod power;
mod validate;

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    GameData,
    blueprint::BlueprintDocument,
    model::{Entity, Position, Recipe},
};

pub use validate::validate_factory;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactoryRequest {
    pub product: String,
    pub rate: f64,
    pub recipe: Option<String>,
    pub machine: String,
    pub belt: String,
    pub inserter: String,
    pub pole: String,
}

impl FactoryRequest {
    pub fn new(product: impl Into<String>, rate: f64) -> Self {
        Self {
            product: product.into(),
            rate,
            recipe: None,
            machine: "assembling-machine-2".into(),
            belt: "transport-belt".into(),
            inserter: "fast-inserter".into(),
            pole: "small-electric-pole".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeltPort {
    pub entity_number: u32,
    pub position: Position,
    pub item: String,
    pub rate: f64,
    pub lanes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionRow {
    pub machines: Vec<u32>,
    pub input: BeltPort,
    pub output: BeltPort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerPlan {
    pub pole: String,
    pub supply_area_distance: f64,
    pub max_wire_distance: f64,
    pub pole_count: usize,
    /// Connect external electricity to this pole; generation is outside the blueprint.
    pub connection_entity: u32,
    pub connection_position: Position,
    /// Conservative installed-load estimate, including idle drain, in watts.
    pub installed_power_watts: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFactory {
    pub schema_version: u32,
    pub game_version: String,
    pub mods: BTreeMap<String, String>,
    pub request: FactoryRequest,
    pub recipe: String,
    pub input_item: String,
    pub machine_count: usize,
    pub input_per_second: f64,
    /// Includes 5% operating headroom; this is the supply used in the game test.
    pub supplied_input_per_second: f64,
    pub planned_output_per_second: f64,
    pub machine_crafts_per_second: f64,
    pub inserter_items_per_second: f64,
    pub belt_items_per_second: f64,
    pub power: PowerPlan,
    pub rows: Vec<ProductionRow>,
    pub entity_sizes: BTreeMap<String, [u32; 2]>,
    pub document: BlueprintDocument,
}

impl GeneratedFactory {
    pub fn write_to(&self, directory: &Path) -> Result<()> {
        fs::create_dir_all(directory)?;
        let old_report = directory.join("verification.json");
        if old_report.exists() {
            fs::remove_file(old_report)?;
        }
        // Keep the editable blueprint JSON separate from the plan and copyable string.
        save_json(&directory.join("factory.json"), self)?;
        save_json(&directory.join("blueprint.json"), &self.document)?;
        fs::write(
            directory.join("blueprint.txt"),
            format!("{}\n", self.document.encode()?),
        )?;
        fs::write(directory.join("preview.svg"), self.preview_svg())?;
        Ok(())
    }

    pub fn preview_svg(&self) -> String {
        let entities = &self.document.blueprint.entities;
        let margin = self.power.supply_area_distance.max(
            self.entity_sizes
                .values()
                .map(|s| f64::from(s[0].max(s[1])) / 2.0)
                .fold(0.0, f64::max),
        ) + 1.0;
        let min_x = entities
            .iter()
            .map(|e| e.position.x)
            .fold(f64::INFINITY, f64::min)
            - margin;
        let min_y = entities
            .iter()
            .map(|e| e.position.y)
            .fold(f64::INFINITY, f64::min)
            - margin;
        let max_x = entities
            .iter()
            .map(|e| e.position.x)
            .fold(f64::NEG_INFINITY, f64::max)
            + margin;
        let max_y = entities
            .iter()
            .map(|e| e.position.y)
            .fold(f64::NEG_INFINITY, f64::max)
            + margin;
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{min_x} {min_y} {} {}\" width=\"1200\"><rect x=\"{min_x}\" y=\"{min_y}\" width=\"100%\" height=\"100%\" fill=\"#18202a\"/>",
            max_x - min_x,
            max_y - min_y
        );
        for e in entities.iter().filter(|e| e.name == self.request.pole) {
            let r = self.power.supply_area_distance;
            svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#f9bd5b\" fill-opacity=\"0.035\" stroke=\"#f9bd5b\" stroke-opacity=\"0.2\" stroke-width=\"0.04\"/>",e.position.x-r,e.position.y-r,2.0*r,2.0*r));
        }
        for wire in &self.document.blueprint.wires {
            let a = entities[(wire[0] - 1) as usize].position;
            let b = entities[(wire[2] - 1) as usize].position;
            svg.push_str(&format!(
                "<path d=\"M {} {} L {} {}\" stroke=\"#f9bd5b\" stroke-width=\"0.08\"/>",
                a.x, a.y, b.x, b.y
            ));
        }
        for e in entities {
            let size = self.entity_sizes[&e.name];
            let (w, h) = (f64::from(size[0]) - 0.1, f64::from(size[1]) - 0.1);
            let color = if e.name == self.request.machine {
                "#4eaa96"
            } else if e.name == self.request.belt {
                "#d1af53"
            } else if e.name == self.request.inserter {
                "#6aaff0"
            } else {
                "#e9dbb9"
            };
            svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" fill=\"{color}\" stroke=\"#18202a\" stroke-width=\"0.08\"/>",e.position.x-w/2.0,e.position.y-h/2.0));
        }
        svg.push_str("</svg>");
        svg
    }
}

pub(crate) fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = BufWriter::new(File::create(path)?);
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

pub(crate) fn entity<'a>(data: &'a GameData, name: &str) -> Result<&'a Entity> {
    data.entities
        .get(name)
        .with_context(|| format!("Unknown entity: {name}"))
}

pub(crate) fn stat(e: &Entity, name: &str) -> Result<f64> {
    let value = e
        .quality_stats
        .get("normal")
        .and_then(|v| v.get(name))
        .and_then(|v| v.as_f64())
        .with_context(|| format!("Missing normal-quality {name} for {}", e.prototype.name))?;
    ensure!(value.is_finite(), "Invalid {name} for {}", e.prototype.name);
    Ok(value)
}

fn property_number(e: &Entity, key: &str) -> Result<f64> {
    let value = e
        .prototype
        .properties
        .get(key)
        .and_then(|v| v.as_f64())
        .with_context(|| format!("Missing {key} for {}", e.prototype.name))?;
    ensure!(value.is_finite(), "Invalid {key}");
    Ok(value)
}

fn stable_item(data: &GameData, name: &str) -> Result<()> {
    let item = data
        .items
        .get(name)
        .with_context(|| format!("Unknown item: {name}"))?;
    let spoil = item
        .prototype
        .properties
        .get("quality_stats")
        .and_then(|v| v.get("normal"))
        .and_then(|v| v.get("spoil_ticks"))
        .and_then(|v| v.as_u64())
        .context("Missing item spoilage data; re-extract the game catalog")?;
    ensure!(spoil == 0, "Spoiling item {name} is not supported yet");
    Ok(())
}

pub(crate) fn supported_recipe(
    data: &GameData,
    r: &Recipe,
    machine: &Entity,
) -> Result<(String, f64, f64)> {
    ensure!(
        !r.prototype.hidden,
        "Hidden recipe {} is not selectable",
        r.prototype.name
    );
    ensure!(
        r.ingredients.len() == 1 && r.products.len() == 1,
        "Recipe {} must have exactly one ingredient and one product",
        r.prototype.name
    );
    let a = &r.ingredients[0];
    let b = &r.products[0];
    ensure!(
        a.kind == "item" && b.kind == "item",
        "Recipe {} uses fluids or non-item products",
        r.prototype.name
    );
    ensure!(
        a.name != b.name,
        "Self-returning recipe {} is not supported",
        r.prototype.name
    );
    ensure!(
        a.amount.is_finite() && a.amount > 0.0,
        "Invalid ingredient amount"
    );
    ensure!(
        b.independent_probability == 1.0
            && b.shared_probability.min == 0.0
            && b.shared_probability.max == 1.0,
        "Random product in recipe {} is not supported",
        r.prototype.name
    );
    let amount = b
        .amount
        .or_else(|| {
            if b.amount_min == b.amount_max {
                b.amount_min
            } else {
                None
            }
        })
        .context("Random output quantities are not supported")?;
    ensure!(amount.is_finite() && amount > 0.0, "Invalid product amount");
    for key in ["extra_count_fraction", "quality_change"] {
        ensure!(
            b.properties
                .get(key)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                == 0.0,
            "Product mechanic {key} is not supported"
        );
    }
    stable_item(data, &a.name)?;
    stable_item(data, &b.name)?;
    ensure!(
        r.categories
            .iter()
            .any(|c| machine.crafting_categories.get(c) == Some(&true)),
        "{} cannot craft {}: incompatible categories",
        machine.prototype.name,
        r.prototype.name
    );
    if let Some(fixed) = machine
        .prototype
        .properties
        .get("fixed_recipe")
        .and_then(|v| v.as_str())
    {
        ensure!(
            fixed == r.prototype.name,
            "Machine has fixed recipe {fixed}"
        );
    }
    // The initial environment is Nauvis. Respect every declared surface condition.
    for p in [&r.prototype.properties, &machine.prototype.properties] {
        if let Some(conditions) = p.get("surface_conditions").and_then(|v| v.as_array()) {
            for condition in conditions {
                let name = condition["property"]
                    .as_str()
                    .context("Invalid surface condition")?;
                let value = data
                    .space_locations
                    .get("nauvis")
                    .and_then(|v| v.properties.get("surface_properties"))
                    .and_then(|v| v.get(name))
                    .and_then(|v| v.as_f64())
                    .or_else(|| {
                        data.surface_properties
                            .get(name)
                            .and_then(|v| v.properties.get("default_value"))
                            .and_then(|v| v.as_f64())
                    })
                    .with_context(|| format!("Missing Nauvis property {name}"))?;
                ensure!(
                    condition["min"].as_f64().is_none_or(|min| value >= min)
                        && condition["max"].as_f64().is_none_or(|max| value <= max),
                    "Recipe/machine requires a different surface: {name}"
                );
            }
        }
    }
    let base = machine
        .prototype
        .properties
        .get("effect_receiver")
        .and_then(|v| v.get("base_effect"));
    for key in ["quality", "consumption"] {
        ensure!(
            base.and_then(|v| v.get(key))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                == 0.0,
            "Machine base effect {key} is not supported in the initial normal-quality power model"
        );
    }
    let productivity = base
        .and_then(|v| v.get("productivity"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .max(0.0)
        .min(
            r.prototype
                .properties
                .get("maximum_productivity")
                .and_then(|v| v.as_f64())
                .unwrap_or(f64::INFINITY),
        );
    let ignored = b
        .properties
        .get("ignored_by_productivity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, amount);
    Ok((
        a.name.clone(),
        a.amount,
        amount + (amount - ignored) * productivity,
    ))
}

/// Generate a candidate and independently check its geometry, wiring and row topology.
/// Throughput is conservatively modelled, not game-certified until `verify_in_game`.
pub fn generate_blueprint(data: &GameData, request: &FactoryRequest) -> Result<GeneratedFactory> {
    ensure!(
        request.rate.is_finite() && request.rate > 0.0,
        "Output rate must be finite and greater than zero"
    );
    let machine = entity(data, &request.machine)?;
    let belt = entity(data, &request.belt)?;
    let arm = entity(data, &request.inserter)?;
    let pole = entity(data, &request.pole)?;
    ensure!(
        matches!(
            machine.prototype.prototype_type.as_str(),
            "assembling-machine" | "furnace"
        ),
        "Only assembling machines and electric furnaces are supported"
    );
    for e in [machine, arm] {
        ensure!(
            e.prototype
                .properties
                .contains_key("electric_energy_source"),
            "{} needs unsupported fuel/energy",
            e.prototype.name
        );
    }
    ensure!(
        belt.prototype.prototype_type == "transport-belt"
            && belt.tile_width == 1
            && belt.tile_height == 1,
        "Select a 1x1 transport belt"
    );
    ensure!(
        arm.prototype.prototype_type == "inserter" && arm.tile_width == 1 && arm.tile_height == 1,
        "Select a 1x1 electric inserter"
    );
    let pickup = arm
        .prototype
        .properties
        .get("inserter_pickup_position")
        .context("Missing pickup position")?;
    let drop = arm
        .prototype
        .properties
        .get("inserter_drop_position")
        .context("Missing drop position")?;
    ensure!(
        pickup[0].as_f64() == Some(0.0)
            && pickup[1].as_f64() == Some(-1.0)
            && drop[0].as_f64() == Some(0.0)
            && drop[1].as_f64().is_some_and(|v| (1.0..1.5).contains(&v)),
        "This template requires a straight, one-tile-reach inserter"
    );
    ensure!(
        machine.tile_width > 0
            && machine.tile_height > 0
            && machine.tile_width <= 16
            && machine.tile_height <= 16,
        "Unsupported machine dimensions"
    );
    ensure!(
        machine
            .prototype
            .properties
            .get("flags")
            .is_none_or(|v| v.get("placeable-off-grid") != Some(&serde_json::Value::Bool(true))),
        "Off-grid machines are unsupported"
    );
    let recipe = if let Some(name) = &request.recipe {
        data.recipes
            .get(name)
            .with_context(|| format!("Unknown recipe: {name}"))?
    } else {
        let candidates: Vec<_> = data
            .recipes
            .values()
            .filter(|r| {
                r.products.iter().any(|p| p.name == request.product)
                    && supported_recipe(data, r, machine).is_ok()
            })
            .collect();
        match candidates.as_slice() {
            [only] => *only,
            [] => bail!(
                "No supported one-input/one-output recipe for {} on {}; specify --recipe for detailed diagnostics",
                request.product,
                request.machine
            ),
            _ => bail!(
                "Several supported recipes produce {}; select --recipe",
                request.product
            ),
        }
    };
    let (input_item, a, b) = supported_recipe(data, recipe, machine)?;
    ensure!(
        recipe.products[0].name == request.product,
        "Selected recipe does not produce {}",
        request.product
    );
    let speed_effect = machine
        .prototype
        .properties
        .get("effect_receiver")
        .and_then(|v| v.get("base_effect"))
        .and_then(|v| v.get("speed"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let craft_speed = stat(machine, "crafting_speed")? * (1.0 + speed_effect).max(0.2);
    let crafts = craft_speed / recipe.energy;
    let belt_speed = property_number(belt, "belt_speed")?;
    let rotation = stat(arm, "inserter_rotation_speed")?;
    let extension = stat(arm, "inserter_extension_speed")?;
    ensure!(
        crafts > 0.0 && belt_speed > 0.0 && rotation > 0.0 && extension > 0.0,
        "Equipment speeds must be positive"
    );
    // Deliberately conservative sequential motion plus belt-pickup latency. Stack
    // size is explicitly fixed to one, so research cannot silently change this plan.
    let cycle = 2.0 * (0.5 / rotation).ceil()
        + 2.0 * (1.0 / extension).ceil()
        + 2.0 * (1.0 / (4.0 * belt_speed)).ceil()
        + 4.0;
    let arm_rate = 0.8 * 60.0 / cycle;
    let arm_columns = if machine.tile_width == 1 {
        vec![0]
    } else {
        vec![0, machine.tile_width - 1]
    };
    let transfer = arm_rate * arm_columns.len() as f64;
    let cell_rate = (crafts * b).min(transfer * b / a).min(transfer);
    let belt_rate = belt_speed * 480.0;
    let row_cap = (belt_rate * 0.85 * b / a).min(belt_rate * 0.5 * 0.85);
    let per_row = ((row_cap / cell_rate).floor() as usize).clamp(1, 8);
    let cell_rate = cell_rate.min(row_cap);
    let planned = request.rate * 1.05;
    ensure!(planned.is_finite(), "Output rate is too large");
    let count = (planned / cell_rate).ceil();
    ensure!(
        count <= 1024.0,
        "Request exceeds prototype budget (1024 machines); reduce the rate or choose faster equipment"
    );
    let count = count as usize;
    let mut doc = BlueprintDocument::new(
        &data.game_version,
        format!("Factorium: {} >= {}/s", request.product, request.rate),
    )?;
    let mut rows = Vec::new();
    let mut remaining = count;
    let mut remaining_rate = planned;
    let gap = pole.tile_width as i32 + 2;
    let w = machine.tile_width as i32;
    let h = machine.tile_height as i32;
    while remaining > 0 {
        let n = remaining.min(per_row);
        let y = rows.len() as i32 * (h + pole.tile_height as i32 + 8);
        let width = n as i32 * (w + gap) - gap;
        let row_rate = remaining_rate.min(n as f64 * cell_rate);
        let mut ids = Vec::new();
        for i in 0..n {
            let x = i as i32 * (w + gap);
            let id = doc.add(
                &request.machine,
                f64::from(x) + f64::from(w) / 2.0,
                f64::from(y) + f64::from(h) / 2.0,
                0,
            );
            if machine.prototype.prototype_type == "assembling-machine" {
                doc.blueprint.entities[(id - 1) as usize].recipe =
                    Some(recipe.prototype.name.clone());
            }
            ids.push(id);
            for column in &arm_columns {
                for arm_y in [y - 1, y + h] {
                    let id = doc.add(
                        &request.inserter,
                        f64::from(x + *column as i32) + 0.5,
                        f64::from(arm_y) + 0.5,
                        0,
                    );
                    doc.blueprint.entities[(id - 1) as usize].override_stack_size = Some(1);
                }
            }
        }
        let mut in_id = 0;
        let mut out_id = 0;
        for x in -2..=width + 1 {
            let id = doc.add(&request.belt, f64::from(x) + 0.5, f64::from(y) - 1.5, 4);
            if x == -2 {
                in_id = id;
            }
            let id = doc.add(&request.belt, f64::from(x) + 0.5, f64::from(y + h) + 1.5, 4);
            if x == width + 1 {
                out_id = id;
            }
        }
        rows.push(ProductionRow {
            machines: ids,
            input: BeltPort {
                entity_number: in_id,
                position: doc.blueprint.entities[(in_id - 1) as usize].position,
                item: input_item.clone(),
                rate: row_rate * a / b,
                lanes: "both".into(),
            },
            output: BeltPort {
                entity_number: out_id,
                position: doc.blueprint.entities[(out_id - 1) as usize].position,
                item: request.product.clone(),
                rate: row_rate,
                lanes: "right".into(),
            },
        });
        remaining -= n;
        remaining_rate -= row_rate;
    }
    ensure!(
        doc.blueprint.entities.len() <= 50000,
        "Layout exceeds prototype entity budget"
    );
    let mut power = power::place_and_wire(data, &mut doc, &request.pole)?;
    let demand = |e: &Entity| -> Result<f64> {
        Ok((stat(e, "max_energy_usage")?
            + e.prototype
                .properties
                .get("electric_energy_source")
                .and_then(|v| v.get("drain"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0))
            * 60.0)
    };
    power.installed_power_watts =
        count as f64 * (demand(machine)? + 2.0 * arm_columns.len() as f64 * demand(arm)?);
    doc.blueprint.description = format!(
        "Output >= {:.3} {}/s; target input {:.3} {}/s; supply {:.3}/s including 5% headroom. Normal quality, no modules, stack size 1. Connect external power to pole #{} ({:.1},{:.1}); installed load <= {:.0} W. {} input/output belt pairs. See factory.json for port coordinates. Throughput is modelled until verified in game.",
        request.rate,
        request.product,
        request.rate * a / b,
        input_item,
        planned * a / b,
        power.connection_entity,
        power.connection_position.x,
        power.connection_position.y,
        power.installed_power_watts,
        rows.len()
    );
    let entity_sizes = [machine, belt, arm, pole]
        .into_iter()
        .map(|e| (e.prototype.name.clone(), [e.tile_width, e.tile_height]))
        .collect();
    let factory = GeneratedFactory {
        schema_version: 1,
        game_version: data.game_version.clone(),
        mods: data.mods.clone(),
        request: request.clone(),
        recipe: recipe.prototype.name.clone(),
        input_item,
        machine_count: count,
        input_per_second: request.rate * a / b,
        supplied_input_per_second: planned * a / b,
        planned_output_per_second: planned,
        machine_crafts_per_second: crafts,
        inserter_items_per_second: arm_rate,
        belt_items_per_second: belt_rate,
        power,
        rows,
        entity_sizes,
        document: doc,
    };
    validate_factory(data, &factory)?;
    Ok(factory)
}
