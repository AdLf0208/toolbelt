# Licence notice — Tool Belt

This port carries the original mod's assets and data, and rewrites its behaviour
in Rust (or, for a data-tier port, declares its blocks from the same data). The
original licence and copyright notices are reproduced verbatim in
[LICENSE](LICENSE).

## Where the licence was read

- The pack jar `ToolBelt-1.21.1-2.2.10.jar` (kept at `licence-cache/`)
  declares `license="BSD (3 clause)"` in `META-INF/neoforge.mods.toml` and
  carries `LICENSE.txt`: the three-clause BSD text with
  `Copyright (c) 2024, David Quintana <gigaherz@gmail.com>`.

## The port

- Original: Tool Belt 2.2.10 by gigaherz (David Quintana) — BSD-3-Clause.
- Licence: **BSD-3-Clause**
- Port to Horizon by AdLf0208: the Rust runtime, the Horizon declarations and the
  build/verification tooling in this repository.
- No Mojang file is included. The carried files are the mod's own assets and
  data; `data/minecraft/tags/…` files only add this mod's ids to vanilla tags.
