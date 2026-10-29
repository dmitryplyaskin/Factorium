local machine = table.deepcopy(data.raw["assembling-machine"]["assembling-machine-1"])
machine.name = "factorium-fixture-machine"
machine.tile_width = settings.startup["factorium-fixture-width"].value
machine.tile_height = 7
machine.minable.result = "factorium-fixture-item"
local item = table.deepcopy(data.raw.item["assembling-machine-1"])
item.type = "item-with-tags"
item.name = "factorium-fixture-item"
item.place_result = machine.name
data:extend({machine, item, {
  type = "recipe",
  name = "factorium-fixture-recipe",
  categories = {"chemistry"},
  enabled = false,
  hidden = true,
  energy_required = 2.5,
  icons = {{icon = "__base__/graphics/icons/iron-plate.png", icon_size = 64}},
  ingredients = {{type = "fluid", name = "water", amount = 10, minimum_temperature = 25}},
  results = {{type = "item", name = item.name, amount_min = 1, amount_max = 3, independent_probability = 0.25}}
}})
