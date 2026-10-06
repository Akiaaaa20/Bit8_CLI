# Bit8 frontend architecture

Bit8 project and runtime semantics do not belong to VS Code. The project
model is intentionally small:

```text
registered tilesheet resources
          ↓
       .b8map
          ↓
      game/runtime
```

`.b8map` files are variable-sized (up to the existing Map Core limit), and
may exceed the 64×64-pixel framebuffer. The Map Workspace's pan and zoom are
editor presentation state only. They are not written to project data. Runtime
Camera Nodes already provide centered viewport transforms; automatic world
transitions are not implemented. Tilesheets are registered
project resources, not standalone Bit8 workspaces.

Tiles are 8×8 game pixels and the framebuffer is fixed at 64×64 game pixels.
The recommended/default map is 64×64 tiles (512×512 game pixels, 4,096 cells),
so it spans 8×8 framebuffer-sized regions. The official tilesheet demo places
its existing 8×8 artwork in the center of each 64×64 map and leaves other cells
empty. The Map Workspace opens a 64×64 map at a deterministic 12.5% zoom
baseline, centered in the viewport; smaller maps use a larger initial zoom.
Runtime `map()` draws through the active centered Camera transform, or from
origin (0,0) without an enabled Camera. Sprite definitions and runtime playback
are Rust-owned; Node Sprite selection and static inspection are frontend UI.

## Layers

- **Project data and core services:** Rust owns validation and asset identity.
  CLI inspection DTOs are authoritative inputs to frontend adapters. Small
  TypeScript models (`project_model.ts` and `map_editor_model.ts`) validate or
  transform data without VS Code or DOM dependencies; they do not replace Rust
  validation.
- **Runtime:** `RuntimeSession` owns game execution. `bit8 host <project>`
  exposes the existing NDJSON `ready`, `step`, `frame`, `error`, `stop`, and
  `exit` boundary.
- **Runtime backend:** `RuntimeBackend` accepts a project URI and complete
  Bit8 button state. `NativeHostRuntimeBackend` translates a local `file:` URI,
  spawns `bit8 host`, and owns NDJSON/process cleanup. The Game Surface
  schedules and presents frames but does not implement runtime behavior.
- **Frontend adapter:** VS Code owns Custom Editor/Webview lifecycle, editor
  events, WorkspaceEdit, URI-to-Webview resource conversion, and presentation.
  The Map Workspace opens and edits the actual `.b8map` document; it does not
  maintain a hidden document copy. It does not define project formats or
  runtime lifecycle.

## Resource and capability boundary

Use VS Code `Uri` and `workspace.fs` for workspace resource identity and
metadata access where available. The Map Workspace lists `.b8map` resources
only within the active workspace folder and consumes registered tilesheet
inspection from the native Bit8 CLI. PNGs remain normal host-editor resources;
explicit Add to Bit8 / Apply Bit8 Name commands invoke the existing Bit8 CLI
asset operations. Map asset IDs and map validity continue to come from Rust.
Native inspection, registration and game running currently require a local
project and the installed `bit8` executable.

`FrontendCapabilities` makes native runtime, workspace editing, and Custom
Editor availability explicit. Future frontends can reuse project formats,
inspection data and editor-neutral actions (`selectMap`, `selectTool`,
`selectTile`, `paintCell`, `panViewport`, `zoomViewport`). Pointer/trackpad
events are VS Code adapter details; no iPad, browser, or remote runtime is
implemented here.
