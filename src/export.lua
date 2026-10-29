-- Runs as an isolated --test-scenario. Only plain values cross the JSON boundary.
local function fields(source, names)
  local result = {}
  for _, name in ipairs(names) do result[name] = source[name] end
  return result
end

local function ref(prototype)
  return prototype and prototype.name or nil
end

local function base(p)
  local result = fields(p, {"name", "type", "hidden", "order", "localised_name", "localised_description"})
  result.group = ref(p.group)
  result.subgroup = ref(p.subgroup)
  return result
end

local function extend(target, values)
  for key, value in pairs(values) do target[key] = value end
  return target
end

local function collect(source, transform)
  local result = {}
  for name, prototype in pairs(source) do result[name] = transform(prototype) end
  return result
end

local function recipe(p)
  local result = extend(base(p), fields(p, {
    "categories", "energy", "ingredients", "products", "main_product", "enabled",
    "hidden_from_player_crafting", "allow_as_intermediate", "allow_intermediates",
    "allow_decomposition", "allowed_effects", "allowed_module_categories",
    "maximum_productivity", "surface_conditions", "emissions_multiplier",
    "allow_inserter_overload", "overload_multiplier", "can_set_quality",
    "preserve_products_in_machine_output"
  }))
  if p.alternative_unlock_methods then
    result.alternative_unlock_methods = {}
    for _, technology in pairs(p.alternative_unlock_methods) do
      table.insert(result.alternative_unlock_methods, technology.name)
    end
    table.sort(result.alternative_unlock_methods)
  end
  return result
end

local function item(p)
  local result = extend(base(p), fields(p, {
    "stack_size", "weight", "fuel_value", "fuel_categories", "flags",
    "module_effects", "category", "tier", "rocket_launch_products",
    "spoil_level", "spoil_quality_change"
  }))
  result.place_result = ref(p.place_result)
  result.plant_result = ref(p.plant_result)
  if p.place_as_tile_result then
    result.place_as_tile_result = fields(p.place_as_tile_result, {"condition", "condition_size", "invert"})
    result.place_as_tile_result.result = ref(p.place_as_tile_result.result)
    result.place_as_tile_result.tile_condition = {}
    for _, tile in pairs(p.place_as_tile_result.tile_condition) do
      table.insert(result.place_as_tile_result.tile_condition, tile.name)
    end
  end
  result.burnt_result = ref(p.burnt_result)
  result.spoil_result = ref(p.spoil_result)
  result.quality_stats = {}
  for name in pairs(prototypes.quality) do
    result.quality_stats[name] = {
      spoil_ticks = p.get_spoil_ticks(name),
      module_effects = p.get_module_effects(name)
    }
  end
  return result
end

local function fluidbox(p)
  local result = fields(p, {"index", "production_type", "pipe_connections"})
  result.filter = ref(p.filter)
  result.volume = p.get_volume("normal")
  result.volume_by_quality = {}
  for name in pairs(prototypes.quality) do result.volume_by_quality[name] = p.get_volume(name) end
  return result
end

local quality_methods = {
  crafting_speed = "get_crafting_speed",
  supply_area_distance = "get_supply_area_distance",
  max_wire_distance = "get_max_wire_distance",
  max_circuit_wire_distance = "get_max_circuit_wire_distance",
  max_energy_usage = "get_max_energy_usage",
  max_energy_production = "get_max_energy_production",
  inserter_extension_speed = "get_inserter_extension_speed",
  inserter_rotation_speed = "get_inserter_rotation_speed",
  researching_speed = "get_researching_speed",
  fluid_capacity = "get_fluid_capacity",
  mining_drill_radius = "get_mining_drill_radius",
  pumping_speed = "get_pumping_speed"
}

