use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use factorium::{
    ExtractOptions, FactoryRequest, GameData, GeneratedFactory, SimulationOptions,
    discover_factorio, discover_mod_directory, extract_game_data, generate_blueprint,
    verify_in_game,
};

#[derive(Parser)]
#[command(
    version,
    about = "Generate Factorio production blueprints from game prototypes"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Open the local web workbench for generation, inspection and game checks.
    Serve {
        #[arg(long, default_value = "data/game/catalog.json")]
        catalog: PathBuf,
        #[arg(long, default_value = "3410")]
        port: u16,
        #[command(flatten)]
        game: GameArgs,
    },
    /// Generate a single-input/single-output solid recipe line.
    Generate {
        #[arg(long)]
        product: String,
        #[arg(long)]
        rate: f64,
        #[arg(long)]
        recipe: Option<String>,
        #[arg(long, default_value = "assembling-machine-2")]
        machine: String,
        #[arg(long, default_value = "transport-belt")]
        belt: String,
        #[arg(long, default_value = "fast-inserter")]
        inserter: String,
        #[arg(long, default_value = "small-electric-pole")]
        pole: String,
        #[arg(long, default_value = "data/game/catalog.json")]
        catalog: PathBuf,
        #[arg(short, long, default_value = "data/generated")]
        output: PathBuf,
        /// Import and measure the encoded blueprint in a temporary Factorio scenario.
        #[arg(long)]
        verify: bool,
        #[command(flatten)]
        game: GameArgs,
        #[command(flatten)]
        simulation: SimulationArgs,
    },
    /// Independently check and measure a previously generated factory.
    Verify {
        #[arg(default_value = "data/generated/factory.json")]
        factory: PathBuf,
        #[arg(long, default_value = "data/game/catalog.json")]
        catalog: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[command(flatten)]
        game: GameArgs,
        #[command(flatten)]
        simulation: SimulationArgs,
    },
    /// Export all prototypes using an installed Factorio 2.1.24+.
    Extract {
        /// Path to factorio.exe / factorio; otherwise search PATH and common installs.
        #[arg(long, env = "FACTORIO_BIN")]
        factorio: Option<PathBuf>,
        /// Snapshot this mod directory, including mod-list.json and mod-settings.dat.
        #[arg(long, env = "FACTORIO_MOD_DIRECTORY", conflicts_with = "builtin_only")]
        mods: Option<PathBuf>,
        /// Ignore user mods and use the game's default set of built-in mods/DLC.
        #[arg(long)]
        builtin_only: bool,
        /// Game data directory for a nonstandard installation.
        #[arg(long)]
        data_dir: Option<PathBuf>,
        #[arg(short, long, default_value = "data/game")]
        output: PathBuf,
        /// Maximum seconds for each game process.
        #[arg(long, default_value = "300", value_parser = clap::value_parser!(u64).range(1..))]
        timeout: u64,
    },
    /// Show counts from an existing catalog without starting Factorio.
    Inspect {
        #[arg(default_value = "data/game/catalog.json")]
        catalog: PathBuf,
    },
}

#[derive(Args)]
struct GameArgs {
    #[arg(long, env = "FACTORIO_BIN")]
    factorio: Option<PathBuf>,
    #[arg(long, env = "FACTORIO_MOD_DIRECTORY", conflicts_with = "builtin_only")]
    mods: Option<PathBuf>,
    #[arg(long)]
    builtin_only: bool,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long,default_value="300",value_parser=clap::value_parser!(u64).range(1..))]
    timeout: u64,
}

impl GameArgs {
    fn options(self) -> Result<ExtractOptions> {
        let binary = self.factorio.map(Ok).unwrap_or_else(discover_factorio)?;
        let mods = if self.builtin_only {
            None
        } else {
            self.mods.or_else(|| discover_mod_directory(&binary))
        };
        let mut options = ExtractOptions::new(binary);
        options.mod_directory = mods;
        options.data_directory = self.data_dir;
        options.timeout = Duration::from_secs(self.timeout);
        Ok(options)
    }
}

#[derive(Args)]
struct SimulationArgs {
    /// Game seconds to run before measuring boundary output.
    #[arg(long,default_value="120",value_parser=clap::value_parser!(u32).range(0..=30000))]
    warmup: u32,
    /// Game seconds to measure (four windows).
    #[arg(long,default_value="120",value_parser=clap::value_parser!(u32).range(4..=30000))]
    measure: u32,
}

impl SimulationArgs {
    fn options(self) -> SimulationOptions {
        SimulationOptions {
            warmup_ticks: self.warmup * 60,
            measurement_ticks: self.measure * 60,
        }
    }
}

fn read_catalog(path: &std::path::Path) -> Result<GameData> {
    GameData::from_reader(
        File::open(path).with_context(|| format!("Cannot open {}", path.display()))?,
    )
}

