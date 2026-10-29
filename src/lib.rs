//! Extract Factorio's resolved prototypes for a future blueprint generator.
//!
//! [`extract_game_data`] runs an installed Factorio 2.1+ in an isolated profile.
//! No save is needed. [`GameData`] can also be loaded without the game installed.

pub mod blueprint;
pub mod extract;
pub mod generate;
pub mod model;
pub mod simulate;
pub mod web;

pub use extract::{ExtractOptions, discover_factorio, discover_mod_directory, extract_game_data};
pub use generate::{FactoryRequest, GeneratedFactory, generate_blueprint, validate_factory};
pub use model::GameData;
pub use simulate::{SimulationOptions, SimulationReport, verify_in_game};
