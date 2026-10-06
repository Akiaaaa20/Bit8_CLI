# Sprite data foundation (0.1.0A Phase 1)

`bit8.sprites.toml` is an optional project-level definition file, independent
of `.b8map`. Nothing creates it automatically. Existing projects need no migration.

```toml
version = 1

[sprite.Player]
preview = "A4"

[sprite.Player.animation.idle]
frames = ["A4"]
fps = 2

[sprite.Player.animation.walk]
frames = ["A1", "A2", "A3", "A4"]
fps = 8
loop = true
```

Names must be nonempty and unique. `preview` is optional; animations may be
omitted entirely. Each animation requires a nonempty string frame array and
integer `fps` from 1 through 30 inclusive (not necessarily a divisor of 30).
`loop` is boolean and defaults to true. Unsupported version, malformed TOML,
duplicate definitions, unknown fields and invalid values are errors.

All preview/frame strings resolve through existing `bit8.assets.toml` registered
tilesheets: `A4` is still Group A's fourth 8×8 cell, not a Sprite name or numeric
legacy index. Unknown groups, invalid IDs and out-of-range cells fail explicitly.
No PNG paths, coordinates, indices or pixel data belong in Sprite definitions.

Effective preview: explicit preview first, otherwise the first frame of `idle`,
otherwise the first frame of the first declared animation, otherwise none.
The Rust loader uses TOML source spans to preserve declaration order, not
alphabetical or hash order. Derived previews are never written back to the file.

Core API (`sprite_definitions`): `SpriteRegistry::parse` validates structure;
`validate_assets` separately validates tile references against the existing
registered tilesheets. `load_project` performs both and returns `None` if the
optional file is absent. Queries: `get_sprite`, `get_animation`, `effective_preview`.
Call asset validation before consuming references parsed independently.

0.1.0A defined the data only. In 0.1.0B, RuntimeSession loads and validates this
optional registry before executing project source or init callbacks; invalid
definitions fail startup. Node playback is described in [Runtime animation](runtime-animation.md).
0.1.0C adds optional [Map Node Sprite selection](map-node-sprites.md) and static
design-time preview. Map Workspace now provides read-only animation information
and file-only Open Definition navigation, not an animation editor or playback UI.
`spr(A4, x, y)` / `sprite(A4, x, y)` remain direct tile drawing APIs.
