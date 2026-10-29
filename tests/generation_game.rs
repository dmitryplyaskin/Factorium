use anyhow::Result;
use factorium::{
    ExtractOptions, FactoryRequest, SimulationOptions, discover_factorio, discover_mod_directory,
    extract_game_data, generate_blueprint, verify_in_game,
};

#[test]
#[ignore = "requires Factorio 2.1.24+; cargo test --test generation_game -- --ignored --nocapture"]
fn measures_blueprint_import_throughput_and_power_in_the_real_game() -> Result<()> {
    let binary = discover_factorio()?;
    let mut game = ExtractOptions::new(&binary);
    game.mod_directory = discover_mod_directory(&binary);
    let export = tempfile::tempdir()?;
    let data = extract_game_data(&game, export.path())?;
    let mut cases = Vec::new();
    for pole in [
        "small-electric-pole",
        "medium-electric-pole",
        "big-electric-pole",
        "substation",
    ] {
        let mut r = FactoryRequest::new("iron-gear-wheel", 5.0);
        r.pole = pole.into();
        cases.push(r);
    }
    let mut r = FactoryRequest::new("iron-gear-wheel", 20.0);
    r.pole = "medium-electric-pole".into();
    cases.push(r);
    let mut r = FactoryRequest::new("iron-plate", 2.5);
    r.machine = "electric-furnace".into();
    r.pole = "big-electric-pole".into();
    cases.push(r);
    let mut r = FactoryRequest::new("copper-cable", 6.0);
    r.belt = "fast-transport-belt".into();
    r.inserter = "bulk-inserter".into();
    r.pole = "substation".into();
    cases.push(r);
    let mut r = FactoryRequest::new("iron-gear-wheel", 1.0);
    r.inserter = "inserter".into();
    cases.push(r);
    if data.entities.contains_key("electromagnetic-plant") {
        let mut r = FactoryRequest::new("copper-cable", 6.0);
        r.machine = "electromagnetic-plant".into();
        r.pole = "substation".into();
        cases.push(r);
    }
    for request in cases {
        eprintln!(
            "{} {}/s, {}, {}, {}",
            request.product, request.rate, request.machine, request.inserter, request.pole
        );
        let factory = generate_blueprint(&data, &request)?;
        let report = verify_in_game(&data, &factory, &game, &SimulationOptions::default())?;
        eprintln!(
            "{} entities, {} poles, {} rows, measured {:.3}/s",
            report.entity_count,
            report.pole_count,
            factory.rows.len(),
            report.measured_output_per_second
        );
        assert!(report.passed, "{report:#?}");
        assert_eq!(report.network_count, 1);
        assert!(report.unpowered_entities.is_empty());
        assert!(report.measured_output_per_second >= request.rate);
    }
    let mut drifted = generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", 1.0))?;
    drifted.machine_crafts_per_second *= 2.0;
    let rejected = verify_in_game(&data, &drifted, &game, &SimulationOptions::default())?;
    assert!(!rejected.passed);
    assert!(
        rejected
            .errors
            .iter()
            .any(|e| e.contains("Crafting speed differs"))
    );
    Ok(())
}
