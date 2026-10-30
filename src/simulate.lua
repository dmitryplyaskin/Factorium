-- cfg is injected as JSON, not interpolated executable source.
local f, opts = cfg.factory, cfg.options
local ingredients = cfg.inputs
local common_input = f.bus ~= nil
local storage_mode = #(f.storage or {}) > 0
local collector_mode = f.bus and #(f.bus.output_entities or {}) > 0
local feeds, observed, chests = {}, {}, {}
local real, rows, outputs, surface, start_tick
local inserted, delivered = 0, 0
local windows, window_count = {}, 0
local report = {
  passed = false, game_version = script.active_mods.base, mods = {},
  entity_count = 0, pole_count = 0, network_count = 0, unpowered_entities = {},
  measured_output_per_second = 0, window_rates = {}, delivered_items = 0,
  inserted_items = 0, input_deliveries = {}, external_input_deliveries = {}, storage_deliveries = {}, output_deliveries = {}, row_outputs = {}, warmup_ticks = opts.warmup_ticks,
  input_measurement_location = common_input and "machines" or "boundary",
  output_measurement_location = storage_mode and "chests" or "belts",
  row_measurement_location = (storage_mode or collector_mode) and "machines" or "boundary",
  measurement_ticks = opts.measurement_ticks, machine_status_counts = {}, errors = {}
}
for name, version in pairs(script.active_mods) do report.mods[name] = version end

local function fail(message) table.insert(report.errors, message) end
local function finish()
  report.inserted_items = inserted
  helpers.write_file("factorium/result.json", helpers.table_to_json(report), false)
  script.finish_test(true) -- A rejected candidate is a report, not a crashed process.
end

