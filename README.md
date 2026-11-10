# Tool Belt — Horizon port

A port of [Tool Belt](https://github.com/gigaherz/ToolBelt) by gigaherz for
Horizon. This slice is the belt item and its on-item storage.

- The belt holds two to nine slots. Hold it and right-click with an allowed item
  in your off hand to insert it; sneak-right-click to take the first slot back
  out; right-click otherwise to open a menu that lists the belt's contents.
- Allowed means upstream's rule: non-stackable items by default, no bundles and
  no other belts; equal stacks with the same components merge into one slot.
- The size and contents live in two item components, so a belt keeps its slots
  and their positions when it travels between players and machines.
- `/toolbelt put|take|info|menu` for headless testing.

The radial key menu, wearing it on a curios slot, the upgrades and the pouch are
later slices. BSD-3-Clause — original copyright and licence notices are kept in
the port repository; see LICENSE and LICENSE-NOTICE.md there.

**Original:** [Tool Belt](https://github.com/gigaherz/ToolBelt) 2.2.10 by
gigaherz, **BSD-3-Clause**. The original
copyright and licence notices are kept in [LICENSE](LICENSE), and
[LICENSE-NOTICE.md](LICENSE-NOTICE.md) records where each was read from.

**Horizon:** a port for [Horizon](https://github.com/horizon-rust/horizon), Minecraft:
Java Edition 26.3 rewritten in Rust. The exact Horizon and horizon-mixins
revisions this port builds against are pinned in `compatibility.json`.

## Install

1. Take `toolbelt.hnmod` from this repository (or build it below).
2. Put it in the game folder's `horizon-native-mods` folder — the game folder is the one
   holding `options.txt`.
3. Native mods are always on once installed; the first install shows a
   trust notice, and the launcher builds the mod for your game.

To try it without installing, start the game with
`--dev-mod toolbelt.hnmod`.

## Build

The port is a native mod (`.hnmod`): carried content plus a Rust runtime and
source mixins.

```sh
cargo test --locked --offline                        # the pure runtime rules
python3 tools/horizon.py stage   --horizon <horizon-checkout>
python3 tools/horizon.py check   --horizon <horizon-checkout>
python3 tools/horizon.py test    --horizon <horizon-checkout>
python3 tools/horizon.py build   --horizon <horizon-checkout>   # rebuild the .hnmod
```

`tools/e2e.sh` is the in-game harness. It expects a campaign checkout
(`$CAMPAIGN_ROOT`), the shared pins under `ports/_shared/pins/`, and the
prepared 26.3 resources (`$HORIZON_RES`, default `.local/horizon-client/26.3`).

## Layout

`content/` is the shipped content; `mixins/toolbelt/` holds the declarations and the runtime; `tools/` builds and verifies the port.

This is an unofficial port: Horizon is not affiliated with the original author.
The licence above is the original's and travels with every copy; see
[LICENSE](LICENSE) and [LICENSE-NOTICE.md](LICENSE-NOTICE.md).
