mod common;

use std::io::Read;

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use factorium::{FactoryRequest, generate_blueprint, validate_factory};

#[test]
fn packed_three_ingredient_bus_reuses_columns_and_balances_even_small_outputs() -> Result<()> {
    let mut raw = common::catalog_value();
    let mut recipe = raw["recipes"]["advanced-circuit"].clone();
    recipe["name"] = json!("low-density-structure");
    recipe["energy"] = json!(20.0);
    recipe["ingredients"] = json!([
        {"type":"item","name":"copper-plate","amount":20},
        {"type":"item","name":"steel-plate","amount":2},
        {"type":"item","name":"plastic-bar","amount":5}
    ]);
    recipe["products"][0]["name"] = json!("low-density-structure");
    raw["recipes"]["low-density-structure"] = recipe;
    let mut item = raw["items"]["advanced-circuit"].clone();
    item["name"] = json!("low-density-structure");
    raw["items"]["low-density-structure"] = item;
    let mut machine = raw["entities"]["assembling-machine-2"].clone();
    machine["name"] = json!("assembling-machine-3");
    machine["quality_stats"]["normal"]["crafting_speed"] = json!(1.25);
    raw["entities"]["assembling-machine-3"] = machine;
    for (old, new) in [
        ("fast-transport-belt", "turbo-transport-belt"),
        ("fast-splitter", "turbo-splitter"),
        ("fast-underground-belt", "turbo-underground-belt"),
    ] {
        let mut prototype = raw["entities"][old].clone();
        prototype["name"] = json!(new);
        prototype["belt_speed"] = json!(0.125);
        prototype["max_underground_distance"] = json!(10);
        raw["entities"][new] = prototype;
    }
    let data = serde_json::from_value(raw)?;
    for reverse in [true, false] {
        let mut request = FactoryRequest::new("low-density-structure", 1.0);
        request.machine = "assembling-machine-3".into();
        request.belt = "turbo-transport-belt".into();
        request.common_input = true;
        request.reverse_output = reverse;
        request.input_markers = true;
        request.max_machines_per_row = Some(14);
        let factory = generate_blueprint(&data, &request)?;
        validate_factory(&data, &factory)?;
        assert!(factory.rows.len() >= 4);
        assert_eq!(factory.input_belt_count(), 2);
        let west = factory
            .external_inputs()
            .iter()
            .map(|p| p.position.x)
            .fold(f64::INFINITY, f64::min);
        let row_start = factory
            .input_ports()
            .map(|p| p.position.x)
            .fold(f64::INFINITY, f64::min);
        assert!(
            row_start - west <= 16.0,
            "bus extends {} tiles west of the rows",
            row_start - west
        );
        let top = factory
            .input_ports()
            .map(|p| p.position.y)
            .fold(f64::INFINITY, f64::min);
        assert_eq!(
            factory
                .external_inputs()
                .iter()
                .map(|p| p.position.y)
                .fold(f64::INFINITY, f64::min),
            top
        );
        assert_eq!(factory.output_ports().len(), 1);
        assert_eq!(factory.output_ports()[0].lanes, "both");
        let bus = factory.bus.as_ref().unwrap();
        for line in &bus.lines {
            let transport: Vec<_> = line
                .entities
                .iter()
                .map(|&id| &factory.document.blueprint.entities[(id - 1) as usize])
                .collect();
            assert!(
                transport.iter().all(|e| e.direction != 0),
                "ingredient route must not travel back north"
            );
            let columns: std::collections::BTreeSet<_> = transport
                .iter()
                .filter(|e| e.direction == 8 && e.name != bus.splitter)
                .map(|e| e.position.x.floor() as i32)
                .collect();
            assert_eq!(
                columns.len(),
                1,
                "each ingredient must use one straight vertical trunk"
            );
        }
    }
    Ok(())
}

