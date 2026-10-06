# Bit8 Specification

Product version: 0.1.0

## 1. What is Bit8?

Bit8 is a tiny 2D fantasy game runtime.

Bit8 is designed around:
- Rust runtime
- Lua scripting
- 64×64 internal resolution
- 8×8 sprites and tiles
- human-readable project files
- a very small API
- simple game development without a large editor

Bit8 is NOT intended to become a general-purpose game engine.

The initial development environment is VS Code.
A dedicated Bit8 editor may be developed later.

---

## 2. Core Philosophy

Bit8 should remain:

- small
- readable
- predictable
- portable
- easy to learn

Do not add systems unless they are required by an actual Bit8 game.

Avoid:
- ECS
- scene graphs
- visual scripting
- complex physics engines
- plugin systems
- networking
- 3D
- large editor frameworks

---

## 3. Runtime

The Bit8 runtime is written in Rust.

Games are scripted using Lua.

Target Lua version:

Lua 5.4

Architecture:

Bit8 Game
    ↓
Lua
    ↓
Bit8 API
    ↓
Rust Runtime
    ↓
Platform Backend

The game must not directly depend on the desktop backend.

---

## 4. Display

Internal resolution:

64 × 64 pixels

Sprite/tile size:

8 × 8 pixels

The internal framebuffer is always 64×64.

The host window may scale the framebuffer using integer scaling.

For example:

64×64 × 8 = 512×512

Pixel art must remain sharp.

No texture filtering should be applied.

## Maps

Tiles are 8×8 game pixels and the framebuffer remains 64×64 game pixels.
The recommended/default room is 64×64 tiles (512×512 game pixels, 4,096
cells), equivalent to 8×8 framebuffer-sized regions. This does not change the
framebuffer resolution or tile size.

`.b8map` files remain variable-sized; the 64×64 size is a default, not a
validation requirement. Existing Map Core size limits remain authoritative.
Camera Nodes supply a centered 64×64-pixel runtime viewport; map dimensions
remain independent of that viewport.

---

## 5. Game Loop

Bit8 games use three primary functions:

```lua
func init()
end

func update()
end

func draw()
end
```
