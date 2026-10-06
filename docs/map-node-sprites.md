# Map Node Sprites (0.1.0C Phase 1)

Ordinary Nodes can optionally persist the name of a project SpriteDefinition:

```toml
[[node_state.nodes]]
id = "N1"
name = "Player"
x = 32
y = 48
enabled = true
script = "player.b8"
sprite = "Player"
visual = "A3" # Optional legacy fallback, not deleted by Sprite selection.
```

No new ID system or migration is introduced. Missing `sprite` stays missing.
Core map loading and Runtime setup validate names against the existing
SpriteRegistry; unknown references report map path and Node ID/name.
Syntax-only `Bit8Map::parse` remains project-independent; consumers with project
data use `validate_sprites`, as map loading and Runtime do.

RuntimeSession loads Sprite definitions before loading/validating the map.
Scripted ordinary Nodes bind the persisted Sprite before their source/init
callbacks execute. For example, init can call `self:play("idle")` without any
`self:sprite("Player")` call. Rendering remains explicit through `self:spr()`;
assignment alone never draws a Node. Runtime `self:sprite("Cat")` overrides
playback binding without changing the map or editor design state.

Map Workspace offers None and the core Sprite names in Properties. Changes
use the existing WorkspaceEdit document lifecycle: dirty/save/revert, undo/redo
and reopen behave like other Node properties. None removes only the Sprite
field. Tiles, scripts, legacy Visual, collider and stable Node identity remain
unchanged. No operation rewrites `.b8` source.

Static editor presentation precedence:

1. Persisted Sprite's effective preview.
2. Legacy/manual Visual (only without Sprite).
3. Existing supported static script spr hint (only without Sprite/Visual).
4. Generic Node marker.

A valid Sprite with **no effective preview** remains authoritative but renders
the generic marker; it does not masquerade as a legacy Visual/script hint.
Removing Sprite reveals the preserved fallback. Camera retains its special
marker, viewport and snapping; it has no ordinary Sprite selector. Collider
overlays remain independent, never inferred from Sprite size.

The existing `inspect tilesheets --json` response additionally includes
`sprites: [{"name":"Player","preview":"A4","animations":[{"name":"idle","frames":["A4"],"fps":2,"loop":true}]}]`; preview may be null. Rust's
existing effective-preview method resolves explicit/idle/first-animation order.
The frontend validates this summary and uses the existing palette-mapped tile
data, never parses Sprite TOML or resolves animation ordering. Reopening reads
definition changes; existing asset refresh notifications also reload summaries.
There is no extra Sprite file watcher or animated preview.

0.1.0D Phase 1 Properties exposes a read-only Sprite Info section with the
ordered animations, frame IDs, FPS and loop state supplied by this DTO.
Preview unavailable is explicit and never falls through to legacy Visual.
Legacy Visual stays editable as a compatibility/fallback field. Inspector
selection itself writes no map, script, assets or Sprite definitions. An older
CLI without animation metadata shows an actionable update message, not inferred
animation information. The CLI must be reinstalled for the new inspector data.

Runtime timing/input/animation APIs and direct `spr(A4,x,y)` / `sprite(A4,x,y)`
are unchanged. Direct tile calls are not bidirectionally synchronized with
design Sprite or Visual. Package versions remain unchanged.

0.1.0D Phase 2 adds **Open Definition** next to Sprite Info for an ordinary
Node with a known persisted Sprite. It resolves the current Node assignment
and opens that project's `bit8.sprites.toml` through VS Code without editing.
No assignment, missing definitions and Camera Nodes have no navigation control;
a missing/unopenable file reports a warning and is never created. Sprites
without preview/animations can still open their definition file.

Navigation uses the permitted file-only fallback: Registry uses TOML source
spans during parsing for declaration order but does not retain source ranges
in its public inspection DTO. Exact Sprite/animation range selection and
per-animation navigation are intentionally deferred rather than adding another
TOML parser or changing the frozen data model. The action's tooltip explicitly
states this limitation. Inspector preview/order/Visual fallback are unchanged.

## Manual test

Install the current CLI (`cargo install --offline --path . --force`) and the
test VSIX. Use a project with registered A4–A7 and Player idle/walk definitions.
Open Map Workspace, select Player, choose Sprite Player, verify its preview,
save and reopen. A script containing init `self:play("idle")`, input-driven
move/play in update, and `self:spr()` in draw should run without explicit bind.
Run BIT8 GAME: RIGHT moves/plays walk; release selects idle. Clear Sprite and
verify Visual/static/generic fallback. GUI live acceptance remains manual.