#[test]
fn one_row_shared_bus_also_balances_the_external_output() -> Result<()> {
    let data = common::catalog();
    for reverse in [true, false] {
        let mut request = FactoryRequest::new("iron-gear-wheel", 0.5);
        request.common_input = true;
        request.reverse_output = reverse;
        let factory = generate_blueprint(&data, &request)?;
        assert_eq!(factory.rows.len(), 1);
        assert_eq!(factory.output_ports()[0].lanes, "both");
        validate_factory(&data, &factory)?;
    }
    Ok(())
}

#[test]
fn dense_headers_at_output_height_remain_separate_from_production_belts() -> Result<()> {
    let data = common::catalog();
    let mut request = FactoryRequest::new("advanced-circuit", 10.0);
    request.common_input = true;
    request.max_machines_per_row = Some(4);
    request.pole = "substation".into();
    let factory = generate_blueprint(&data, &request)?;
    assert!(factory.external_inputs().iter().any(|input| {
        factory
            .rows
            .iter()
            .any(|row| input.position.y == row.production_output.as_ref().unwrap().position.y)
    }));
    validate_factory(&data, &factory)?;
    let mut broken = factory.clone();
    let id = factory.rows[0]
        .production_output
        .as_ref()
        .unwrap()
        .entity_number;
    broken.document.blueprint.entities[(id - 1) as usize].direction = 8;
    assert!(validate_factory(&data, &broken).is_err());
    Ok(())
}

#[test]
fn compact_bus_keeps_sixty_plate_ports_inside_the_production_height() -> Result<()> {
    let mut raw = common::catalog_value();
    raw["recipes"]["iron-plate"]["energy"] = json!(3.2);
    let data = serde_json::from_value(raw)?;
    for reverse in [true, false] {
        let mut r = FactoryRequest::new("iron-plate", 60.0);
        r.machine = "electric-furnace".into();
        r.belt = "fast-transport-belt".into();
        r.common_input = true;
        r.reverse_output = reverse;
        r.input_markers = true;
        let f = generate_blueprint(&data, &r)?;
        validate_factory(&data, &f)?;
        let bus = f.bus.as_ref().unwrap();
        assert!(bus.compact);
        assert_eq!(bus.lines.len(), 3);
        let mut inputs = f.external_inputs();
        inputs.sort_by(|a, b| a.position.y.total_cmp(&b.position.y));
        assert!(
            inputs
                .windows(2)
                .all(|p| p[1].position.y > p[0].position.y && p[0].position.x == p[1].position.x)
        );
        let top = f
            .input_ports()
            .map(|p| p.position.y)
            .fold(f64::INFINITY, f64::min);
        assert!(top - inputs[0].position.y <= 3.0);
        for line in &bus.lines {
            let first = f
                .input_ports()
                .filter(|p| line.targets.contains(&p.entity_number))
                .map(|p| p.position.y)
                .fold(f64::INFINITY, f64::min);
            assert!(
                line.entities
                    .iter()
                    .map(|&id| &f.document.blueprint.entities[(id - 1) as usize])
                    .filter(|e| e.name == bus.splitter)
                    .all(|e| e.position.y >= first),
                "Branches must start at their first production row, not above the factory"
            );
        }
        let outputs = f.output_ports();
        assert_eq!(outputs.len(), 3);
        let bottom = f
            .input_ports()
            .map(|p| p.position.y)
            .chain(
                f.rows
                    .iter()
                    .map(|r| r.production_output.as_ref().unwrap_or(&r.output).position.y),
            )
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(outputs.iter().all(|p| p.position.y <= bottom));
        assert!(
            outputs
                .windows(2)
                .all(|p| p[0].position.x == p[1].position.x)
        );
        if reverse {
            assert!(f.rows[0].input.position.x - inputs[0].position.x <= 45.0);
        }
        let mut bad = f.clone();
        bad.rows[0].production_output.as_mut().unwrap().rate += 1.0;
        assert!(validate_factory(&data, &bad).is_err());
        let roundtrip = serde_json::from_value(serde_json::to_value(&f)?)?;
        validate_factory(&data, &roundtrip)?;
    }
    Ok(())
}