local function entity(p)
  local result = extend(base(p), fields(p, {
    "tile_width", "tile_height", "collision_box", "selection_box", "collision_mask",
    "secondary_collision_box", "flags", "build_grid_size", "grid_offset",
    "rotatable", "supports_direction", "direction_count", "allows_flipping",
    "is_building", "items_to_place_this", "fast_replaceable_group",
    "crafting_categories", "ingredient_count", "module_inventory_size",
    "allowed_effects", "allowed_module_categories", "effect_receiver",
    "energy_usage", "heating_energy", "surface_conditions", "tile_buildability_rules",
    "belt_speed", "max_underground_distance", "related_underground_belt",
    "belt_distance", "belt_length", "inserter_pickup_position", "inserter_drop_position",
    "inserter_stack_size_bonus", "filter_count", "mining_speed", "resource_categories",
    "resource_category", "mineable_properties", "resource_drain_rate_percent",
    "distribution_effectivity", "distribution_effectivity_bonus_per_quality_level", "profile",
    "logistic_radius", "construction_radius", "logistic_mode", "lab_inputs"
  }))
  -- Prototype references must be converted to names, never serialized as LuaObjects.
  result.related_underground_belt = ref(p.related_underground_belt)
  result.fixed_recipe = ref(p.fixed_recipe)
  result.fixed_quality = ref(p.fixed_quality)
  result.next_upgrade = ref(p.next_upgrade)
  result.fluid_boxes = {}
  for _, box in pairs(p.fluidbox_prototypes) do table.insert(result.fluid_boxes, fluidbox(box)) end
  if p.electric_energy_source_prototype then
    result.electric_energy_source = fields(p.electric_energy_source_prototype, {
      "buffer_capacity", "drain", "usage_priority", "emissions_per_joule"
    })
  end
  if p.burner_prototype then
    result.burner = fields(p.burner_prototype, {
      "effectivity", "fuel_categories", "fuel_inventory_size", "burnt_inventory_size", "emissions_per_joule"
    })
  end
  result.quality_stats = {}
  for name in pairs(prototypes.quality) do
    local stats = {}
    for key, method in pairs(quality_methods) do stats[key] = p[method](name) end
    stats.inventory_sizes = {}
    for inventory, index in pairs(defines.inventory) do
      stats.inventory_sizes[inventory] = p.get_inventory_size(index, name)
    end
    result.quality_stats[name] = stats
  end
  return result
end

local function technology(p)
  local result = extend(base(p), fields(p, {
    "effects", "enabled", "level", "max_level", "research_unit_count",
    "research_unit_count_formula", "research_unit_energy", "research_unit_ingredients",
    "research_trigger", "upgrade", "essential"
  }))
  result.prerequisites = {}
  for name in pairs(p.prerequisites) do table.insert(result.prerequisites, name) end
  table.sort(result.prerequisites)
  return result
end

script.on_init(function()
  local data = {
    schema_version = 1,
    game_version = script.active_mods.base,
    mods = {},
    recipes = collect(prototypes.recipe, recipe),
    items = collect(prototypes.item, item),
    entities = collect(prototypes.entity, entity),
    fluids = collect(prototypes.fluid, function(p)
      return extend(base(p), fields(p, {
        "default_temperature", "max_temperature", "heat_capacity", "fuel_value", "gas_temperature"
      }))
    end),
    technologies = collect(prototypes.technology, technology),
    tiles = collect(prototypes.tile, function(p)
      return extend(base(p), fields(p, {
        "collision_mask", "items_to_place_this", "can_be_part_of_blueprint", "is_foundation",
        "walking_speed_modifier", "vehicle_friction_modifier", "mineable_properties"
      }))
    end),
    qualities = collect(prototypes.quality, function(p)
      local result = extend(base(p), fields(p, {"level", "next_probability"}))
      result.next = ref(p.next)
      return result
    end),
    space_locations = collect(prototypes.space_location, function(p)
      return extend(base(p), fields(p, {"surface_properties", "solar_power_in_space", "entities_require_heating"}))
    end),
    surfaces = collect(prototypes.surface, function(p)
      return extend(base(p), fields(p, {"surface_properties"}))
    end),
    surface_properties = collect(prototypes.surface_property, function(p)
      return extend(base(p), fields(p, {"default_value", "is_time", "localised_unit_key"}))
    end)
  }
  for name, version in pairs(script.active_mods) do data.mods[name] = version end
  helpers.write_file("factorium/catalog.json", helpers.table_to_json(data), false)
end)

-- Finishing on the first tick gives on_init handlers of enabled mods time to complete.
script.on_nth_tick(1, function() script.finish_test(true) end)
