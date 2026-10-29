mod common;

use std::io::Read;

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use factorium::{FactoryRequest, generate_blueprint, validate_factory};
use flate2::read::ZlibDecoder;
use serde_json::json;

#[test]
fn produces_a_deterministic_importable_envelope_and_balanced_port_rates() -> Result<()> {
    let data = common::catalog();
    let request = FactoryRequest::new("iron-gear-wheel", 5.0);
    let f = generate_blueprint(&data, &request)?;
    assert_eq!(f.input_per_second, 10.0);
    assert_eq!(f.supplied_input_per_second, 10.5);
    assert!(f.power.installed_power_watts > 0.0);
    let string = f.document.encode()?;
    assert!(string.starts_with('0'));
    let compressed = STANDARD.decode(&string[1..])?;
    let mut decoded = Vec::new();
    ZlibDecoder::new(compressed.as_slice()).read_to_end(&mut decoded)?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&decoded)?,
        serde_json::to_value(&f.document)?
    );
    assert_eq!(
        string,
        generate_blueprint(&data, &request)?.document.encode()?
    );
    Ok(())
}

#[test]
fn separates_supply_coverage_from_wire_reach_for_every_standard_pole() -> Result<()> {
    let data = common::catalog();
    let mut counts = Vec::new();
    for pole in [
        "small-electric-pole",
        "medium-electric-pole",
        "big-electric-pole",
        "substation",
    ] {
        let mut request = FactoryRequest::new("iron-gear-wheel", 5.0);
        request.pole = pole.into();
        let f = generate_blueprint(&data, &request)?;
        validate_factory(&data, &f)?;
        assert_eq!(
            f.power.max_wire_distance,
            data.entities[pole].quality_stats["normal"]["max_wire_distance"]
                .as_f64()
                .unwrap()
        );
        assert_eq!(f.document.blueprint.wires.len(), f.power.pole_count - 1);
        counts.push(f.power.pole_count);
    }
    assert!(
        counts[3] < counts[0],
        "a larger supply area should cover this row with fewer poles"
    );
    Ok(())
}

#[test]
fn splits_large_flows_into_independent_belt_pairs_and_supports_even_footprints() -> Result<()> {
    let data = common::catalog();
    let mut request = FactoryRequest::new("iron-gear-wheel", 20.0);
    request.machine = "wide-machine".into();
    request.pole = "big-electric-pole".into();
    let f = generate_blueprint(&data, &request)?;
    assert!(f.rows.len() > 1);
    assert_eq!(f.entity_sizes["wide-machine"], [4, 6]);
    for row in &f.rows {
        assert!(row.input.rate <= 15.0);
        assert!(row.output.rate <= 7.5);
    }
    validate_factory(&data, &f)?;
    Ok(())
}

#[test]
fn independent_checker_rejects_disconnected_wires_unpowered_consumers_and_broken_belts()
-> Result<()> {
    let data = common::catalog();
    let f = generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", 5.0))?;
    let mut bad = f.clone();
    bad.document.blueprint.wires.pop();
    assert!(validate_factory(&data, &bad).is_err());
    let mut bad = f.clone();
    let pole = bad
        .document
        .blueprint
        .entities
        .iter_mut()
        .find(|e| e.name == bad.request.pole)
        .unwrap();
    pole.position.x += 100.0;
    assert!(validate_factory(&data, &bad).is_err());
    let mut bad = f.clone();
    bad.document
        .blueprint
        .entities
        .iter_mut()
        .find(|e| e.name == bad.request.inserter)
        .unwrap()
        .direction = 8;
    assert!(validate_factory(&data, &bad).is_err());
    let mut bad = f.clone();
    let id = bad
        .document
        .blueprint
        .entities
        .iter()
        .find(|e| e.name == bad.request.belt && e.position.x > 3.0)
        .unwrap()
        .entity_number;
    bad.document
        .blueprint
        .entities
        .retain(|e| e.entity_number != id);
    assert!(validate_factory(&data, &bad).is_err());
    let mut bad = f.clone();
    bad.document.blueprint.entities[1].position = bad.document.blueprint.entities[2].position;
    assert!(validate_factory(&data, &bad).is_err());
    let mut changed_catalog = common::catalog_value();
    changed_catalog["entities"]["small-electric-pole"]["quality_stats"]["normal"]["supply_area_distance"] =
        json!(0.1);
    assert!(
        validate_factory(&serde_json::from_value(changed_catalog)?, &f)
            .unwrap_err()
            .to_string()
            .contains("power coverage")
    );
    Ok(())
}

#[test]
fn handles_furnace_selection_and_rejects_unsupported_mechanics() -> Result<()> {
    let data = common::catalog();
    let mut request = FactoryRequest::new("iron-plate", 2.0);
    request.machine = "electric-furnace".into();
    let f = generate_blueprint(&data, &request)?;
    assert_eq!(f.input_item, "iron-ore");
    assert!(
        f.document
            .blueprint
            .entities
            .iter()
            .filter(|e| e.name == request.machine)
            .all(|e| e.recipe.is_none())
    );
    let mut request = FactoryRequest::new("iron-gear-wheel", 1.0);
    request.inserter = "long-handed-inserter".into();
    assert!(generate_blueprint(&data, &request).is_err());
    for rate in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e12] {
        assert!(generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", rate)).is_err());
    }
    let mut raw = common::catalog_value();
    raw["items"]["iron-plate"]["quality_stats"]["normal"]["spoil_ticks"] = json!(60);
    assert!(
        generate_blueprint(
            &serde_json::from_value(raw)?,
            &FactoryRequest::new("iron-gear-wheel", 1.0)
        )
        .is_err()
    );
    let mut raw = common::catalog_value();
    raw["recipes"]["iron-gear-wheel"]["products"][0]["independent_probability"] = json!(0.5);
    assert!(
        generate_blueprint(
            &serde_json::from_value(raw)?,
            &FactoryRequest::new("iron-gear-wheel", 1.0)
        )
        .is_err()
    );
    Ok(())
}
