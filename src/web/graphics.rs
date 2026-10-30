//! Static game sprite metadata; textures remain in the user's installation.

use crate::GameData;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub(super) struct EntityGraphics {
    pub manifest: Value,
    pub files: HashMap<String, PathBuf>,
}

pub(super) fn load(data: &GameData, root: Option<&Path>) -> EntityGraphics {
    let mut output = EntityGraphics {
        manifest: json!({"assets":{},"entities":{}}),
        files: HashMap::new(),
    };
    let Some(root) = root else { return output };
    let source: Value = serde_json::from_str(include_str!("../../web/entity-graphics.json"))
        .expect("bundled sprite metadata");
    for (id, asset) in source["assets"].as_object().unwrap() {
        let relative = Path::new(asset["file"].as_str().unwrap());
        if !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        {
            continue;
        }
        let file = root.join(relative);
        let mut header = [0_u8; 24];
        let valid = std::fs::File::open(&file)
            .and_then(|mut f| f.read_exact(&mut header))
            .is_ok()
            && header.starts_with(b"\x89PNG\r\n\x1a\n")
            && u32::from_be_bytes(header[16..20].try_into().unwrap()) as u64
                == asset["width"].as_u64().unwrap()
            && u32::from_be_bytes(header[20..24].try_into().unwrap()) as u64
                == asset["height"].as_u64().unwrap();
        if valid {
            output.files.insert(id.clone(), file);
            output.manifest["assets"][id] =
                json!({"width":asset["width"],"height":asset["height"]});
        }
    }
    for (name, definition) in source["entities"].as_object().unwrap() {
        if !data.entities.contains_key(name) {
            continue;
        }
        let mut frames: Vec<&Value> = definition["directions"]
            .as_object()
            .unwrap()
            .values()
            .flat_map(|direction| direction["layers"].as_array().unwrap())
            .collect();
        if let Some(arm) = definition.get("arm") {
            frames.extend([&arm["base"], &arm["hand"]]);
        }
        if !frames.is_empty()
            && frames
                .iter()
                .all(|frame| output.files.contains_key(frame["asset"].as_str().unwrap()))
        {
            output.manifest["entities"][name] = definition.clone();
        }
    }
    output
}
