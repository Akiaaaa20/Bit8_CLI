# Runtime animation (0.1.0B Phase 1)

Node scripts can bind shared definitions from `bit8.sprites.toml`:

Prefer assigning `sprite = "Player"` in Map Workspace; it binds before init,
so the explicit `self:sprite("Player")` below is unnecessary for that workflow.
The explicit method remains valid for runtime-only overrides.

```lua
func init()
    self:sprite("Player")
    self:play("idle")
end

func update()
    if btn("RIGHT") then
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

- `self:sprite(name)` binds a SpriteDefinition. Unknown names are errors.
  The same name does nothing; a different name clears playback (frame 0,
  accumulator 0, finished false). Binding does not automatically play idle.
- `self:play(animation)` requires a bound Sprite and a known animation.
  Changing animation starts frame 0. Repeating the current name never restarts,
  including after a non-loop animation finishes. Switching away and back restarts.
- `self:spr()` draws the current frame at `self.x/self.y`, or the existing
  effective preview if no animation is selected. No Sprite/preview is a safe no-op.
  It calls the same renderer as direct tile drawing, including Camera transform,
  clipping, palette mapping and index-0 transparency. It never advances time.

Missing Sprite/animation errors include the relevant names; Node callback errors
also retain Node and `.b8` script context. Invalid definitions/references fail
project setup before game init. Missing optional definitions remain compatible.

## Logical timing and lifecycle

Simulation uses fixed **30 Hz logical ticks**, without Lua delta time.
Each enabled scripted Node runs `update()` and then advances playback once, even
without an update callback or when offscreen. All Node updates finish before
main/Node draws. Disabled Nodes do not advance. Init consumes no ticks.

To present a newly selected frame 0, a Sprite/animation change during that Node's
update resets state and the post-update advancement leaves it unchanged for that
tick. Normal accumulation begins on the next tick. Same-name calls do not count
as changes, so repeated binding/play in update keeps animating. An animation
selected in init accumulates on the first normal update tick.

At each normal playback tick, add integer `fps` (1–30) to the accumulator;
while it is at least 30, subtract 30 and advance one frame. For 8 FPS the
accumulator is 8, 16, 24, then 2 with one frame advanced. No divisor restriction
or floating-point timing is needed. 30 FPS advances every normal playback tick.

Looping wraps to the first frame, never finishing. Non-looping displays the final
frame for its full interval, then holds it with internal finished state and stops
accumulating; there are no events, automatic idle transition or restart API.

Phase 2 adds [Runtime-owned fixed timing](fixed-runtime-tick.md): frontends may
continue presenting at 60 Hz, while RuntimeSession consumes measured elapsed
into 30 Hz simulation ticks. Drawing can repeat between ticks without advancing
playback. Host step still returns one frame, but may execute zero through five
ticks; its default elapsed source is Rust's monotonic clock. Animation FPS is
not scaled again by elapsed time.

Every Node owns independent playback state; immutable Sprite definitions are
shared. Playback is runtime-only and never writes `.b8map` or Sprite definitions.
0.1.0C optionally [binds a persisted design Sprite before init](map-node-sprites.md);
runtime overrides still never persist or rewrite source.
Raw position assignment, movement/collision and Camera behavior are unchanged.
Global `spr(A4, x, y)` / `sprite(A4, x, y)` (and numeric legacy IDs) remain tile
APIs, distinct from the Node's named Sprite binding.
