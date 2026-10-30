//! Small synthetic catalog; game integration tests use freshly extracted real data.

use factorium::GameData;
use serde_json::{Value, json};

pub fn catalog_value() -> Value {
    let mut data = json!({"schema_version":1,"game_version":"2.1.24","mods":{"base":"2.1.24"},"recipes":{},"items":{},"entities":{},"fluids":{},"technologies":{},"tiles":{},"qualities":{},"space_locations":{},"surfaces":{},"surface_properties":{}});
    for (recipe, input, a, output, b, time, category) in [
        (
            "iron-gear-wheel",
            "iron-plate",
            2.0,
            "iron-gear-wheel",
            1.0,
            0.5,
            "crafting",
        ),
        (
            "copper-cable",
            "copper-plate",
            1.0,
            "copper-cable",
            2.0,
            0.5,
            "crafting",
        ),
        (
            "iron-plate",
            "iron-ore",
            1.0,
            "iron-plate",
            1.0,
            3.2,
            "smelting",
        ),
        (
            "steel-plate",
            "iron-plate",
            5.0,
            "steel-plate",
            1.0,
            16.0,
            "smelting",
        ),
    ] {
        data["recipes"][recipe] = json!({"name":recipe,"type":"recipe","hidden":false,"enabled":true,"energy":time,"categories":[category],"ingredients":[{"type":"item","name":input,"amount":a}],"products":[{"type":"item","name":output,"amount":b,"independent_probability":1.0,"shared_probability":{"min":0,"max":1}}]});
        for name in [input, output] {
            data["items"][name] = json!({"name":name,"type":"item","hidden":false,"stack_size":100,"quality_stats":{"normal":{"spoil_ticks":0}}});
        }
    }
    for (name, ingredients, time, amount) in [
        (
            "electronic-circuit",
            vec![("iron-plate", 1.0), ("copper-cable", 3.0)],
            0.5,
            1.0,
        ),
        (
            "automation-science-pack",
            vec![("copper-plate", 1.0), ("iron-gear-wheel", 1.0)],
            5.0,
            1.0,
        ),
        (
            "transport-belt",
            vec![("iron-plate", 1.0), ("iron-gear-wheel", 1.0)],
            0.5,
            2.0,
        ),
    ] {
        data["recipes"][name] = json!({"name":name,"type":"recipe","hidden":false,"enabled":true,"energy":time,"categories":["crafting"],"ingredients":ingredients.iter().map(|(item,amount)|json!({"type":"item","name":item,"amount":amount})).collect::<Vec<_>>(),"products":[{"type":"item","name":name,"amount":amount,"independent_probability":1.0,"shared_probability":{"min":0,"max":1}}]});
        data["items"][name] = json!({"name":name,"type":"item","hidden":false,"stack_size":100,"quality_stats":{"normal":{"spoil_ticks":0}}});
    }
    for (name, w, h, speed, category) in [
        ("assembling-machine-2", 3, 3, 0.75, "crafting"),
        ("electric-furnace", 3, 3, 2.0, "smelting"),
        ("wide-machine", 4, 6, 1.0, "crafting"),
    ] {
        let mut e = prototype(
            name,
            if category == "smelting" {
                "furnace"
            } else {
                "assembling-machine"
            },
            w,
            h,
        );
        e["crafting_categories"] = json!({category:true});
        e["quality_stats"]["normal"] = json!({"crafting_speed":speed,"max_energy_usage":2500.0});
        e["electric_energy_source"] = json!({"drain":50.0});
        data["entities"][name] = e;
    }
    for (name, speed) in [("transport-belt", 0.03125), ("fast-transport-belt", 0.0625)] {
        let mut e = prototype(name, "transport-belt", 1, 1);
        e["belt_speed"] = json!(speed);
        data["entities"][name] = e;
        let prefix = if name.starts_with("fast-") {
            "fast-"
        } else {
            ""
        };
        for (kind, w) in [("splitter", 2), ("underground-belt", 1)] {
            let name = format!("{prefix}{kind}");
            let mut e = prototype(&name, kind, w, 1);
            e["belt_speed"] = json!(speed);
            e["max_underground_distance"] = json!(5);
            data["entities"][name] = e;
        }
    }
    for name in [
        "steel-chest",
        "iron-chest",
        "wooden-chest",
        "constant-combinator",
    ] {
        let kind = if name == "constant-combinator" {
            name
        } else {
            "container"
        };
        data["entities"][name] = prototype(name, kind, 1, 1);
    }
    for (name, rot, ext, reach) in [
        ("fast-inserter", 0.04, 0.1, 1),
        ("inserter", 0.014, 0.035, 1),
        ("long-handed-inserter", 0.02, 0.05, 2),
    ] {
        let mut e = prototype(name, "inserter", 1, 1);
        e["inserter_pickup_position"] = json!([0, -reach]);
        e["inserter_drop_position"] = json!([0, reach as f64 + 0.2]);
        e["electric_energy_source"] = json!({"drain":8.0});
        e["quality_stats"]["normal"] = json!({"inserter_rotation_speed":rot,"inserter_extension_speed":ext,"max_energy_usage":980.0});
        data["entities"][name] = e;
    }
    for (name, size, radius, wire) in [
        ("small-electric-pole", 1, 2.5, 7.5),
        ("medium-electric-pole", 1, 3.5, 9.0),
        ("big-electric-pole", 2, 2.0, 32.0),
        ("substation", 2, 9.0, 18.0),
    ] {
        let mut e = prototype(name, "electric-pole", size, size);
        e["quality_stats"]["normal"] =
            json!({"supply_area_distance":radius,"max_wire_distance":wire});
        data["entities"][name] = e;
    }
    data
}

fn prototype(name: &str, kind: &str, w: u32, h: u32) -> Value {
    let x = f64::from(w) / 2.0 - 0.35;
    let y = f64::from(h) / 2.0 - 0.35;
    json!({"name":name,"type":kind,"hidden":false,"tile_width":w,"tile_height":h,"collision_box":{"left_top":{"x":-x,"y":-y},"right_bottom":{"x":x,"y":y}},"selection_box":{"left_top":{"x":-x,"y":-y},"right_bottom":{"x":x,"y":y}},"fluid_boxes":[],"quality_stats":{"normal":{}}})
}

pub fn catalog() -> GameData {
    serde_json::from_value(catalog_value()).unwrap()
}
