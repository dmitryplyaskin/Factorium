//! Single-recipe solid production lines. Layout and power use game prototypes.

mod infrastructure;
mod infrastructure_check;
mod layout;
mod power;
mod rows;
mod transport;
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

pub(crate) fn inserter_observations(
    data: &GameData,
    f: &GeneratedFactory,
) -> Result<Vec<serde_json::Value>> {
    infrastructure::observed_inserters(data, f)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactoryRequest {
    pub product: String,
    pub rate: f64,
    pub recipe: Option<String>,
    pub machine: String,
    pub belt: String,
    pub inserter: String,
    #[serde(default)]
    pub secondary_inserter: Option<String>,
    pub pole: String,
    #[serde(default)]
    pub common_input: bool,
    #[serde(default)]
    pub reverse_output: bool,
    #[serde(default)]
    pub output_chest: Option<String>,
    #[serde(default)]
    pub input_markers: bool,
    #[serde(default)]
    pub max_machines_per_row: Option<usize>,
    #[serde(default)]
    pub max_row_width: Option<u32>,
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
            secondary_inserter: None,
            pole: "small-electric-pole".into(),
            common_input: false,
            reverse_output: false,
            output_chest: None,
            input_markers: false,
            max_machines_per_row: None,
            max_row_width: None,
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
    #[serde(default = "east")]
    pub direction: u8,
}

fn east() -> u8 {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusLine {
    pub source: BeltPort,
    pub targets: Vec<u32>,
    pub entities: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBus {
    pub splitter: String,
    pub underground: String,
    pub lines: Vec<BusLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputStorage {
    pub output_entity: u32,
    pub belts: Vec<u32>,
    pub inserters: Vec<u32>,
    pub chests: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMarker {
    pub entity_number: u32,
    pub target_entity: u32,
    pub item: String,
    pub rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductionRow {
    pub machines: Vec<u32>,
    pub input: BeltPort,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_inputs: Vec<BeltPort>,
    pub output: BeltPort,
}

impl ProductionRow {
    pub fn input_ports(&self) -> impl Iterator<Item = &BeltPort> {
        std::iter::once(&self.input).chain(&self.additional_inputs)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputRequirement {
    pub item: String,
    pub per_second: f64,
    pub supplied_per_second: f64,
    pub inserter: String,
    pub inserter_items_per_second: f64,
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
    /// Compatibility alias for the first ingredient; see input_requirements().
    pub input_item: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<InputRequirement>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<InputBus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub storage: Vec<OutputStorage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<InputMarker>,
}

impl GeneratedFactory {
    /// Includes a compatibility view for previously saved single-input projects.
    pub fn input_requirements(&self) -> Vec<InputRequirement> {
        if !self.inputs.is_empty() {
            return self.inputs.clone();
        }
        vec![InputRequirement {
            item: self.input_item.clone(),
            per_second: self.input_per_second,
            supplied_per_second: self.supplied_input_per_second,
            inserter: self.request.inserter.clone(),
            inserter_items_per_second: self.inserter_items_per_second,
        }]
    }

    pub fn input_ports(&self) -> impl Iterator<Item = &BeltPort> {
        self.rows.iter().flat_map(ProductionRow::input_ports)
    }

    pub fn external_inputs(&self) -> Vec<BeltPort> {
        self.bus
            .as_ref()
            .map(|bus| bus.lines.iter().map(|l| l.source.clone()).collect())
            .unwrap_or_else(|| self.input_ports().cloned().collect())
    }

    /// Physical output ports, with row contributions combined on shared belts.
    /// Each row's output still records its own rate and assigned lane.
    pub fn output_ports(&self) -> Vec<BeltPort> {
        let mut ports: Vec<BeltPort> = Vec::new();
        for row in &self.rows {
            if let Some(port) = ports
                .iter_mut()
                .find(|p| p.entity_number == row.output.entity_number)
            {
                port.rate += row.output.rate;
                if port.lanes != row.output.lanes {
                    port.lanes = "both".into();
                }
            } else {
                ports.push(row.output.clone());
            }
        }
        ports
    }

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
            let size = if matches!(e.direction, 4 | 12) {
                [size[1], size[0]]
            } else {
                size
            };
            let (w, h) = (f64::from(size[0]) - 0.1, f64::from(size[1]) - 0.1);
            let color = if e.name == self.request.machine {
                "#4eaa96"
            } else if e.name == self.request.belt {
                "#d1af53"
            } else if e.name == self.request.inserter
                || self
                    .input_requirements()
                    .iter()
                    .any(|r| r.inserter == e.name)
            {
                "#6aaff0"
            } else if e.name == "constant-combinator" {
                "#a5bce1"
            } else if Some(e.name.as_str()) == self.request.output_chest.as_deref() {
                "#ba8967"
            } else if self
                .bus
                .as_ref()
                .is_some_and(|b| e.name == b.splitter || e.name == b.underground)
            {
                "#c38d43"
            } else {
                "#e9dbb9"
            };
            svg.push_str(&format!("<rect x=\"{}\" y=\"{}\" width=\"{w}\" height=\"{h}\" fill=\"{color}\" stroke=\"#18202a\" stroke-width=\"0.08\"/>",e.position.x-w/2.0,e.position.y-h/2.0));
            if e.name == self.request.belt {
                svg.push_str(&format!("<path d=\"M -0.25 0 L 0.25 0 L 0.05 -0.2 M 0.25 0 L 0.05 0.2\" transform=\"translate({} {}) rotate({})\" stroke=\"#f2df99\" fill=\"none\" stroke-width=\"0.06\"/>",e.position.x,e.position.y,i32::from(e.direction)*45/2-90));
            }
        }
        for (i, port) in self.external_inputs().iter().enumerate() {
            let item = port
                .item
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            svg.push_str(&format!("<text x=\"{}\" y=\"{}\" fill=\"#85d9ba\" font-size=\"0.5\" text-anchor=\"end\">IN {}: {} {:.3}/s</text>",port.position.x-1.0,port.position.y-0.8,i+1,item,port.rate));
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

pub(crate) struct RecipeFlow {
    pub ingredients: Vec<(String, f64)>,
    pub output: f64,
}

pub(crate) fn inserter_reach(e: &Entity, reach: u32) -> bool {
    let props = &e.prototype.properties;
    let Some(pickup) = props.get("inserter_pickup_position") else {
        return false;
    };
    let Some(drop) = props.get("inserter_drop_position") else {
        return false;
    };
    e.prototype.prototype_type == "inserter"
        && e.tile_width == 1
        && e.tile_height == 1
        && props.contains_key("electric_energy_source")
        && pickup[0].as_f64() == Some(0.0)
        && pickup[1].as_f64() == Some(-f64::from(reach))
        && drop[0].as_f64() == Some(0.0)
        && drop[1]
            .as_f64()
            .is_some_and(|v| (f64::from(reach)..f64::from(reach) + 0.5).contains(&v))
}

pub(crate) fn supported_recipe(
    data: &GameData,
    r: &Recipe,
    machine: &Entity,
) -> Result<RecipeFlow> {
    ensure!(
        !r.prototype.hidden,
        "Hidden recipe {} is not selectable",
        r.prototype.name
    );
    ensure!(
        (1..=2).contains(&r.ingredients.len()) && r.products.len() == 1,
        "Recipe {} must have one or two ingredients and exactly one product",
        r.prototype.name
    );
    let b = &r.products[0];
    ensure!(
        r.ingredients.iter().all(|a| a.kind == "item") && b.kind == "item",
        "Recipe {} uses fluids or non-item products",
        r.prototype.name
    );
    ensure!(
        r.ingredients.iter().all(|a| a.name != b.name),
        "Self-returning recipe {} is not supported",
        r.prototype.name
    );
    ensure!(
        r.ingredients
            .iter()
            .all(|a| a.amount.is_finite() && a.amount > 0.0),
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
    for a in &r.ingredients {
        stable_item(data, &a.name)?;
    }
    if r.ingredients.len() == 2 {
        ensure!(
            r.ingredients[0].name != r.ingredients[1].name,
            "Ingredients must have distinct item types"
        );
        ensure!(
            machine.tile_width >= 2 && machine.tile_height >= 2,
            "Two-input template needs a machine at least 2x2 tiles"
        );
        ensure!(
            data.entities.values().any(|e| inserter_reach(e, 2)),
            "Two-input template needs an electric two-tile-reach inserter"
        );
    }
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
    Ok(RecipeFlow {
        ingredients: r
            .ingredients
            .iter()
            .map(|a| (a.name.clone(), a.amount))
            .collect(),
        output: amount + (amount - ignored) * productivity,
    })
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
                "No supported one/two-input recipe for {} on {}; specify --recipe for detailed diagnostics",
                request.product,
                request.machine
            ),
            _ => bail!(
                "Several supported recipes produce {}; select --recipe",
                request.product
            ),
        }
    };
    let flow = supported_recipe(data, recipe, machine)?;
    let (input_item, a) = &flow.ingredients[0];
    let b = flow.output;
    let secondary = if flow.ingredients.len() == 2 {
        let selected = if let Some(name) = &request.secondary_inserter {
            entity(data, name)?
        } else {
            data.entities
                .get("long-handed-inserter")
                .filter(|e| inserter_reach(e, 2))
                .or_else(|| data.entities.values().find(|e| inserter_reach(e, 2)))
                .context("No electric long-handed inserter available")?
        };
        ensure!(
            inserter_reach(selected, 2),
            "Select an electric two-tile-reach secondary inserter"
        );
        Some(selected)
    } else {
        None
    };
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
    let secondary_rate = if let Some(e) = secondary {
        let r = stat(e, "inserter_rotation_speed")?;
        let x = stat(e, "inserter_extension_speed")?;
        ensure!(
            r > 0.0 && x > 0.0,
            "Secondary inserter speeds must be positive"
        );
        0.8 * 60.0
            / (2.0 * (0.5 / r).ceil()
                + 2.0 * (2.0 / x).ceil()
                + 2.0 * (1.0 / (4.0 * belt_speed)).ceil()
                + 4.0)
    } else {
        0.0
    };
    let mut cell_rate = (crafts * b).min(transfer);
    for (i, (_, amount)) in flow.ingredients.iter().enumerate() {
        let input_transfer = if i == 1 {
            secondary_rate
        } else if secondary.is_some() {
            arm_rate
        } else {
            transfer
        };
        cell_rate = cell_rate.min(input_transfer * b / amount);
    }
    let belt_rate = belt_speed * 480.0;
    let row_cap = flow
        .ingredients
        .iter()
        .fold(belt_rate * 0.5 * 0.85, |cap, (_, amount)| {
            cap.min(belt_rate * 0.85 * b / amount)
        });
    let legacy_per_row = ((row_cap / cell_rate).floor() as usize).clamp(1, 8);
    let cell_rate = cell_rate.min(row_cap);
    let planned = request.rate * 1.05;
    ensure!(planned.is_finite(), "Output rate is too large");
    let count = (planned / cell_rate).ceil();
    ensure!(
        count <= 1024.0,
        "Request exceeds prototype budget (1024 machines); reduce the rate or choose faster equipment"
    );
    let count = count as usize;
    ensure!(
        request
            .max_machines_per_row
            .is_none_or(|n| (1..=64).contains(&n)),
        "Row machine limit must be 1..64"
    );
    ensure!(
        request
            .max_row_width
            .is_none_or(|n| n >= machine.tile_width + 4 && n <= 10000),
        "Row width must be at least {} tiles (machine plus belt ends), and at most 10000",
        machine.tile_width + 4
    );
    // Row limits follow the supplied flow per machine. The final machine need
    // not run at full modelled speed: rounding its count up must not force an
    // unnecessary extra output belt when two lanes can carry the whole target.
    let max_per_row = ((row_cap / (planned / count as f64)).floor() as usize).clamp(1, 64);
    layout::optimize(
        layout::Dimensions {
            count,
            max_per_row,
            legacy_per_row,
            input_belts: flow.ingredients.len() as u32,
            machine_limit: request.max_machines_per_row,
            width_limit: request.max_row_width,
            machine: [machine.tile_width, machine.tile_height],
            pole: [pole.tile_width, pole.tile_height],
        },
        |spec, best_area| {
            let mut doc = BlueprintDocument::new(
                &data.game_version,
                format!("Factorium: {} >= {}/s", request.product, request.rate),
            )?;
            let mut rows = rows::Production {
                request,
                machine,
                recipe: &recipe.prototype.name,
                inputs: &flow.ingredients,
                output_amount: b,
                secondary,
                arm_columns: &arm_columns,
                count,
                planned,
            }
            .place(&mut doc, spec);
            let (bus, storage, markers) = infrastructure::place(
                data,
                request,
                &mut rows,
                &mut doc,
                infrastructure::Rates {
                    belt: belt_rate,
                    arm: arm_rate,
                    machine_output: crafts * b,
                },
            )?;
            ensure!(
                doc.blueprint.entities.len() <= 50000,
                "Layout exceeds prototype entity budget"
            );
            if let Some(best_area) = best_area {
                // Infrastructure changes the footprint substantially. Reject
                // candidates that already exceed the incumbent before the
                // more expensive power-network search. Poles can only expand it.
                let (mut x0, mut y0, mut x1, mut y1) = (
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                );
                for e in &doc.blueprint.entities {
                    let p = entity(data, &e.name)?;
                    let (w, h) = if matches!(e.direction, 4 | 12) {
                        (p.tile_height, p.tile_width)
                    } else {
                        (p.tile_width, p.tile_height)
                    };
                    x0 = x0.min(e.position.x - f64::from(w) / 2.0);
                    x1 = x1.max(e.position.x + f64::from(w) / 2.0);
                    y0 = y0.min(e.position.y - f64::from(h) / 2.0);
                    y1 = y1.max(e.position.y + f64::from(h) / 2.0);
                }
                ensure!(
                    ((x1 - x0) * (y1 - y0)).round() as u64 <= best_area,
                    "Layout already exceeds the best candidate footprint"
                );
            }
            let mut power = power::place_and_wire(data, &mut doc, &request.pole, &rows)?;
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
                doc.blueprint.entities.iter().try_fold(0.0, |total, e| {
                    let proto = entity(data, &e.name)?;
                    Ok::<_, anyhow::Error>(
                        total
                            + if proto
                                .prototype
                                .properties
                                .contains_key("electric_energy_source")
                            {
                                demand(proto)?
                            } else {
                                0.0
                            },
                    )
                })?;
            let input_summary = flow
                .ingredients
                .iter()
                .map(|(item, amount)| {
                    format!(
                        "{item}: target {:.3}/s, supply {:.3}/s",
                        request.rate * amount / b,
                        planned * amount / b
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            doc.blueprint.description = format!(
                "Output >= {:.3} {}/s; inputs: {} (5% headroom). Normal quality, no modules, stack size 1. Connect external power to pole #{} ({:.1},{:.1}); installed load <= {:.0} W. {} input belts and {} output belts. See factory.json for port coordinates. Throughput is modelled until verified in game.",
                request.rate,
                request.product,
                input_summary,
                power.connection_entity,
                power.connection_position.x,
                power.connection_position.y,
                power.installed_power_watts,
                bus.as_ref()
                    .map(|b| b.lines.len())
                    .unwrap_or_else(|| rows.iter().map(|r| r.input_ports().count()).sum::<usize>()),
                rows.iter()
                    .map(|r| r.output.entity_number)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
            );
            if bus.is_some() && !request.reverse_output {
                doc.blueprint.description.push_str(" Shared bus: connect separate ingredient belts from the north; splitters redistribute flow between rows.");
            }
            if request.reverse_output {
                doc.blueprint.description.push_str(" Inputs and outputs are on the west side. Supply ingredients from the west; continue output belts to the west.");
            }
            if !storage.is_empty() {
                doc.blueprint.description.push_str(
                    " Output goes into finite chest storage; production stops when full.",
                );
            }
            let entity_sizes = doc
                .blueprint
                .entities
                .iter()
                .map(|e| {
                    let p = entity(data, &e.name)?;
                    Ok((e.name.clone(), [p.tile_width, p.tile_height]))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            let factory = GeneratedFactory {
                schema_version: 1,
                game_version: data.game_version.clone(),
                mods: data.mods.clone(),
                request: request.clone(),
                recipe: recipe.prototype.name.clone(),
                input_item: input_item.clone(),
                inputs: flow
                    .ingredients
                    .iter()
                    .enumerate()
                    .map(|(i, (item, amount))| InputRequirement {
                        item: item.clone(),
                        per_second: request.rate * amount / b,
                        supplied_per_second: planned * amount / b,
                        inserter: if i == 1 {
                            secondary.unwrap().prototype.name.clone()
                        } else {
                            request.inserter.clone()
                        },
                        inserter_items_per_second: if i == 1 { secondary_rate } else { arm_rate },
                    })
                    .collect(),
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
                bus,
                storage,
                markers,
            };
            validate_factory(data, &factory)?;
            Ok(factory)
        },
    )
}
