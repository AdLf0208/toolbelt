#!/usr/bin/env python3
"""Carry the Tool Belt slice (2.2.10) into content/.

The jar is the exact ATM10 one, resolved by SHA-1 from the pack record (see
../../_shared/carry-work/toolbelt/SOURCE.md). Asset bytes are copied verbatim
from the jar; the item models are upgraded to 26.3's `items/` + `models/item/`
spellings, and the recipe drops NeoForge's `neoforge:conditions` block.

Run: python3 tools/carry_content.py [--jar PATH] [--check]
  --check  compare the files on disk with what the carry would write, exit 1
           on a difference (no writes).
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_JAR = (
    ROOT.parent / "_shared/carry-work/toolbelt" / "ToolBelt-1.21.1-2.2.10.jar"
)
JAR_SHA1 = "96862e5d8df67d63b749fc15d18262f3736d1828"
MOD = "toolbelt"
BELT = "belt"

# Bytes copied unchanged from the jar.
VERBATIM = [
    f"assets/{MOD}/textures/item/{BELT}.png",
    f"assets/{MOD}/textures/item/belt_buckle.png",
    f"assets/{MOD}/textures/gui/{BELT}.png",
]
LANG = [
    "da_dk", "de_de", "en_us", "es_es", "es_mx", "hr_hr", "ja_jp", "ko_kr",
    "nl_nl", "pl_pl", "ru_ru", "tr_tr", "uk_ua", "zh_cn", "zh_tw",
]

MANIFEST = f"""\
# Port-owned content mod for the Tool Belt 2.2.10 slice: the belt item and its
# menu assets. The item models, textures, lang and the belt recipe are the
# upstream mod's own BSD-3-Clause assets, read from the exact ATM10 jar (see
# ../../_shared/carry-work/toolbelt/SOURCE.md).
id = "{MOD}"
name = "Tool Belt"
version = "0.1.0"
api = "0.2"
side = "both"
capabilities = ["registries"]
"""

# 1.21.1 `item/generated` model, minus the dyed-belt override (that model is
# not carried in slice 1).
MODEL = {
    "parent": "minecraft:item/generated",
    "textures": {
        "layer0": f"{MOD}:item/{BELT}",
        "layer1": f"{MOD}:item/belt_buckle",
    },
}

# 26.3 item model definition.
ITEMS = {
    "model": {"type": "minecraft:model", "model": f"{MOD}:item/{BELT}"},
}

# The block-free item: a plain item with one stack.
ITEM_DECL = {
    "template": "minecraft:stick",
    "max_stack_size": 1,
    "components": {"!minecraft:cooking_fuel": {}},
}

# Upstream crafting recipe, in 26.3's spelling (no NeoForge conditions).
RECIPE = {
    "type": "minecraft:crafting_shaped",
    "category": "equipment",
    "key": {
        "i": "minecraft:iron_ingot",
        "l": "minecraft:leather",
        "s": "minecraft:string",
    },
    "pattern": ["sls", "l l", "lil"],
    "result": {"id": f"{MOD}:{BELT}", "count": 1},
}


def carried(jar: zipfile.ZipFile) -> dict[str, bytes]:
    out: dict[str, bytes] = {}
    out[f"assets/{MOD}/models/item/{BELT}.json"] = json_bytes(MODEL)
    out[f"assets/{MOD}/items/{BELT}.json"] = json_bytes(ITEMS)
    out[f"data/{MOD}/horizon/item/{BELT}.json"] = json_bytes(ITEM_DECL)
    out[f"data/{MOD}/recipe/{BELT}.json"] = json_bytes(RECIPE)
    out["horizon-mod.toml"] = MANIFEST.encode()
    for name in VERBATIM:
        out[name] = jar.read(name)
    for lang in LANG:
        name = f"assets/{MOD}/lang/{lang}.json"
        out[name] = jar.read(name)
    return out


def json_bytes(value) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--jar", type=Path, default=DEFAULT_JAR)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data = args.jar.read_bytes()
    digest = hashlib.sha1(data).hexdigest()
    if digest != JAR_SHA1:
        raise SystemExit(f"{args.jar}: SHA-1 {digest} != {JAR_SHA1}")
    with zipfile.ZipFile(args.jar) as jar:
        files = carried(jar)
    content = ROOT / "content"
    differences = 0
    for name, data in sorted(files.items()):
        target = content / name
        if args.check:
            if not target.exists() or target.read_bytes() != data:
                differences += 1
                print(f"differs: {name}", file=sys.stderr)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    if args.check:
        if differences:
            raise SystemExit(f"{differences} content file(s) differ")
        print(f"{len(files)} content files match the carry")
    else:
        print(f"wrote {len(files)} content files")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