script.on_init(function()
  local force = game.forces.player
  -- Enable recipes without granting research productivity or inserter bonuses.
  for _, recipe in pairs(force.recipes) do recipe.enabled = true end
  local extent = 16
  for _, e in ipairs(f.document.blueprint.entities) do
    extent = math.max(extent, math.abs(e.position.x) + 16, math.abs(e.position.y) + 16)
  end
  surface = game.create_surface("factorium-verification", {seed = 1, width = 0, height = 0})
  surface.generate_with_lab_tiles = true
  surface.request_to_generate_chunks({0, 0}, math.ceil(extent / 32) + 2)
  surface.force_generate_chunk_requests()
  surface.always_day = true
  for _, e in pairs(surface.find_entities_filtered({force = "neutral"})) do e.destroy() end
  local inventory = game.create_inventory(1)
  if inventory[1].import_stack(cfg.blueprint_string) ~= 0 then
    fail("Encoded blueprint failed to import cleanly"); finish(); return
  end
  local ghosts = inventory[1].build_blueprint({surface = surface, force = force, position = {0, 0}, build_mode = defines.build_mode.normal})
  if #ghosts ~= #f.document.blueprint.entities then
    fail("Blueprint did not create every entity ghost: " .. #ghosts .. "/" .. #f.document.blueprint.entities); finish(); return
  end
  local gx, gy, bx, by = math.huge, math.huge, math.huge, math.huge
  for _, ghost in pairs(ghosts) do gx = math.min(gx, ghost.position.x); gy = math.min(gy, ghost.position.y) end
  for _, e in ipairs(f.document.blueprint.entities) do bx = math.min(bx, e.position.x); by = math.min(by, e.position.y) end
  local dx, dy = gx - bx, gy - by
  for _, ghost in pairs(ghosts) do
    local _, entity = ghost.revive()
    if not entity then fail("Could not revive an imported entity") end
  end
  if #report.errors > 0 then finish(); return end
  real = {}
  for _, e in ipairs(f.document.blueprint.entities) do
    local entity = surface.find_entity(e.name, {e.position.x + dx, e.position.y + dy})
    if not entity then fail("Imported entity missing: " .. e.entity_number); finish(); return end
    real[e.entity_number] = entity
    report.entity_count = report.entity_count + 1
    if entity.type == "inserter" and entity.inserter_stack_size_override ~= 1 then
      fail("Inserter stack-size override was lost during blueprint import")
    end
    if entity.type == "assembling-machine" then
      local recipe = entity.get_recipe()
      if not recipe or recipe.name ~= f.recipe then fail("Machine recipe was lost during import") end
    end
  end
  -- Detect relevant startup-setting/prototype drift even when mod version strings
  -- stayed the same. Use the game's resolved arithmetic for productivity.
  for name, size in pairs(f.entity_sizes) do
    local p = prototypes.entity[name]
    if not p or p.tile_width ~= size[1] or p.tile_height ~= size[2] then fail("Equipment footprint differs from the catalog: " .. name) end
  end
  local machine = real[f.rows[1].machines[1]]
  local recipe = prototypes.recipe[f.recipe]
  if #recipe.ingredients ~= #ingredients or #recipe.products ~= 1 or recipe.products[1].name ~= f.request.product then
    fail("Recipe ingredients/products differ from the catalog")
  else
    local output = recipe.get_product_amount(1, machine.productivity_bonus)
    local amounts = {}
    for _, ingredient in ipairs(recipe.ingredients) do amounts[ingredient.name] = ingredient.amount end
    for _, input in ipairs(ingredients) do
      if not amounts[input.item] or math.abs(amounts[input.item] / output - input.per_second / f.request.rate) > 1e-8 then fail("Recipe amounts/productivity differ from the catalog: " .. input.item) end
    end
    if math.abs(machine.crafting_speed / recipe.energy - f.machine_crafts_per_second) > 1e-8 then fail("Crafting speed differs from the catalog") end
  end
  for _, input in ipairs(ingredients) do
    if prototypes.item[input.item].get_spoil_ticks("normal") > 0 then fail("Ingredient spoils in this game profile: " .. input.item) end
  end
  if prototypes.item[f.request.product].get_spoil_ticks("normal") > 0 then fail("Product spoils in this game profile") end
  if math.abs(prototypes.entity[f.request.belt].belt_speed * 480 - f.belt_items_per_second) > 1e-8 then fail("Belt speed differs from the catalog") end
  local pole_proto = prototypes.entity[f.request.pole]
  if math.abs(pole_proto.get_supply_area_distance("normal") - f.power.supply_area_distance) > 1e-8 or math.abs(pole_proto.get_max_wire_distance("normal") - f.power.max_wire_distance) > 1e-8 then fail("Pole supply area or wire reach differs from the catalog") end
  if #report.errors > 0 then finish(); return end
  for _, marker in ipairs(f.markers or {}) do
    local cb = real[marker.entity_number].get_control_behavior()
    local section = cb and cb.get_section(1)
    local slot = section and section.get_slot(1)
    if not slot or not slot.value or slot.value.name ~= marker.item or slot.min ~= math.ceil(marker.rate) then
      fail("Input marker signal was lost during blueprint import")
    end
    if real[marker.entity_number].combinator_description ~= f.document.blueprint.entities[marker.entity_number].player_description then
      fail("Input marker description was lost during blueprint import")
    end
  end
  if #report.errors > 0 then finish(); return end
  inventory.destroy()
  local networks = {}
  for _, e in pairs(real) do
    if e.type == "electric-pole" then
      report.pole_count = report.pole_count + 1
      if e.electric_network_id then networks[e.electric_network_id] = true end
    end
  end
  for _ in pairs(networks) do report.network_count = report.network_count + 1 end
  if report.network_count ~= 1 then fail("Imported copper wires did not form one network") end

  -- Add one external pole, disconnect its automatic wires, and connect ONLY the
  -- declared entry. The harness must not repair missing internal connections.
  local entry = real[f.power.connection_entity]
  local bounds = {left = math.huge, right = -math.huge, top = math.huge, bottom = -math.huge}
  for _, e in pairs(real) do
    local box = e.bounding_box
    bounds.left = math.min(bounds.left, box.left_top.x); bounds.right = math.max(bounds.right, box.right_bottom.x)
    bounds.top = math.min(bounds.top, box.left_top.y); bounds.bottom = math.max(bounds.bottom, box.right_bottom.y)
  end
  local pp = prototypes.entity[f.request.pole]
  local reach = pp.get_max_wire_distance("normal")
  local supply = pp.get_supply_area_distance("normal")
  local external
  for y = math.floor(entry.position.y - reach), math.ceil(entry.position.y + reach) do
    for x = math.floor(entry.position.x - reach), math.ceil(entry.position.x + reach) do
      local pos = {x = x + pp.tile_width % 2 * 0.5, y = y + pp.tile_height % 2 * 0.5}
      local outside = pos.x + pp.tile_width / 2 < bounds.left or pos.x - pp.tile_width / 2 > bounds.right or pos.y + pp.tile_height / 2 < bounds.top or pos.y - pp.tile_height / 2 > bounds.bottom
      if outside and (pos.x-entry.position.x)^2+(pos.y-entry.position.y)^2 <= reach^2 and surface.can_place_entity({name = f.request.pole, position = pos, force = force}) then
        external = surface.create_entity({name = f.request.pole, position = pos, force = force})
        break
      end
    end
    if external then break end
  end
  if not external then fail("Cannot attach external supply to the declared power entry"); finish(); return end
  local connector = external.get_wire_connector(defines.wire_connector_id.pole_copper, true)
  connector.disconnect_all()
  if not connector.connect_to(entry.get_wire_connector(defines.wire_connector_id.pole_copper, true), true) then
    fail("External power wire exceeds reach"); finish(); return
  end
  local generator
  for y = math.floor(external.position.y - supply), math.ceil(external.position.y + supply) do
    for x = math.floor(external.position.x - supply), math.ceil(external.position.x + supply) do
      if surface.can_place_entity({name = "electric-energy-interface", position = {x, y}, force = force}) then
        generator = surface.create_entity({name = "electric-energy-interface", position = {x, y}, force = force})
        break
      end
    end
    if generator then break end
  end
  if not generator then fail("No room for external test generator"); finish(); return end
  generator.electric_buffer_size = 1e12
  generator.power_production = 1e12
  generator.energy = 1e12
  rows = {}
  outputs = {}
  local by_output = {}
  for i, row in ipairs(f.rows) do
    local production = row.production_output or row.output
    local lane = production.lanes == "left" and defines.transport_line.left_line or defines.transport_line.right_line
    local measured = {
      row_index = i, output_entity = row.output.entity_number, lane = production.lanes,
      target_per_second = row.output.rate / 1.05, measured_per_second = 0,
      delivered_items = 0, window_rates = {}
    }
    table.insert(report.row_outputs, measured)
    local state = {inputs = {}, measured = measured, window_count = 0}
    local ports = {row.input}
    for _, port in ipairs(row.additional_inputs or {}) do table.insert(ports, port) end
    for _, port in ipairs(ports) do
      local delivered_input = {row_index = i, input_entity = port.entity_number, item = port.item,
        supplied_per_second = port.rate, measured_per_second = 0, inserted_items = 0}
      table.insert(report.input_deliveries, delivered_input)
      table.insert(state.inputs, {entity = real[port.entity_number], item = port.item, rate = port.rate,
        credit = 0, lane = port.lanes == "right" and 2 or 1, measured = delivered_input})
    end
    table.insert(rows, state)
    local port = by_output[row.output.entity_number]
    if not port then
      port = {entity = real[row.output.entity_number], rows = {}, unexpected = 0}
      port.measured = {output_entity = row.output.entity_number, left_items = 0, right_items = 0}
      if not storage_mode then table.insert(report.output_deliveries, port.measured) end
      by_output[row.output.entity_number] = port
      table.insert(outputs, port)
    end
    port.rows[lane] = state
  end
  local by_input = {}
  for _, row in ipairs(rows) do for _, input in ipairs(row.inputs) do by_input[input.entity.unit_number .. ":" .. input.item] = input.measured end end
  for _, port in ipairs(cfg.external_inputs) do
    local measured = by_input[real[port.entity_number].unit_number .. ":" .. port.item]
    if common_input then
      measured = {row_index = 0, input_entity = port.entity_number, item = port.item,
        supplied_per_second = port.rate, measured_per_second = 0, inserted_items = 0}
      table.insert(report.external_input_deliveries, measured)
    end
    table.insert(feeds, {entity = real[port.entity_number], item = port.item, rate = port.rate,
      credit = 0, lane = port.lanes == "right" and 2 or 1, both = port.lanes == "both", measured = measured})
  end
  for _, mapping in ipairs(cfg.observed_inserters or {}) do
    local entity = real[mapping.entity_number]
    table.insert(observed, {entity = entity, row = rows[mapping.row_index], kind = mapping.kind,
      input_index = mapping.input_index, last_count = 0})
  end
  for _, bank in ipairs(f.storage or {}) do for _, id in ipairs(bank.chests) do
    local measured = {chest_entity = id, stored_items = 0, added_items = 0}
    table.insert(report.storage_deliveries, measured)
    table.insert(chests, {inventory = real[id].get_inventory(defines.inventory.chest), measured = measured, previous = 0})
  end end
  start_tick = game.tick
end)

script.on_event(defines.events.on_tick, function()
  if not start_tick then return end
  local tick = game.tick - start_tick
  if tick == 2 then
    for id, entity in pairs(real) do
      if entity.type ~= "electric-pole" and entity.prototype.electric_energy_source_prototype and not entity.is_connected_to_electric_network() then
        table.insert(report.unpowered_entities, id)
      end
    end
    if #report.unpowered_entities > 0 then fail("Some consumers have no connection to the external electricity supply") end
  end
  for _, input in ipairs(feeds) do
    input.credit = math.min(input.credit + input.rate / 60, input.rate + 2)
    while input.credit >= 1 do
      local line = input.entity.get_transport_line(input.lane)
      if not line.insert_at_back({name = input.item, count = 1}) then break end
      if input.both then input.lane = 3 - input.lane end
      input.credit = input.credit - 1
      inserted = inserted + 1
      if tick > opts.warmup_ticks then input.measured.inserted_items = input.measured.inserted_items + 1 end
    end
  end
  for _, hand in ipairs(observed) do
    local count = hand.entity.held_stack.valid_for_read and hand.entity.held_stack.count or 0
    local dropped = math.max(0, hand.last_count-count)
    if tick > opts.warmup_ticks and dropped > 0 then
      if hand.kind == "input" and common_input then
        for _, input in ipairs(hand.row.inputs) do
          if input.item == hand.last_item then input.measured.inserted_items = input.measured.inserted_items+dropped end
        end
      elseif hand.kind == "output" and (storage_mode or collector_mode) then
        hand.row.measured.delivered_items = hand.row.measured.delivered_items+dropped
        hand.row.window_count = hand.row.window_count+dropped
      end
    end
    hand.last_count = count
    if count > 0 then hand.last_item = hand.entity.held_stack.name end
  end
  for _, chest in ipairs(chests) do
    local count = chest.inventory.get_item_count(f.request.product)
    local added = count-chest.previous
    if tick > opts.warmup_ticks then
      chest.measured.added_items = chest.measured.added_items+added
      delivered = delivered+added; window_count = window_count+added
    end
    chest.previous = count; chest.measured.stored_items = count
  end
  -- Read each physical output once. Shared-belt lanes belong to different rows;
  -- total throughput alone must never conceal a stalled half of a pair.
  if not storage_mode then
  for _, port in ipairs(outputs) do
    for lane = 1, 2 do
      local line = port.entity.get_transport_line(lane)
      local count = line.remove_item({name = f.request.product, count = 100000})
      if tick > opts.warmup_ticks then
        delivered = delivered + count; window_count = window_count + count
        local lane_key = lane == 1 and "left_items" or "right_items"
        port.measured[lane_key] = port.measured[lane_key] + count
        local row = port.rows[lane]
        if collector_mode then
          -- Row contributions are measured at their machine inserters before
          -- streams mix. Boundary belts still determine total delivered flow.
        elseif row then
          row.measured.delivered_items = row.measured.delivered_items + count
          row.window_count = row.window_count + count
        else
          port.unexpected = port.unexpected + count
        end
      end
    end
  end
  end
  if tick > opts.warmup_ticks and (tick-opts.warmup_ticks) % (opts.measurement_ticks/4) == 0 then
    table.insert(windows, window_count * 60 / (opts.measurement_ticks/4)); window_count = 0
    for _, row in ipairs(rows) do
      table.insert(row.measured.window_rates, row.window_count * 60 / (opts.measurement_ticks/4))
      row.window_count = 0
    end
  end
  if tick >= opts.warmup_ticks + opts.measurement_ticks then
    report.delivered_items = delivered
    report.measured_output_per_second = delivered * 60 / opts.measurement_ticks
    report.window_rates = windows
    if delivered < 20 then fail("Too few delivered items for verification; increase measurement duration") end
    if report.measured_output_per_second + 1e-8 < f.request.rate then fail("Measured boundary output is below the requested rate") end
    for _, input in ipairs(report.input_deliveries) do
      input.measured_per_second = input.inserted_items * 60 / opts.measurement_ticks
      if input.inserted_items == 0 or (not common_input and input.measured_per_second + 60 / opts.measurement_ticks + 1e-8 < input.supplied_per_second / 1.05) then
        fail("Ingredient " .. input.item .. " at row " .. input.row_index .. " received less than its requested share")
      end
    end
    for _, input in ipairs(report.external_input_deliveries) do
      input.measured_per_second = input.inserted_items * 60 / opts.measurement_ticks
      if input.inserted_items == 0 or input.measured_per_second + 60 / opts.measurement_ticks + 1e-8 < input.supplied_per_second / 1.05 then
        fail("Shared input belt received less than its requested supply: " .. input.item)
      end
    end
    for _, row in ipairs(rows) do
      local result = row.measured
      result.measured_per_second = result.delivered_items * 60 / opts.measurement_ticks
      -- One item of measurement quantization is allowed, but every row must
      -- actually deliver something, including a short final row of an odd pair.
      -- Passive splitters redistribute flow between rows. Only the aggregate
      -- target applies in bus mode, but a completely stalled row still fails.
      if result.delivered_items == 0 or (not common_input and result.measured_per_second + 60 / opts.measurement_ticks + 1e-8 < result.target_per_second) then
        fail("Row " .. result.row_index .. " (" .. result.lane .. " lane) is below its requested share; increase measurement duration if necessary")
      end
    end
    for _, port in ipairs(outputs) do
      if port.unexpected > 0 then fail("Output arrived on an unassigned belt lane") end
    end
    local smallest, largest = math.huge, 0
    for _, rate in ipairs(windows) do smallest = math.min(smallest, rate); largest = math.max(largest, rate) end
    if largest - smallest > math.max(0.1 * report.measured_output_per_second, 120 / (opts.measurement_ticks/4)) then
      fail("Output has not reached a steady rate; increase warmup/measurement duration")
    end
    local names = {}
    for name, status in pairs(defines.entity_status) do names[status] = name end
    for _, row in ipairs(f.rows) do for _, id in ipairs(row.machines) do
      local name = names[real[id].status] or "unknown"
      report.machine_status_counts[name] = (report.machine_status_counts[name] or 0) + 1
    end end
    report.passed = #report.errors == 0
    finish()
  end
end)