#[test]
fn shared_outputs_follow_flow_instead_of_row_count_and_use_both_lanes() -> Result<()> {
    let data = common::catalog();
    for reverse in [false, true] {
        for rate in [5.0, 12.0, 14.0, 20.0] {
            let mut request = FactoryRequest::new("automation-science-pack", rate);
            request.common_input = true;
            request.reverse_output = reverse;
            request.max_machines_per_row = Some(8);
            request.pole = "substation".into();
            let f = generate_blueprint(&data, &request)?;
            validate_factory(&data, &f)?;
            assert!(f.bus.as_ref().unwrap().compact);
            let outputs = f.output_ports();
            assert_eq!(
                outputs.len(),
                (f.planned_output_per_second / f.belt_items_per_second).ceil() as usize,
                "rate={rate}, reverse={reverse}"
            );
            assert!(
                (outputs.iter().map(|p| p.rate).sum::<f64>() - f.planned_output_per_second).abs()
                    < 1e-8
            );
            if rate < 15.0 {
                assert_eq!(outputs.len(), 1);
            }
            let top = f
                .input_ports()
                .map(|p| p.position.y)
                .fold(f64::INFINITY, f64::min);
            assert_eq!(
                f.external_inputs()
                    .iter()
                    .map(|p| p.position.y)
                    .fold(f64::INFINITY, f64::min),
                top
            );
            let bottom = f
                .input_ports()
                .map(|p| p.position.y)
                .chain(
                    f.rows
                        .iter()
                        .map(|r| r.production_output.as_ref().unwrap_or(&r.output).position.y),
                )
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(outputs.iter().all(|p| p.position.y <= bottom));
            if rate == 12.0 {
                assert_eq!(outputs[0].lanes, "both");
                assert!(outputs[0].rate > f.belt_items_per_second / 2.0);
                let mut broken = f.clone();
                let splitter = &f.bus.as_ref().unwrap().splitter;
                let id = *f
                    .bus
                    .as_ref()
                    .unwrap()
                    .output_entities
                    .iter()
                    .find(|&&id| f.document.blueprint.entities[(id - 1) as usize].name == *splitter)
                    .unwrap();
                broken.document.blueprint.entities[(id - 1) as usize].direction =
                    (broken.document.blueprint.entities[(id - 1) as usize].direction + 8) % 16;
                assert!(validate_factory(&data, &broken).is_err());
            }
        }
    }
    Ok(())
}

#[test]
fn small_poles_follow_matching_regular_lines_between_furnace_inserters() -> Result<()> {
    let mut raw = common::catalog_value();
    raw["recipes"]["iron-plate"]["energy"] = json!(3.2);
    let data = serde_json::from_value(raw)?;
    let mut r = FactoryRequest::new("iron-plate", 5.0);
    r.machine = "electric-furnace".into();
    r.belt = "fast-transport-belt".into();
    // Isolate the repeating production power pattern from the optional bus,
    // whose collector footprint can change the optimizer's preferred row count.
    let f = generate_blueprint(&data, &r)?;
    validate_factory(&data, &f)?;
    assert_eq!(f.machine_count, 9);
    assert_eq!(f.rows.len(), 1);
    assert_eq!(f.power.pole_count, 10);
    let entities = &f.document.blueprint.entities;
    let mut bands = std::collections::BTreeMap::<i32, Vec<f64>>::new();
    for p in entities.iter().filter(|e| e.name == r.pole) {
        for dx in [-1.0, 1.0] {
            assert!(
                entities.iter().any(|e| e.name == r.inserter
                    && e.position.y == p.position.y
                    && e.position.x == p.position.x + dx),
                "Every pole must sit between the two hands, including the row ends"
            );
        }
        bands
            .entry(p.position.y.floor() as i32)
            .or_default()
            .push(p.position.x);
    }
    assert_eq!(bands.len(), 2);
    for xs in bands.values_mut() {
        xs.sort_by(f64::total_cmp);
        assert_eq!(xs.len(), 5);
        assert!(xs.windows(2).all(|s| s[1] - s[0] == 6.0));
    }
    let mut bands = bands.values();
    assert_eq!(
        bands.next(),
        bands.next(),
        "Both sides must use matching columns"
    );
    Ok(())
}

