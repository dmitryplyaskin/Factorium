"""Build static sprite metadata from Factorio's resolved data-stage dump.

Only paths and geometry are written. Game textures stay in the installation.
Usage: python scripts/build_entity_graphics.py DUMP GAME_DATA [OUTPUT]
"""

import hashlib
import json
from pathlib import Path
import struct
import sys


DIRECTIONS = [(0, "north"), (4, "east"), (8, "south"), (12, "west")]
ENTITIES = {
    "assembling-machine": ["assembling-machine-1", "assembling-machine-2", "assembling-machine-3",
                          "centrifuge", "chemical-plant", "crusher", "cryogenic-plant",
                          "electromagnetic-plant", "foundry", "oil-refinery"],
    "furnace": ["electric-furnace", "recycler"],
    "electric-pole": ["small-electric-pole", "medium-electric-pole", "big-electric-pole", "substation"],
    "inserter": ["inserter", "fast-inserter", "long-handed-inserter", "bulk-inserter", "stack-inserter"],
    "container": ["wooden-chest", "iron-chest", "steel-chest"],
    "constant-combinator": ["constant-combinator"],
}


def build(dump, game_data):
    assets = {}

    def frame(sprite, direction, *, sheet_directions=1, offset=(0, 0)):
        if not sprite or not isinstance(sprite, dict):
            return []
        facing = dict(DIRECTIONS)[direction]
        if facing in sprite:
            return frame(sprite[facing], direction, offset=offset)
        if "layers" in sprite:
            return [layer for child in sprite["layers"]
                    for layer in frame(child, direction, sheet_directions=sheet_directions, offset=offset)]
        if "sheet" in sprite:
            return frame(sprite["sheet"], direction, sheet_directions=sheet_directions, offset=offset)
        if sprite.get("draw_as_glow") or sprite.get("draw_as_light") or sprite.get("blend_mode") == "additive":
            return []
        filename = sprite.get("filename") or (sprite.get("filenames") or [None])[0]
        if not filename or "empty.png" in filename:
            return []
        module, relative = filename[2:].split("__/", 1)
        path = f"{module}/{relative}"
        contents = (game_data / path).read_bytes()
        image_width, image_height = struct.unpack(">II", contents[16:24])
        asset = hashlib.sha256(path.encode()).hexdigest()[:16]
        assets[asset] = {"file": path, "width": image_width, "height": image_height}
        width = sprite.get("width", sprite.get("size"))
        height = sprite.get("height", sprite.get("size"))
        if not width or not height:
            raise ValueError(f"Missing dimensions: {filename}")
        column = direction // 4 if sprite.get("direction_count", sheet_directions) == 4 else 0
        shift = sprite.get("shift", [0, 0])
        if isinstance(shift, dict):
            shift = [shift.get("x", 0), shift.get("y", 0)]
        result = {"asset": asset, "width": width, "height": height,
                  "x": sprite.get("x", 0) + column * width, "y": sprite.get("y", 0),
                  "scale": sprite.get("scale", 1),
                  "shift": [shift[0] + offset[0], shift[1] + offset[1]]}
        if sprite.get("draw_as_shadow"):
            result["shadow"] = True
        if result["x"] + width > image_width or result["y"] + height > image_height:
            raise ValueError(f"Frame outside texture: {filename}")
        return [result]

    entities = {}
    for kind, names in ENTITIES.items():
        for name in names:
            prototype = dump.get(kind, {}).get(name)
            if prototype is None:
                continue
            definition = {"kind": kind, "directions": {}}
            for direction, facing in DIRECTIONS:
                if kind in ("assembling-machine", "furnace"):
                    graphics = prototype["graphics_set"]
                    layers = frame(graphics.get("animation") or graphics.get("idle_animation"), direction)
                    for visual in graphics.get("working_visualisations", []):
                        if any(visual.get(k) for k in ("apply_recipe_tint", "effect", "enabled_by_name")):
                            continue
                        position = visual.get(f"{facing}_position", [0, 0])
                        layers += frame(visual.get(f"{facing}_animation") or visual.get("animation"), direction, offset=position)
                elif kind == "electric-pole":
                    layers = frame(prototype["pictures"], direction)
                elif kind == "inserter":
                    layers = frame(prototype["platform_picture"], direction, sheet_directions=4)
                elif kind == "constant-combinator":
                    layers = frame(prototype["sprites"], direction)
                else:
                    layers = frame(prototype["picture"], direction)
                entry = {"layers": layers}
                if kind == "electric-pole":
                    point = prototype["connection_points"][direction // 4]["wire"]["copper"]
                    entry["wire"] = point
                definition["directions"][str(direction)] = entry
            if kind == "inserter":
                definition["arm"] = {
                    "base": frame(prototype["hand_base_picture"], 0)[0],
                    "hand": frame(prototype["hand_open_picture"], 0)[0],
                    "pickup": prototype["pickup_position"], "drop": prototype["insert_position"],
                }
            entities[name] = definition
    return {"version": 1, "assets": dict(sorted(assets.items())), "entities": dict(sorted(entities.items()))}


if __name__ == "__main__":
    output = Path(sys.argv[3] if len(sys.argv) > 3 else "web/entity-graphics.json")
    result = build(json.loads(Path(sys.argv[1]).read_text(encoding="utf-8")), Path(sys.argv[2]))
    output.write_text(json.dumps(result, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    print(f"{len(result['entities'])} entities, {len(result['assets'])} textures: {output}")
