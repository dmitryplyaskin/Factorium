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
    cases.push(FactoryRequest::new("iron-gear-wheel", 10.0));
    cases.push(FactoryRequest::new("iron-gear-wheel", 12.0));
    cases.push(FactoryRequest::new("iron-gear-wheel", 0.5));
    // Regression for the visible 5 plates/s example: small poles should fit
    // inside both inserter lines without detached poles above or at the ends.
    let mut r = FactoryRequest::new("iron-plate", 5.0);
    r.machine = "electric-furnace".into();
    r.belt = "fast-transport-belt".into();
    r.common_input = true;
    r.reverse_output = true;
    r.input_markers = true;
    cases.push(r);
    for pole in [
        "small-electric-pole",
        "medium-electric-pole",
        "big-electric-pole",
        "substation",
    ] {
        let mut r = FactoryRequest::new("electronic-circuit", 1.0);
        r.pole = pole.into();
        cases.push(r);
    }
    cases.push(FactoryRequest::new("electronic-circuit", 3.0));
    cases.push(FactoryRequest::new("transport-belt", 3.0));
    cases.push(FactoryRequest::new("automation-science-pack", 1.0));
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
        assert_eq!(report.row_outputs.len(), factory.rows.len());
        assert_eq!(report.input_deliveries.len(), factory.input_ports().count());
        for measured in &report.input_deliveries {
            assert!(
                measured.inserted_items > 0,
                "every ingredient must be supplied"
            );
            assert!(
                factory
                    .input_requirements()
                    .iter()
                    .any(|r| r.item == measured.item)
            );
            assert!(
                measured.measured_per_second + 60.0 / f64::from(report.measurement_ticks) + 1e-8
                    >= measured.supplied_per_second / 1.05
            );
        }
        assert_eq!(
            report
                .row_outputs
                .iter()
                .map(|r| r.delivered_items)
                .sum::<u64>(),
            report.delivered_items
        );
        for (row, measured) in factory.rows.iter().zip(&report.row_outputs) {
            assert_eq!(measured.output_entity, row.output.entity_number);
            assert_eq!(measured.lane, row.output.lanes);
            assert!(
                measured.delivered_items > 0,
                "every row/lane must contribute"
            );
            assert!(
                measured.measured_per_second + 60.0 / f64::from(report.measurement_ticks) + 1e-8
                    >= measured.target_per_second
            );
        }
        if request.product == "iron-gear-wheel" && matches!(request.rate, 10.0 | 12.0) {
            assert_eq!(factory.output_ports().len(), 1);
            assert_eq!(factory.output_ports()[0].lanes, "both");
            assert!(report.measured_output_per_second > factory.belt_items_per_second / 2.0);
        }
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

#[test]
#[ignore = "requires installed Factorio; cargo test --test generation_game optional_infrastructure -- --ignored --nocapture"]
fn optional_infrastructure_transports_items_and_stores_real_output() -> Result<()> {
    let binary = discover_factorio()?;
    let mut game = ExtractOptions::new(&binary);
    game.mod_directory = discover_mod_directory(&binary);
    let export = tempfile::tempdir()?;
    let data = extract_game_data(&game, export.path())?;
    let mut cases = Vec::new();
    let mut r = FactoryRequest::new("iron-gear-wheel", 12.0);
    r.output_chest = Some("steel-chest".into());
    cases.push(r);
    let mut r = FactoryRequest::new("iron-gear-wheel", 5.0);
    r.common_input = true;
    r.input_markers = true;
    r.max_machines_per_row = Some(3);
    cases.push(r);
    for rate in [1.0, 5.0] {
        let mut r = FactoryRequest::new("electronic-circuit", rate);
        r.pole = "substation".into();
        r.common_input = true;
        r.input_markers = true;
        r.output_chest = Some("steel-chest".into());
        r.max_machines_per_row = Some(4);
        r.max_row_width = Some(20);
        cases.push(r);
    }
    let mut r = FactoryRequest::new("iron-plate", 1.0);
    r.machine = "electric-furnace".into();
    r.output_chest = Some("iron-chest".into());
    r.inserter = "inserter".into();
    r.belt = "fast-transport-belt".into();
    r.common_input = true;
    r.input_markers = true;
    r.max_row_width = Some(10);
    cases.push(r);
    let reversed: Vec<_> = cases
        .iter()
        .cloned()
        .map(|mut r| {
            r.reverse_output = true;
            r
        })
        .collect();
    cases.extend(reversed);
    for r in cases {
        eprintln!(
            "Options case: {} {}/s, bus={}, reverse={}, storage={:?}",
            r.product, r.rate, r.common_input, r.reverse_output, r.output_chest
        );
        let factory = generate_blueprint(&data, &r)?;
        // Returning output across a large bus adds conveyor travel time before
        // chest deliveries settle; measure the steady state after that delay.
        let options = SimulationOptions {
            warmup_ticks: if r.reverse_output && r.common_input {
                360 * 60
            } else {
                120 * 60
            },
            ..SimulationOptions::default()
        };
        let report = verify_in_game(&data, &factory, &game, &options)?;
        eprintln!(
            "{} rows, {} external inputs, measured {:.3}/s",
            factory.rows.len(),
            factory.external_inputs().len(),
            report.measured_output_per_second
        );
        assert!(report.passed, "{report:#?}");
        assert!(report.unpowered_entities.is_empty());
        assert_eq!(report.network_count, 1);
        assert!(report.row_outputs.iter().all(|row| row.delivered_items > 0));
        if r.common_input {
            assert_eq!(
                report.external_input_deliveries.len(),
                factory.external_inputs().len()
            );
        }
        if r.output_chest.is_some() {
            assert_eq!(report.output_measurement_location, "chests");
            assert_eq!(
                report
                    .storage_deliveries
                    .iter()
                    .map(|c| c.added_items)
                    .sum::<u64>(),
                report.delivered_items
            );
            assert!(
                report
                    .storage_deliveries
                    .iter()
                    .all(|c| c.stored_items >= c.added_items)
            );
        }
    }
    // Storage is finite: the harness must neither clear chests nor count
    // items waiting on a blocked belt as delivered output.
    let mut r = FactoryRequest::new("iron-gear-wheel", 0.5);
    r.output_chest = Some("wooden-chest".into());
    let factory = generate_blueprint(&data, &r)?;
    let full = verify_in_game(
        &data,
        &factory,
        &game,
        &SimulationOptions {
            warmup_ticks: 4000 * 60,
            measurement_ticks: 120 * 60,
        },
    )?;
    assert!(!full.passed);
    assert_eq!(full.delivered_items, 0);
    assert!(
        full.storage_deliveries
            .iter()
            .all(|c| c.stored_items > 0 && c.added_items == 0)
    );
    Ok(())
}