#[test]
fn reverse_output_keeps_ports_and_storage_on_the_input_side() -> Result<()> {
    let data = common::catalog();
    for product in ["iron-gear-wheel", "electronic-circuit"] {
        for common_input in [false, true] {
            for storage in [false, true] {
                let mut r = FactoryRequest::new(
                    product,
                    if product == "electronic-circuit" {
                        1.0
                    } else {
                        5.0
                    },
                );
                r.reverse_output = true;
                r.common_input = common_input;
                r.output_chest = storage.then(|| "steel-chest".into());
                r.input_markers = true;
                r.max_machines_per_row = Some(3);
                r.max_row_width = Some(14);
                r.pole = "substation".into();
                let f = generate_blueprint(&data, &r)?;
                validate_factory(&data, &f)?;
                let boundary = f.external_inputs();
                assert!(boundary.iter().all(|p| p.direction == 4));
                for port in f.output_ports() {
                    assert_eq!(port.direction, 12);
                    assert!(boundary.iter().all(|p| p.position.x == port.position.x));
                    assert!(
                        f.storage
                            .iter()
                            .filter(|b| b.output_entity == port.entity_number)
                            .flat_map(|b| &b.chests)
                            .all(
                                |&id| f.document.blueprint.entities[(id - 1) as usize].position.x
                                    < port.position.x
                            )
                    );
                }
                let mut bad = f.clone();
                let id = bad.rows[0].output.entity_number;
                bad.document.blueprint.entities[(id - 1) as usize].direction = 4;
                assert!(validate_factory(&data, &bad).is_err());
                let mut bad = f.clone();
                bad.rows[0].output.lanes = if bad.rows[0].output.lanes == "left" {
                    "right".into()
                } else {
                    "left".into()
                };
                assert!(validate_factory(&data, &bad).is_err());
                if common_input {
                    let mut bad = f.clone();
                    let crossing = bad
                        .document
                        .blueprint
                        .entities
                        .iter_mut()
                        .find(|e| e.belt_type.as_deref() == Some("output"))
                        .expect("compact routes have underground crossings");
                    crossing.belt_type = Some("input".into());
                    assert!(validate_factory(&data, &bad).is_err());
                }
                let restored = serde_json::from_value(serde_json::to_value(&f)?)?;
                validate_factory(&data, &restored)?;
            }
        }
    }
    Ok(())
}

