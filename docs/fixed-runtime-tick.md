# Fixed Runtime tick (0.1.0B Phase 2)

RuntimeSession owns simulation debt. Frontends own input sampling, monotonic
elapsed-time measurement and presentation cadence, not the simulation clock.
There is no Lua delta-time API or interpolation.

```rust
session.set_buttons(held_mask); // Complete current UP/DOWN/LEFT/RIGHT/A/B mask.
let framebuffer = session.advance(elapsed)?; // std::time::Duration
```

`runtime_timing` centralizes `FIXED_HZ = 30`, `FIXED_DT = 33,333,333 ns`
(1/30 second rounded down by less than one nanosecond), and
`MAX_CATCH_UP_TICKS = 5`. Duration arithmetic is integer-only. Startup/init adds
no debt and consumes no tick; the first advance accumulates normally.

An advance adds elapsed to the prior fractional remainder, executes at most five
whole ticks, and retains `total_debt % FIXED_DT`. Any additional whole ticks are
discarded, not repaid on later frames. For example, 70 ms produces two updates
and approximately 3.33 ms remainder; a five-second stall produces only five
updates. Even maximum Duration input does not overflow the accumulator.

Each tick commits sampled input/latched presses, runs main update, runs enabled
Node updates in existing stable order, and advances each Node's animation after
its own update. Phase 1 selection/idempotency rules remain unchanged. Tick-end
bookkeeping clears consumed presses. Catch-up ticks do not draw intermediate
frames. Advance draws once after all updates, including when zero ticks ran.
`draw()` may also be called directly; it does not update simulation or consume
pending input. Scripts should keep simulation mutation in update, not draw.

`btn()` is the held mask committed at the logical tick. `btnp()` is the latched
press mask for that tick: a sampled press survives any number of draws and even
a sampled release before the next tick. Thus a short tap can produce btn=false,
btnp=true once. Multiple press edges on the same button before a tick coalesce
into one boolean edge. In catch-up, pending presses appear only on the first
tick; later ticks retain the held state but not the already consumed edge.
Input events never delivered by a frontend cannot be reconstructed by Runtime.

Animation's existing integer FPS accumulator runs once per logical tick, not
per rendered frame and not multiplied by elapsed. 8, 15 and 30 FPS definitions
retain their intended cadence at 30, 60, 120 or irregular frontend rendering.

## Native and host integration

Native minifb polling/presentation remains approximately 60 FPS. It supplies
elapsed from Rust `Instant` to the same RuntimeSession advance API.

`bit8 host` also measures elapsed with `Instant`, initialized after successful
startup. Each valid step samples buttons, advances by the elapsed since the
previous valid step, and returns one frame. Sequence counts emitted frames, not
logical ticks. A step may execute zero through five ticks. Stop/EOF remain
clean; invalid commands are recoverable, emit an error, and do not sample input
or advance the clock. They do not reset the monotonic reference point.

The original command remains valid:

```json
{"type":"step","buttons":["RIGHT"]}
```

For deterministic headless driving, an additive optional integer `elapsed_ns`
overrides measured elapsed for that command (it is a duration, not a timestamp):

```json
{"type":"step","buttons":[],"elapsed_ns":33333333}
```

Zero is valid; negative, fractional and values outside u64 are protocol errors.
Explicit steps also reset the monotonic reference to their receipt time, so
switching back to measured steps does not replay old elapsed time. Real native
and VS Code frontends use measured time, never hard-coded per-frame deltas.
Protocol output fields are unchanged. VS Code still sends original step
commands, maintains one request in flight, and needs no timing logic changes.

`RuntimeSession::step(mask)` remains a deterministic headless convenience:
sample input and call `advance(FIXED_DT)`. It is not the real frontend clock.
Existing one-tick regression fixtures use it or explicit host durations.