fn verify_and_save(
    data: &GameData,
    factory: &GeneratedFactory,
    game: GameArgs,
    simulation: SimulationArgs,
    path: &std::path::Path,
) -> Result<()> {
    eprintln!("Importing blueprint and measuring it in Factorio...");
    let report = verify_in_game(data, factory, &game.options()?, &simulation.options())?;
    let mut writer = BufWriter::new(File::create(path)?);
    serde_json::to_writer_pretty(&mut writer, &report)?;
    writer.flush()?;
    println!(
        "Measured: {:.3}/s | target: {:.3}/s | electric networks: {} | unpowered consumers: {}",
        report.measured_output_per_second,
        factory.request.rate,
        report.network_count,
        report.unpowered_entities.len()
    );
    anyhow::ensure!(
        report.passed,
        "Game verification failed: {} (report: {})",
        report.errors.join("; "),
        path.display()
    );
    println!("Game verification passed. Report: {}", path.display());
    Ok(())
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Serve {
            catalog,
            port,
            game,
        } => {
            let data = read_catalog(&catalog)?;
            let game = match game.options() {
                Ok(options) => Some(options),
                Err(error) => {
                    eprintln!("Game verification unavailable: {error}");
                    None
                }
            };
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()?
                .block_on(factorium::web::serve(
                    data,
                    factorium::web::WebOptions { port, game },
                ))?;
        }
        Commands::Generate {
            product,
            rate,
            recipe,
            machine,
            belt,
            inserter,
            pole,
            catalog,
            output,
            verify,
            game,
            simulation,
        } => {
            let data = read_catalog(&catalog)?;
            let request = FactoryRequest {
                product,
                rate,
                recipe,
                machine,
                belt,
                inserter,
                pole,
            };
            let factory = generate_blueprint(&data, &request)?;
            factory.write_to(&output)?;
            println!(
                "{} >= {:.3}/s | machines: {} | rows: {}",
                factory.request.product,
                factory.request.rate,
                factory.machine_count,
                factory.rows.len()
            );
            println!(
                "Input {}: target {:.3}/s, supply {:.3}/s including headroom",
                factory.input_item, factory.input_per_second, factory.supplied_input_per_second
            );
            println!(
                "Power: {} x {} | supply radius: {} | wire reach: {} | installed load <= {:.0} W",
                factory.power.pole_count,
                factory.power.pole,
                factory.power.supply_area_distance,
                factory.power.max_wire_distance,
                factory.power.installed_power_watts
            );
            println!(
                "Connect electricity to pole #{} at ({}, {})",
                factory.power.connection_entity,
                factory.power.connection_position.x,
                factory.power.connection_position.y
            );
            println!("Blueprint: {}", output.join("blueprint.txt").display());
            if verify {
                verify_and_save(
                    &data,
                    &factory,
                    game,
                    simulation,
                    &output.join("verification.json"),
                )?;
            } else {
                println!(
                    "Static checks passed; throughput is modelled. Use --verify for game measurement."
                );
            }
        }
        Commands::Verify {
            factory,
            catalog,
            output,
            game,
            simulation,
        } => {
            let data = read_catalog(&catalog)?;
            let value: GeneratedFactory =
                serde_json::from_reader(BufReader::new(File::open(&factory)?))?;
            let output = output.unwrap_or_else(|| factory.with_file_name("verification.json"));
            verify_and_save(&data, &value, game, simulation, &output)?;
        }
        Commands::Extract {
            factorio,
            mods,
            builtin_only,
            data_dir,
            output,
            timeout,
        } => {
            let binary = factorio.map(Ok).unwrap_or_else(discover_factorio)?;
            let mods = if builtin_only {
                None
            } else {
                mods.or_else(|| discover_mod_directory(&binary))
            };
            eprintln!("Factorio: {}", binary.display());
            eprintln!(
                "Mods: {}",
                mods.as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "built-in defaults".into())
            );
            eprintln!("Exporting raw and runtime prototypes (two game starts)...");
            let mut options = ExtractOptions::new(binary);
            options.mod_directory = mods;
            options.data_directory = data_dir;
            options.timeout = Duration::from_secs(timeout);
            let data = extract_game_data(&options, &output)?;
            print_summary(&data);
            println!("Output: {}", output.canonicalize()?.display());
        }
        Commands::Inspect { catalog } => {
            let file = File::open(&catalog)
                .with_context(|| format!("Cannot open {}", catalog.display()))?;
            print_summary(&GameData::from_reader(file)?);
        }
    }
    Ok(())
}

fn print_summary(data: &GameData) {
    println!(
        "Factorio {} | catalog schema {}",
        data.game_version, data.schema_version
    );
    println!(
        "Recipes: {} | Items: {} | Entities: {} | Fluids: {}",
        data.recipes.len(),
        data.items.len(),
        data.entities.len(),
        data.fluids.len()
    );
    println!(
        "Technologies: {} | Tiles: {} | Qualities: {} | Space locations: {}",
        data.technologies.len(),
        data.tiles.len(),
        data.qualities.len(),
        data.space_locations.len()
    );
    println!(
        "Mods: {}",
        data.mods
            .iter()
            .map(|(name, version)| format!("{name}@{version}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
}