#[test]
fn optional_bus_storage_markers_and_both_row_limits_are_validated() -> Result<()> {
    let data = common::catalog();
    let mut r = FactoryRequest::new("electronic-circuit", 1.0);
    r.pole = "substation".into();
    r.common_input = true;
    r.output_chest = Some("steel-chest".into());
    r.input_markers = true;
    r.max_machines_per_row = Some(4);
    r.max_row_width = Some(14);
    let f = generate_blueprint(&data, &r)?;
    validate_factory(&data, &f)?;
    assert!(f.bus.as_ref().unwrap().compact);
    assert!(f.rows.iter().all(|row| {
        let port = row.production_output.as_ref().unwrap_or(&row.output);
        row.machines.len() <= 4 && port.position.x - row.input.position.x + 1.0 <= 14.0
    }));
    assert_eq!(f.external_inputs().len(), 2);
    assert_eq!(f.markers.len(), 2);
    assert_eq!(f.storage.len(), f.output_ports().len());
    for need in f.input_requirements() {
        let supplied: f64 = f
            .external_inputs()
            .iter()
            .filter(|p| p.item == need.item)
            .map(|p| p.rate)
            .sum();
        assert!((supplied - need.supplied_per_second).abs() < 1e-8);
    }
    let entities = &f.document.blueprint.entities;
    let entry = f.power.connection_position;
    let x0 = entities
        .iter()
        .map(|e| e.position.x - f64::from(f.entity_sizes[&e.name][0]) / 2.0)
        .fold(f64::INFINITY, f64::min);
    let x1 = entities
        .iter()
        .map(|e| e.position.x + f64::from(f.entity_sizes[&e.name][0]) / 2.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let y0 = entities
        .iter()
        .map(|e| e.position.y - f64::from(f.entity_sizes[&e.name][1]) / 2.0)
        .fold(f64::INFINITY, f64::min);
    let y1 = entities
        .iter()
        .map(|e| e.position.y + f64::from(f.entity_sizes[&e.name][1]) / 2.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let [w, h] = f.entity_sizes[&r.pole];
    let exit = (entry.x - x0).min(x1 - entry.x) + f64::from(w) / 2.0 + 1.0;
    let exit = exit.min((entry.y - y0).min(y1 - entry.y) + f64::from(h) / 2.0 + 1.0);
    assert!(exit <= f.power.max_wire_distance);
    let original = f.clone();
    let mut bad = f.clone();
    let id = bad.bus.as_ref().unwrap().lines[0].source.entity_number;
    bad.document.blueprint.entities[(id - 1) as usize].direction = 0;
    assert!(validate_factory(&data, &bad).is_err());
    let mut bad = original.clone();
    let id = bad.storage[0].inserters[0];
    bad.document.blueprint.entities[(id - 1) as usize].direction ^= 8;
    assert!(validate_factory(&data, &bad).is_err());
    let mut bad = original.clone();
    let id = bad.markers[0].entity_number;
    bad.document.blueprint.entities[(id - 1) as usize].control_behavior = None;
    assert!(validate_factory(&data, &bad).is_err());
    let mut bad = original;
    bad.request.max_machines_per_row = Some(1);
    assert!(validate_factory(&data, &bad).is_err());
    for limit in [0, 65] {
        r.max_machines_per_row = Some(limit);
        assert!(generate_blueprint(&data, &r).is_err());
    }
    r.max_machines_per_row = None;
    r.max_row_width = Some(6);
    assert!(generate_blueprint(&data, &r).is_err());
    Ok(())
}

#[test]
fn large_bus_flows_keep_ingredient_partitions_and_crossings_intact() -> Result<()> {
    let data = common::catalog();
    let mut r = FactoryRequest::new("electronic-circuit", 5.0);
    r.pole = "substation".into();
    r.common_input = true;
    r.max_machines_per_row = Some(4);
    let f = generate_blueprint(&data, &r)?;
    let bus = f.bus.as_ref().unwrap();
    assert!(bus.compact);
    assert!(bus.lines.len() >= 4);
    assert!(
        bus.lines
            .iter()
            .all(|l| l.source.rate <= f.belt_items_per_second * 0.85 + 1e-8)
    );
    for pair in bus.lines.chunks_exact(2) {
        let rows = |targets: &Vec<u32>| {
            f.rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r.input_ports().any(|p| targets.contains(&p.entity_number)))
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(&pair[0].targets), rows(&pair[1].targets));
    }
    let mut bad = f;
    let underground = bad
        .document
        .blueprint
        .entities
        .iter_mut()
        .find(|e| e.belt_type.as_deref() == Some("output"))
        .unwrap();
    underground.belt_type = Some("input".into());
    assert!(validate_factory(&data, &bad).is_err());
    Ok(())
}

#[test]
fn old_requests_default_to_direct_ports_without_optional_infrastructure() -> Result<()> {
    let mut json = serde_json::to_value(FactoryRequest::new("iron-gear-wheel", 1.0))?;
    for key in [
        "common_input",
        "reverse_output",
        "output_chest",
        "input_markers",
        "max_machines_per_row",
        "max_row_width",
    ] {
        json.as_object_mut().unwrap().remove(key);
    }
    let request: FactoryRequest = serde_json::from_value(json)?;
    assert!(!request.reverse_output);
    let f = generate_blueprint(&common::catalog(), &request)?;
    assert!(f.bus.is_none() && f.storage.is_empty() && f.markers.is_empty());
    Ok(())
}
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
fn mirrored_rows_share_one_output_with_independent_lane_budgets() -> Result<()> {
    let data = common::catalog();
    let f = generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", 10.0))?;
    assert_eq!(f.rows.len(), 2);
    assert_eq!(f.output_ports().len(), 1);
    let port = &f.output_ports()[0];
    assert_eq!(port.lanes, "both");
    assert!(port.rate > f.belt_items_per_second / 2.0);
    assert!(port.rate <= f.belt_items_per_second);
    assert_eq!(
        f.rows[0].output.entity_number,
        f.rows[1].output.entity_number
    );
    assert_ne!(f.rows[0].output.lanes, f.rows[1].output.lanes);
    assert_ne!(f.rows[0].machines.len(), f.rows[1].machines.len());
    assert!(
        f.document
            .blueprint
            .entities
            .iter()
            .any(|e| e.name == f.request.inserter && e.direction == 8)
    );
    validate_factory(&data, &f)?;

    let near_capacity = generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", 12.0))?;
    assert_eq!(near_capacity.output_ports().len(), 1);
    assert_eq!(near_capacity.output_ports()[0].lanes, "both");
    assert_eq!(near_capacity.rows.len(), 2);
    assert!(near_capacity.rows.iter().all(|row| row.machines.len() > 8));
    validate_factory(&data, &near_capacity)?;

    let mut wrong = f.clone();
    wrong.rows[1].output.lanes = wrong.rows[0].output.lanes.clone();
    assert!(
        validate_factory(&data, &wrong)
            .unwrap_err()
            .to_string()
            .contains("Output lane")
    );
    let mut wrong = f.clone();
    for row in &mut wrong.rows {
        row.output.lanes = if row.output.lanes == "left" {
            "right"
        } else {
            "left"
        }
        .into();
    }
    assert!(
        validate_factory(&data, &wrong)
            .unwrap_err()
            .to_string()
            .contains("physical drop side")
    );

    let mut wrong = generate_blueprint(&data, &FactoryRequest::new("copper-cable", 10.0))?;
    assert_eq!(wrong.output_ports().len(), 1);
    // Combined output still fits the belt, but one individual lane is overloaded.
    wrong.rows[0].output.rate = 8.0;
    wrong.rows[0].input.rate = 4.0;
    assert!(
        validate_factory(&data, &wrong)
            .unwrap_err()
            .to_string()
            .contains("output-lane capacity")
    );

    let single = generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", 0.5))?;
    assert_eq!(single.rows.len(), 1);
    assert_eq!(single.output_ports()[0].lanes, "right");
    validate_factory(&data, &single)?;
    Ok(())
}

#[test]
fn two_ingredients_have_independent_belts_inserters_and_balanced_flows() -> Result<()> {
    let data = common::catalog();
    let f = generate_blueprint(&data, &FactoryRequest::new("electronic-circuit", 3.0))?;
    assert_eq!(f.input_requirements().len(), 2);
    assert_eq!(f.input_ports().count(), 2 * f.rows.len());
    for (i, (item, amount)) in [("iron-plate", 1.0), ("copper-cable", 3.0)]
        .into_iter()
        .enumerate()
    {
        let requirement = &f.inputs[i];
        assert_eq!(requirement.item, item);
        assert_eq!(requirement.per_second, 3.0 * amount);
        assert!((requirement.supplied_per_second - 3.15 * amount).abs() < 1e-8);
        let supplied: f64 = f
            .rows
            .iter()
            .flat_map(|r| r.input_ports())
            .filter(|p| p.item == item)
            .map(|p| p.rate)
            .sum();
        assert!((supplied - requirement.supplied_per_second).abs() < 1e-8);
    }
    assert_eq!(f.inputs[1].inserter, "long-handed-inserter");
    assert_eq!(f.entity_sizes["long-handed-inserter"], [1, 1]);
    assert_eq!(
        f.document
            .blueprint
            .entities
            .iter()
            .filter(|e| e.name == "long-handed-inserter")
            .count(),
        f.machine_count
    );
    validate_factory(&data, &f)?;

    let mut wrong = f.clone();
    wrong.rows[0].additional_inputs.clear();
    assert!(validate_factory(&data, &wrong).is_err());
    let mut wrong = f.clone();
    wrong.rows[0].additional_inputs[0].item = "iron-plate".into();
    assert!(validate_factory(&data, &wrong).is_err());
    let mut wrong = f.clone();
    wrong.rows[0].additional_inputs[0].entity_number = wrong.rows[0].input.entity_number;
    wrong.rows[0].additional_inputs[0].position = wrong.rows[0].input.position;
    assert!(validate_factory(&data, &wrong).is_err());
    let mut wrong = f.clone();
    wrong
        .document
        .blueprint
        .entities
        .iter_mut()
        .find(|e| e.name == "long-handed-inserter")
        .unwrap()
        .direction = 8;
    assert!(validate_factory(&data, &wrong).is_err());
    let mut bad_request = FactoryRequest::new("electronic-circuit", 1.0);
    bad_request.secondary_inserter = Some("fast-inserter".into());
    assert!(generate_blueprint(&data, &bad_request).is_err());

    let single = generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", 1.0))?;
    let mut legacy = serde_json::to_value(single)?;
    legacy.as_object_mut().unwrap().remove("inputs");
    legacy["request"]
        .as_object_mut()
        .unwrap()
        .remove("secondary_inserter");
    validate_factory(&data, &serde_json::from_value(legacy)?)?;
    let mut raw = common::catalog_value();
    raw["recipes"]["electronic-circuit"]["ingredients"][1]["type"] = json!("fluid");
    assert!(
        generate_blueprint(
            &serde_json::from_value(raw)?,
            &FactoryRequest::new("electronic-circuit", 1.0)
        )
        .is_err()
    );
    let large = generate_blueprint(&data, &FactoryRequest::new("electronic-circuit", 10.0))?;
    assert!(
        large.rows.len() > 2,
        "wire demand exceeds one pair of input belts"
    );
    for port in large.input_ports() {
        assert!(port.rate <= large.belt_items_per_second);
    }
    validate_factory(&data, &large)?;
    let mut raw = common::catalog_value();
    raw["entities"]
        .as_object_mut()
        .unwrap()
        .remove("long-handed-inserter");
    let no_long = serde_json::from_value(raw)?;
    assert!(generate_blueprint(&no_long, &FactoryRequest::new("electronic-circuit", 1.0)).is_err());
    assert!(generate_blueprint(&no_long, &FactoryRequest::new("iron-gear-wheel", 1.0)).is_ok());
    Ok(())
}

fn footprint(f: &factorium::GeneratedFactory) -> [f64; 2] {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for e in &f.document.blueprint.entities {
        let size = f.entity_sizes[&e.name];
        for (axis, position) in [e.position.x, e.position.y].into_iter().enumerate() {
            min[axis] = min[axis].min(position - f64::from(size[axis]) / 2.0);
            max[axis] = max[axis].max(position + f64::from(size[axis]) / 2.0);
        }
    }
    [max[0] - min[0], max[1] - min[1]]
}

#[test]
fn compacts_single_and_multiple_rows_including_their_power_networks() -> Result<()> {
    let data = common::catalog();
    for rate in [5.0, 20.0] {
        let f = generate_blueprint(&data, &FactoryRequest::new("iron-gear-wheel", rate))?;
        let [width, height] = footprint(&f);
        // Former template: eight machines per row, three-tile machine gaps,
        // five empty rows between seven-tile production bands.
        let old_rows = f.machine_count.div_ceil(8);
        let old_width = f.machine_count.min(8) * 3 + (f.machine_count.min(8) - 1) * 3 + 4;
        let old_height = old_rows * 7 + (old_rows - 1) * 5;
        assert!(width * height <= (old_width * old_height) as f64 * 0.7);
        validate_factory(&data, &f)?;
        assert_eq!(f.input_per_second, rate * 2.0);
        assert!((f.planned_output_per_second - rate * 1.05).abs() < 1e-8);
    }
    Ok(())
}

#[test]
fn adapts_pole_clearance_and_row_length_to_equipment_and_recipe() -> Result<()> {
    let data = common::catalog();
    let request = FactoryRequest::new("iron-gear-wheel", 5.0);
    let small = generate_blueprint(&data, &request)?;
    let mut request = request;
    request.pole = "big-electric-pole".into();
    let big = generate_blueprint(&data, &request)?;
    validate_factory(&data, &big)?;
    // A 2x2 pole with short supply reach needs actual clearance. Dense layouts
    // suitable for a 1x1 pole must not be accepted merely for their small area.
    // The extra clearance may be arranged horizontally or in additional rows.
    let [sw, sh] = footprint(&small);
    let [bw, bh] = footprint(&big);
    assert!(bw * bh > sw * sh);

    let mut request = FactoryRequest::new("steel-plate", 1.5);
    request.machine = "electric-furnace".into();
    let slow = generate_blueprint(&data, &request)?;
    assert!(slow.machine_count > 8);
    assert_eq!(slow.rows.len(), 1, "a slow recipe fits one belt pair");
    validate_factory(&data, &slow)?;

    let mut raw = common::catalog_value();
    let mut narrow = raw["entities"]["assembling-machine-2"].clone();
    narrow["name"] = json!("narrow-machine");
    narrow["tile_width"] = json!(1);
    narrow["collision_box"]["left_top"]["x"] = json!(-0.15);
    narrow["collision_box"]["right_bottom"]["x"] = json!(0.15);
    raw["entities"]["narrow-machine"] = narrow;
    let data = serde_json::from_value(raw)?;
    let mut request = FactoryRequest::new("iron-gear-wheel", 2.0);
    request.machine = "narrow-machine".into();
    let narrow = generate_blueprint(&data, &request)?;
    validate_factory(&data, &narrow)?;
    assert!(narrow.machine_count > 4);
    assert!(
        narrow.rows.len() > 1,
        "narrow rows allow power from their sides"
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

#[test]
fn three_ingredients_select_lane_packing_from_supplied_flow() -> Result<()> {
    let data = common::catalog();
    for (rate, belt_count) in [(0.5, 2), (4.0, 3)] {
        for common in [false, true] {
            for reverse in [false, true] {
                let mut request = FactoryRequest::new("advanced-circuit", rate);
                request.common_input = common;
                request.reverse_output = reverse;
                request.input_markers = true;
                request.output_chest = Some("steel-chest".into());
                let f = generate_blueprint(&data, &request)?;
                validate_factory(&data, &f)?;
                assert_eq!(f.inputs.len(), 3);
                if common {
                    assert_eq!(f.output_ports().len(), 1);
                }
                if common && belt_count == 2 {
                    assert_eq!(f.input_belt_count(), 2);
                }
                for row in &f.rows {
                    let ports: Vec<_> = row.input_ports().collect();
                    assert_eq!(ports.len(), 3);
                    assert_eq!(
                        ports
                            .iter()
                            .map(|p| p.entity_number)
                            .collect::<std::collections::BTreeSet<_>>()
                            .len(),
                        belt_count
                    );
                    if belt_count == 2 {
                        assert_eq!(ports.iter().filter(|p| p.lanes == "both").count(), 1);
                        assert_eq!(ports.iter().filter(|p| p.lanes == "left").count(), 1);
                        assert_eq!(ports.iter().filter(|p| p.lanes == "right").count(), 1);
                    }
                }
                let restored = serde_json::from_value(serde_json::to_value(&f)?)?;
                validate_factory(&data, &restored)?;
            }
        }
    }
    Ok(())
}

#[test]
fn rejects_reusing_or_overlapping_a_shared_input_lane() -> Result<()> {
    let data = common::catalog();
    let request = FactoryRequest::new("advanced-circuit", 0.5);
    let f = generate_blueprint(&data, &request)?;
    let mut broken = f.clone();
    let row = &mut broken.rows[0];
    let lane = row.input.lanes.clone();
    row.additional_inputs[1].lanes = lane;
    assert!(validate_factory(&data, &broken).is_err());
    let mut broken = f.clone();
    broken.rows[0].input.lanes = "both".into();
    assert!(validate_factory(&data, &broken).is_err());
    Ok(())
}
