//! Factorio 2.x blueprint envelope and explicit copper wire connections.

use std::io::Write;

use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::{Compression, write::ZlibEncoder};
use serde::{Deserialize, Serialize};

use crate::model::Position;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintDocument {
    pub blueprint: Blueprint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    pub item: String,
    pub label: String,
    pub description: String,
    pub version: u64,
    pub entities: Vec<BlueprintEntity>,
    /// [source entity, source connector, target entity, target connector].
    pub wires: Vec<[u32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintEntity {
    pub entity_number: u32,
    pub name: String,
    pub position: Position,
    pub direction: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_stack_size: Option<u32>,
}

impl BlueprintDocument {
    pub fn new(game_version: &str, label: String) -> Result<Self> {
        let parts = game_version
            .split('.')
            .map(str::parse::<u16>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(parts.len() == 3, "Invalid game version: {game_version}");
        Ok(Self {
            blueprint: Blueprint {
                item: "blueprint".into(),
                label,
                description: String::new(),
                version: (u64::from(parts[0]) << 48)
                    | (u64::from(parts[1]) << 32)
                    | (u64::from(parts[2]) << 16),
                entities: Vec::new(),
                wires: Vec::new(),
            },
        })
    }

    pub fn encode(&self) -> Result<String> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        serde_json::to_writer(&mut encoder, self)?;
        encoder.flush()?;
        Ok(format!(
            "0{}",
            STANDARD.encode(encoder.finish().context("Cannot compress blueprint")?)
        ))
    }

    pub(crate) fn add(&mut self, name: &str, x: f64, y: f64, direction: u8) -> u32 {
        let id = self.blueprint.entities.len() as u32 + 1;
        self.blueprint.entities.push(BlueprintEntity {
            entity_number: id,
            name: name.into(),
            position: Position { x, y },
            direction,
            recipe: None,
            override_stack_size: None,
        });
        id
    }
}
