# Changelog

## 0.1.0 — local release preparation

This describes the accepted capabilities prepared for 0.1.0, not a publication
announcement. BIT8 is MIT licensed, copyright AKI. Source repository metadata
now identifies https://github.com/Akiaaaa20/Bit8_CLI; publication is a separate step.

- Rust/Lua 5.4 runtime and CLI: `bit8 run <project>` opens the native desktop
  window; `bit8 host <project>` serves editor-independent NDJSON frames/input.
- Fixed 64×64 framebuffer, nearest-neighbor presentation, six logical buttons,
  optional init/update/draw, and Runtime-owned fixed 30 Hz simulation ticks.
- Explicit PNG registration, canonical naming, stable group/cell IDs and
  automatic logical 8×8 row-major slicing. Palette-mapped editor previews agree
  with Runtime. Retired groups are not automatically reused.
- Variable-sized maps, map/mget/mset APIs and Map Workspace painting with
  pan/zoom, cell coordinates, save/revert, undo/redo and dirty-state preservation.
- Stable Nodes, script assignment/lifecycle, centered Camera rendering,
  Box Collider persistence, Solid map tiles, `self:collide()` prospective queries
  and swept X-before-Y `self:move()` resolution. Raw position assignment remains
  unconstrained; runtime mutation does not write project files.
- Project Sprite definitions, ordered animation frames/FPS/loop state and
  per-Node playback through `self:sprite()`, `self:play()` and `self:spr()`.
- Persisted Map Node Sprite assignment binds before Node init. Map Workspace
  shows static Sprite previews and a read-only Sprite Inspector; Open Definition
  opens the project Sprite TOML without changing it (file-only navigation).
  Legacy Visual/static script hints remain compatible fallback presentation.
- Thin VS Code frontend: native `.b8` mode, Rust Language Service diagnostics,
  completion/hover, templates, PNG registration, Map Workspace and Explorer
  BIT8 GAME via the independent host. Ordinary `.lua` behavior stays separate.
- RC regression correction: assigning the first script after adding a Collider
  inserts the script at Node scope rather than inside its collider subtable.

### Preferred authoring workflow

PNG → stable tile IDs → `bit8.sprites.toml` Sprite definition → Map Node Sprite
assignment → Node init `self:play("idle")` → Node draw `self:spr()`.
Node update may switch animation and move using the accepted input/collision
APIs. Persisted Sprite binding means init does not need `self:sprite()`.
Direct `spr(A4,x,y)` / `sprite(A4,x,y)` and legacy numeric sprites remain valid
low-level tile APIs. Sprite/Map/asset persisted schema versions remain 1.

Map Layers, `img()`, large-image backgrounds, animation editing/timelines,
state machines and new rendering APIs are not part of 0.1.0.
