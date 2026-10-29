//! Stable, versioned catalog. Extra runtime fields are retained in `properties`.

use std::{
    collections::BTreeMap,
    fmt,
    io::{BufReader, Read},
    marker::PhantomData,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

pub const SCHEMA_VERSION: u32 = 1;

/// Empty Lua arrays can also be encoded as `{}`.
pub(crate) fn lua_vec<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct LuaVecVisitor<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> de::Visitor<'de> for LuaVecVisitor<T> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an array or an empty Lua table")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            Vec::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }

        fn visit_map<A: de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            if map.next_entry::<String, Value>()?.is_some() {
                return Err(de::Error::custom("expected an array or an empty Lua table"));
            }
            Ok(Vec::new())
        }
    }
    deserializer.deserialize_any(LuaVecVisitor(PhantomData))
}

/// Factorio's Lua JSON encoder represents an empty table as `[]`, even for maps.
pub(crate) fn lua_map<'de, D, T>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct LuaMapVisitor<T>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>> de::Visitor<'de> for LuaMapVisitor<T> {
        type Value = BTreeMap<String, T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a map or an empty Lua table")
        }

        fn visit_map<A: de::MapAccess<'de>>(
            self,
            map: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            BTreeMap::deserialize(de::value::MapAccessDeserializer::new(map))
        }

        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            if seq.next_element::<Value>()?.is_some() {
                return Err(de::Error::custom("expected a map or an empty Lua table"));
            }
            Ok(BTreeMap::new())
        }
    }
    deserializer.deserialize_any(LuaMapVisitor(PhantomData))
}

/// All prototypes, including hidden and initially locked recipes.
///
/// Keys are internal Factorio names. Positions and sizes use tiles; recipe energy
/// uses seconds. Other fields retain the units from the Factorio runtime API.
#[derive(Debug, Serialize, Deserialize)]
pub struct GameData {
    pub schema_version: u32,
    pub game_version: String,
    #[serde(deserialize_with = "lua_map")]
    pub mods: BTreeMap<String, String>,
    #[serde(deserialize_with = "lua_map")]
    pub recipes: BTreeMap<String, Recipe>,
    #[serde(deserialize_with = "lua_map")]
    pub items: BTreeMap<String, Item>,
    #[serde(deserialize_with = "lua_map")]
    pub entities: BTreeMap<String, Entity>,
    #[serde(deserialize_with = "lua_map")]
    pub fluids: BTreeMap<String, Prototype>,
    #[serde(deserialize_with = "lua_map")]
    pub technologies: BTreeMap<String, Prototype>,
    #[serde(deserialize_with = "lua_map")]
    pub tiles: BTreeMap<String, Prototype>,
    #[serde(deserialize_with = "lua_map")]
    pub qualities: BTreeMap<String, Prototype>,
    #[serde(deserialize_with = "lua_map")]
    pub space_locations: BTreeMap<String, Prototype>,
    #[serde(deserialize_with = "lua_map")]
    pub surfaces: BTreeMap<String, Prototype>,
    #[serde(deserialize_with = "lua_map")]
    pub surface_properties: BTreeMap<String, Prototype>,
}

impl GameData {
    /// Load a previous export; no Factorio process is started.
    pub fn from_reader(reader: impl Read) -> Result<Self> {
        let data: Self = serde_json::from_reader(BufReader::new(reader))
            .context("Invalid Factorium catalog JSON")?;
        data.validate()?;
        Ok(data)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "Unsupported catalog schema {}; expected {SCHEMA_VERSION}",
            self.schema_version
        );
        ensure!(!self.game_version.is_empty(), "Missing game version");
        ensure!(
            self.mods.get("base") == Some(&self.game_version),
            "Catalog game version does not match the base mod version"
        );
        ensure!(!self.recipes.is_empty(), "Catalog contains no recipes");
        ensure!(!self.items.is_empty(), "Catalog contains no items");
        ensure!(!self.entities.is_empty(), "Catalog contains no entities");
        for (key, recipe) in &self.recipes {
            ensure!(
                key == &recipe.prototype.name,
                "Recipe key/name mismatch: {key}"
            );
            ensure!(recipe.energy > 0.0, "Invalid recipe duration: {key}");
            ensure!(
                !recipe.categories.is_empty(),
                "Recipe has no categories: {key}"
            );
        }
        for (key, item) in &self.items {
            ensure!(key == &item.prototype.name, "Item key/name mismatch: {key}");
        }
        for (key, entity) in &self.entities {
            ensure!(
                key == &entity.prototype.name,
                "Entity key/name mismatch: {key}"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Prototype {
    pub name: String,
    #[serde(rename = "type")]
    pub prototype_type: String,
    pub hidden: bool,
    /// Localised strings, flags, restrictions, and type-specific runtime values.
    #[serde(flatten)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Recipe {
    #[serde(deserialize_with = "lua_vec")]
    pub categories: Vec<String>,
    /// Crafting time in seconds at crafting speed 1.
    pub energy: f64,
    #[serde(deserialize_with = "lua_vec")]
    pub ingredients: Vec<Ingredient>,
    #[serde(deserialize_with = "lua_vec")]
    pub products: Vec<Product>,
    /// Initial prototype state, independent of research in an existing save.
    pub enabled: bool,
    #[serde(flatten)]
    pub prototype: Prototype,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ingredient {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub amount: f64,
    /// Temperature bounds and other constraints, without losing modded fields.
    #[serde(flatten)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Product {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_max: Option<f64>,
    /// Independent roll, multiplied by the shared probability interval length.
    pub independent_probability: f64,
    /// Preserves correlation between products from the same craft in Factorio 2.1.
    pub shared_probability: ProbabilityRange,
    /// Temperature, productivity exclusions, spoilage, and other product metadata.
    #[serde(flatten)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProbabilityRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Item {
    pub stack_size: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place_result: Option<String>,
    #[serde(flatten)]
    pub prototype: Prototype,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entity {
    /// Engine-resolved placement footprint, including explicit mod overrides.
    pub tile_width: u32,
    pub tile_height: u32,
    pub collision_box: BoundingBox,
    pub selection_box: BoundingBox,
    #[serde(default, deserialize_with = "lua_map")]
    pub crafting_categories: BTreeMap<String, bool>,
    #[serde(deserialize_with = "lua_vec")]
    pub fluid_boxes: Vec<Value>,
    #[serde(deserialize_with = "lua_map")]
    pub quality_stats: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub prototype: Prototype,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub left_top: Position,
    pub right_bottom: Position,
}
