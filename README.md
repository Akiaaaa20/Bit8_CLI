# Bit8 0.1.0

Bit8 is a tiny Lua-powered 2D game runtime and development toolkit
built in Rust.

Games run on a fixed 64×64 framebuffer with Lua 5.4 scripting.
Bit8 includes a CLI runtime, tilemaps, Sprites and animation, Nodes,
collision, Camera support, a language service, and an integrated
VS Code development workflow.

## Features

- Lua 5.4 scripting with `.b8`
- 64×64 framebuffer
- Fixed 30 Hz simulation
- 8×8 tiles and stable asset IDs
- Tilemaps up to 256×256 tiles
- Sprite and animation definitions
- Scriptable Map Nodes
- Box collision and Solid tiles
- Camera-based world rendering
- Language Service
- VS Code Map Workspace
- Integrated BIT8 GAME view
- Editor-independent host protocol

## Quick Start

### Build and install

From the repository root:

    cargo install --path . --locked

Run a project:

    bit8 run <project-path>

A Bit8 project normally uses:

    main.b8

Legacy `main.lua` projects are also supported.

### Controls

The desktop runtime uses:

    Arrow Keys   Direction
    Z            A
    X            B
    Escape       Exit

## A tiny Bit8 program

    func init()
        x = 28
        y = 28
    end

    func update()
        if btn(LEFT) then x = x - 1 end
        if btn(RIGHT) then x = x + 1 end
        if btn(UP) then y = y - 1 end
        if btn(DOWN) then y = y + 1 end
    end

    func draw()
        cls()
        rectfill(x, y, x + 7, y + 7, 7)
    end

## Project Structure

A Bit8 project can contain:

    my-game/
    ├── main.b8
    ├── world.b8map
    ├── bit8.assets.toml
    ├── bit8.sprites.toml
    ├── player.b8
    └── tilesheet (A).png

Not every project needs every file.

## Tilesheets and Asset IDs

PNG tilesheets are registered through `bit8.assets.toml`.

Tiles are 8×8 game pixels and receive stable IDs such as:

    A1
    A2
    A3
    B1

Asset group identity is explicit and is not inferred from the
filename.

Registered tiles can be drawn directly:

    spr(A4, x, y)

## Maps

Bit8 uses `.b8map` text maps.

    version = 1
    width = 4
    height = 2

    A1 A1 A1 A1
    A1 -- A4 A1

Maps may be up to 256×256 tiles.

At runtime:

    map()
    mget(x, y)
    mset(x, y, tile)

The framebuffer is still 64×64 game pixels. Maps represent a world,
not the framebuffer itself.

## Nodes

Maps may contain Nodes with position, scripts, Sprite bindings,
colliders and other runtime state.

Bit8 follows a simple idea:

> Main assembles the game. Nodes live the game.

A Node script can implement:

    func init()
    end

    func update()
    end

    func draw()
    end

## Sprites and Animation

Sprites are defined in `bit8.sprites.toml` using stable tile IDs.

For example:

    [sprite.Player]
    preview = "A4"

    [sprite.Player.animation.idle]
    frames = ["A4"]
    fps = 2
    loop = true

    [sprite.Player.animation.walk]
    frames = ["A4", "A5", "A6", "A7"]
    fps = 8
    loop = true

A Node with the `Player` Sprite can then use:

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

## Collision

Registered tiles may be marked Solid.

Nodes can use Box Colliders and query or perform collision-aware
movement:

    self:collide(dx, dy)
    self:move(dx, dy)

Movement is resolved one game pixel at a time, X before Y.

## Camera

A Camera Node defines the center of the 64×64 viewport.

World-space maps and sprites are transformed through the active
Camera, while screen-space drawing APIs remain screen-space.

## VS Code

The `bit8-vscode` extension provides:

- Bit8 syntax support
- completion and hover information
- diagnostics
- Map Workspace
- Sprite information
- Node editing
- asset registration
- integrated BIT8 GAME view
- Run and Stop commands

The Game View communicates with the runtime through Bit8's
editor-independent host interface.

## Host Protocol

Other editors and tools can run Bit8 without using the native game
window:

    bit8 host <project-path>

The host communicates using NDJSON over stdin/stdout.

This interface is intentionally independent from VS Code so other
frontends can integrate with Bit8 in the future.

## Architecture

Bit8 keeps the runtime separate from editor integrations.

    Game Project
         │
         ▼
    RuntimeSession
         │
       ┌─┴──────────┐
       ▼            ▼
    bit8 run      bit8 host
       │            │
       ▼            ▼
    Desktop       Editors /
    Window        Tooling

VS Code is a Bit8 frontend, not the Bit8 runtime itself.

## Documentation

More detailed documentation is available in `docs/`.

The Wiki will contain user-oriented guides for:

- Getting Started
- Bit8 language basics
- Drawing
- Input
- Assets
- Maps
- Nodes
- Sprites and Animation
- Collision
- Camera
- VS Code
- CLI reference
- Host integration

See `CHANGELOG.md` for the 0.1.0 capability summary.

## Status

Bit8 is currently at **0.1.0**.

This is the first public release. The project is still young and its
APIs and file formats may evolve in future versions.

## License

Bit8 is released under the MIT License.

Copyright (c) 2026 AKI
