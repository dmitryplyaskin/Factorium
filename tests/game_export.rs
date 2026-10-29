//! Opt-in integration test: requires a real licensed Factorio installation.

use std::{fs, path::Path};

use anyhow::Result;
use factorium::{ExtractOptions, GameData, discover_factorio, extract_game_data};
use serde_json::Value;

#[test]
#[ignore = "requires Factorio 2.1.24+; run cargo test --test game_export -- --ignored"]
fn exports_modded_prototypes_and_preserves_original_profile() -> Result<()> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mod");
    let mods = tempfile::tempdir()?;
    let output = tempfile::tempdir()?;
    let folder = mods.path().join("factorium-fixture_1.0.0");
    fs::create_dir(&folder)?;
    for name in ["info.json", "settings.lua", "data.lua"] {
        fs::copy(fixture.join(name), folder.join(name))?;
    }
    let mod_list =
        br#"{"mods":[{"name":"base","enabled":true},{"name":"factorium-fixture","enabled":true}]}"#;
    fs::write(mods.path().join("mod-list.json"), mod_list)?;
    let mut options = ExtractOptions::new(discover_factorio()?);
    options.mod_directory = Some(mods.path().to_owned());
    let data = extract_game_data(&options, output.path())?;
    assert_eq!(data.mods["factorium-fixture"], "1.0.0");
    let entity = &data.entities["factorium-fixture-machine"];
    assert_eq!((entity.tile_width, entity.tile_height), (5, 7));
    assert_eq!(entity.quality_stats["normal"]["crafting_speed"], 0.5);
    let item = &data.items["factorium-fixture-item"];
    assert_eq!(item.prototype.prototype_type, "item-with-tags");
    assert_eq!(
        item.place_result.as_deref(),
        Some("factorium-fixture-machine")
    );
    let recipe = &data.recipes["factorium-fixture-recipe"];
    assert!(recipe.prototype.hidden);
    assert!(!recipe.enabled);
    assert_eq!(recipe.energy, 2.5);
    assert_eq!(recipe.ingredients[0].properties["minimum_temperature"], 25);
    assert_eq!(recipe.products[0].independent_probability, 0.25);
    assert_eq!(recipe.products[0].shared_probability.min, 0.0);
    assert_eq!(recipe.products[0].shared_probability.max, 1.0);
    assert_eq!(recipe.products[0].amount_min, Some(1.0));
    assert_eq!(recipe.products[0].amount_max, Some(3.0));
    assert_eq!(fs::read(mods.path().join("mod-list.json"))?, mod_list);
    assert_eq!(
        fs::read(folder.join("data.lua"))?,
        fs::read(fixture.join("data.lua"))?
    );
    let raw: Value = serde_json::from_slice(&fs::read(output.path().join("data-raw-dump.json"))?)?;
    assert_eq!(
        raw["assembling-machine"]["factorium-fixture-machine"]["tile_width"],
        5
    );
    assert_eq!(raw["recipe"].as_object().unwrap().len(), data.recipes.len());
    let saved = GameData::from_reader(fs::File::open(output.path().join("catalog.json"))?)?;
    assert_eq!(saved.entities.len(), data.entities.len());
    Ok(())
}
