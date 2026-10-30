//! Test the encoded artifact in Factorio with rate-limited boundary conditions.

use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use crate::{
    ExtractOptions, GameData, GeneratedFactory, extract::run_test_scenario, validate_factory,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationOptions {
    pub warmup_ticks: u32,
    /// Four equal measurement windows. No initial buffers are supplied.
    pub measurement_ticks: u32,
}

impl Default for SimulationOptions {
    fn default() -> Self {
        Self {
            warmup_ticks: 7200,
            measurement_ticks: 7200,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RowMeasurement {
    /// One-based row index in GeneratedFactory.rows.
    pub row_index: usize,
    pub output_entity: u32,
    pub lane: String,
    pub target_per_second: f64,
    pub measured_per_second: f64,
    pub delivered_items: u64,
    #[serde(deserialize_with = "crate::model::lua_vec")]
    pub window_rates: Vec<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InputMeasurement {
    pub row_index: usize,
    pub input_entity: u32,
    pub item: String,
    pub supplied_per_second: f64,
    pub measured_per_second: f64,
    pub inserted_items: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SimulationReport {
    pub passed: bool,
    pub game_version: String,
    #[serde(deserialize_with = "crate::model::lua_map")]
    pub mods: BTreeMap<String, String>,
    pub entity_count: usize,
    pub pole_count: usize,
    pub network_count: usize,
    #[serde(deserialize_with = "crate::model::lua_vec")]
    pub unpowered_entities: Vec<u32>,
    pub measured_output_per_second: f64,
    #[serde(deserialize_with = "crate::model::lua_vec")]
    pub window_rates: Vec<f64>,
    pub delivered_items: u64,
    pub inserted_items: u64,
    #[serde(default, deserialize_with = "crate::model::lua_vec")]
    pub input_deliveries: Vec<InputMeasurement>,
    #[serde(default, deserialize_with = "crate::model::lua_vec")]
    pub row_outputs: Vec<RowMeasurement>,
    pub warmup_ticks: u32,
    pub measurement_ticks: u32,
    #[serde(deserialize_with = "crate::model::lua_map")]
    pub machine_status_counts: BTreeMap<String, u32>,
    #[serde(deserialize_with = "crate::model::lua_vec")]
    pub errors: Vec<String>,
}

pub fn verify_in_game(
    data: &GameData,
    factory: &GeneratedFactory,
    game: &ExtractOptions,
    options: &SimulationOptions,
) -> Result<SimulationReport> {
    validate_factory(data, factory)?;
    ensure!(
        options.measurement_ticks >= 240 && options.measurement_ticks % 4 == 0,
        "Measurement duration must be >=240 ticks and divisible by four"
    );
    let ticks = options
        .warmup_ticks
        .checked_add(options.measurement_ticks)
        .and_then(|n| n.checked_add(2))
        .filter(|&n| n <= 3_600_000)
        .ok_or_else(|| anyhow::anyhow!("Simulation duration exceeds the prototype limit"))?;
    let payload=serde_json::json!({"factory":factory,"inputs":factory.input_requirements(),"blueprint_string":factory.document.encode()?,"options":options}).to_string();
    let mut equals = String::from("=");
    while payload.contains(&format!("]{equals}]")) {
        equals.push('=');
    }
    let script = format!(
        "local cfg = helpers.json_to_table([{equals}[{payload}]{equals}])\n{}",
        include_str!("simulate.lua")
    );
    let mut report: SimulationReport =
        serde_json::from_value(run_test_scenario(game, &script, ticks)?)?;
    if report.game_version != factory.game_version || report.mods != factory.mods {
        report.passed = false;
        report.errors.push(
            "Game/catalog version or enabled mod set differs from the generated factory".into(),
        );
    }
    Ok(report)
}
