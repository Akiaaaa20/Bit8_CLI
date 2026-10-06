# Bit8 0.1.0

Bit8 is a small 2D game runtime written in Rust with Lua 5.4 scripting. Games
draw into a fixed 64×64 framebuffer; the desktop frontend presents it using
nearest-neighbor scaling.

See [CHANGELOG](CHANGELOG.md) for the 0.1.0 capability summary. This repository
is undergoing local final-release preparation; no publication is implied.
Licensed under the [MIT License](LICENSE), copyright AKI.

Optional project Sprite/Animation definitions are documented in
[Sprite data foundation](docs/sprite-data.md).

[Node runtime animation](docs/runtime-animation.md).

[Runtime-owned fixed 30 Hz timing](docs/fixed-runtime-tick.md).

[Map Node Sprite selection and preview](docs/map-node-sprites.md).

## Run a project

A project directory contains `main.b8` (preferred) or legacy `main.lua`:

```sh
cargo run -- run games/tilesheet_demo
# or, after installing the CLI:
bit8 run games/tilesheet_demo
```

The desktop window polls the arrow keys and maps Z to A and X to B. Escape or
closing the window exits. From the repository root, install the CLI with:

```sh
cargo install --path . --locked
```

Project paths may be relative to the current directory or absolute.

## Frontend host protocol

Frontends that provide their own display and input can start Bit8 without a
native game window:

```sh
bit8 host games/tilesheet_demo
```

The host uses NDJSON over stdin/stdout. Send one `step` command per frame with
the complete held-button list, for example
`{"type":"step","buttons":["RIGHT","A"]}`; send
`{"type":"stop"}` to exit. Stdout contains protocol messages only: `ready`,
one 64×64 `frame` per step, `error`, and `exit`. Frame pixels are row-major
`0xRRGGBB` values. Diagnostics go to stderr. The host protocol is frontend-
independent; the VS Code extension uses it for its Game Surface.

Simulation is fixed at 30 Hz by RuntimeSession, independent of frame requests.
The host normally measures elapsed with a monotonic clock; each step may run
zero through five updates, then draw once. For deterministic headless execution,
step also accepts optional integer `elapsed_ns` (for example `33333333` for one
tick). See [fixed timing and input latching](docs/fixed-runtime-tick.md).

## Tilesheets and asset IDs

Projects explicitly register PNG tilesheets in `bit8.assets.toml`. Each PNG
must have dimensions divisible by 8. Cells are numbered left-to-right,
top-to-bottom; registry group IDs are stable and are never inferred from
filenames. Thus IDs such as `A1`, `A2`, and `B1` retain both group and cell
identity. Palette index 0 is transparent when drawing a sprite.

PNG files remain in the host editor's normal Image Preview/Text Editor flow.
Use the PNG's Explorer context menu commands **Bit8: Add to Bit8** or
**Bit8: Apply Bit8 Name** to explicitly register an image or apply the
canonical `<stem> (<GROUP>).png` name. Registration and identity are handled
by the Bit8 CLI and `bit8.assets.toml`.

Inspect registered sheets with:

```sh
bit8 inspect tilesheets games/tilesheet_demo
bit8 inspect tilesheets games/tilesheet_demo --json
```

## Maps

`world.b8map` is a version 1 text map with positive `width` and `height` up to
256 tiles. Each following row has exactly the declared number of cells. `--`
is empty; all other cells must be registered asset IDs:

```text
version = 1
width = 4
height = 2

A1 A1 A1 A1
A1 -- A4 A1
```

Inspect maps with `bit8 inspect map <project> <map-file>`; add `--json` for
machine-readable output. For example:

```sh
bit8 inspect map games/tilesheet_demo games/tilesheet_demo/world.b8map
```

At runtime, `map()` draws the project's `world.b8map`. `mget(x, y)` reads a
zero-based tile coordinate and returns its registered asset ID or `nil`.
`mset(x, y, tile)` changes only the current RuntimeSession's in-memory map;
passing `nil` clears a cell. Runtime edits do not modify the source file and
are discarded when the session ends.

Tiles are 8×8 game pixels. The framebuffer remains 64×64 game pixels. The
recommended/default room and the official demo maps are 64×64 tiles, or
512×512 game pixels (4,096 logical cells). A map is not a framebuffer: the
64×64-pixel framebuffer covers only an 8×8-tile region.

The VS Code **Bit8 Map Editor** is the Map Workspace for `.b8map` files. Its
project-local picker lists only maps within the active workspace folder. The
sidebar combines Pencil, Eraser, Pan and registered tiles; the map viewport
supports pan and zoom. Painting edits the actual VS Code document through
WorkspaceEdit, preserving save, undo/redo, revert and dirty state. The editor
uses a deterministic initial zoom based on map dimensions (64×64 maps open at
12.5%, showing the full map at about 512×512 CSS px in a typical viewport),
then supports free pan and zoom. CSS size is presentation only. `.b8map`
remains variable-sized (up to 256×256 tiles); smaller and larger valid maps are
still supported. The framebuffer remains 64×64 game pixels. The enabled
Camera with the smallest numeric Node ID supplies the center of the 64×64 viewport; without one the origin is
(0,0). Runtime `map()` and sprite drawing use that world-to-screen transform.

The project model is deliberately linear: tilesheet resources feed `.b8map`
data, which a game may consume.

## Node Sprite workflow

Register a PNG → use stable tile IDs → define a Sprite in `bit8.sprites.toml`
→ assign the Sprite to an ordinary Map Node → assign its `.b8` script.
For example, a Player Sprite may use preview `A4`, idle frames `["A4"]` at
2 FPS and walk frames `["A4", "A5", "A6", "A7"]` at 8 FPS, all registered
8×8 cells. A persisted `sprite = "Player"` binds before Node init:

```lua
func init()
    self:play("idle")
end
func update()
    if btn(RIGHT) then
        self:move(1, 0)
        self:play("walk")
    else
        self:play("idle")
    end
end
func draw()
    self:spr()
end
```

Map Workspace shows a static preview and read-only animation information.
Open Definition opens the project's Sprite TOML (file-only navigation).
Legacy Visual remains a fallback only when no Sprite is assigned.
Direct `spr(A4,x,y)` / `sprite(A4,x,y)` and numeric sprites remain valid
low-level APIs. See [Node Sprite workflow](docs/map-node-sprites.md).

Box Collider offsets and dimensions are integer game pixels. Mark registered
map tiles Solid with `bit8 tilesheet solid <project> A1 true` or the existing
editor tile metadata control. `self:collide(dx,dy)` is a non-mutating query;
`self:move(dx,dy)` sweeps integer pixels, X before Y, against Solid tiles.
Raw `self.x/self.y` assignment remains unconstrained, and runtime mutation never
rewrites `.b8map`. Collision coordinates are independent of Camera rendering.

## VS Code

The `bit8-vscode` extension provides Bit8 syntax and language-service support,
the Map Workspace, explicit PNG registration commands, and **Explorer → BIT8 GAME**. Run and Stop
are VS Code commands; the Explorer Game Surface displays only the host's
framebuffer. Its logical size remains 64×64 and its square display uses the
largest size that fits (`min(available width, available height)`), aligned at
the top-left. Focus the canvas for gameplay input: arrows map to
UP/DOWN/LEFT/RIGHT, Z to A, and X to B. Collapsing the view does not stop the
game.
