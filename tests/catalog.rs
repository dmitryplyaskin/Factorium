use anyhow::Result;
use factorium::GameData;
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("fixtures/catalog.json");

#[test]
fn preserves_modded_data_probabilities_temperatures_and_exact_footprints() -> Result<()> {
    let data = GameData::from_reader(FIXTURE.as_bytes())?;
    let recipe = &data.recipes["test-recipe"];
    assert!(recipe.prototype.hidden);
    assert!(!recipe.enabled);
    assert_eq!(recipe.ingredients[0].properties["minimum_temperature"], 25);
    assert_eq!(recipe.products[0].amount, None);
    assert_eq!(recipe.products[0].amount_min, Some(1.0));
    assert_eq!(recipe.products[0].amount_max, Some(3.0));
    assert_eq!(recipe.products[0].independent_probability, 0.25);
    assert_eq!(recipe.products[0].shared_probability.min, 0.0);
    assert_eq!(recipe.products[0].shared_probability.max, 1.0);
    assert_eq!(recipe.products[0].properties["ignored_by_productivity"], 1);
    assert_eq!(
        data.items["test-item"].prototype.prototype_type,
        "item-with-tags"
    );
    assert_eq!(
        data.items["test-item"].place_result.as_deref(),
        Some("test-machine")
    );
    let entity = &data.entities["test-machine"];
    assert_eq!((entity.tile_width, entity.tile_height), (5, 7));
    assert_eq!(entity.quality_stats["legendary"]["crafting_speed"], 1.25);
    assert!(entity.crafting_categories.is_empty());
    assert!(entity.fluid_boxes.is_empty());
    assert!(data.technologies.is_empty());
    let encoded = serde_json::to_vec(&data)?;
    let roundtrip = GameData::from_reader(encoded.as_slice())?;
    assert_eq!(
        roundtrip.entities["test-machine"].prototype.properties["unknown_modded_property"],
        json!({"preserve": true})
    );
    Ok(())
}

#[test]
fn rejects_unknown_schema_and_malformed_collections() -> Result<()> {
    let original: Value = serde_json::from_str(FIXTURE)?;
    for (key, invalid) in [
        ("schema_version", json!(999)),
        ("technologies", json!([1])),
        ("game_version", json!("2.0.0")),
        ("recipes", json!({})),
    ] {
        let mut document = original.clone();
        document[key] = invalid;
        assert!(
            GameData::from_reader(serde_json::to_vec(&document)?.as_slice()).is_err(),
            "accepted invalid {key}"
        );
    }
    let mut document = original;
    document["entities"]["test-machine"]["fluid_boxes"] = json!({"unexpected": 1});
    assert!(GameData::from_reader(serde_json::to_vec(&document)?.as_slice()).is_err());
    Ok(())
}
