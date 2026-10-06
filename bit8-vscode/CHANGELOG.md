# Changelog

## 0.1.0 — local release preparation

BIT8 is MIT licensed, copyright AKI. Source repository metadata now identifies
https://github.com/Akiaaaa20/Bit8_CLI; publication has not occurred.
The extension remains a thin frontend; install the Bit8 CLI and
Language Service separately.

- Native `.b8` language registration, syntax highlighting, Rust LSP
  diagnostics/completion/hover and game-loop templates, with ordinary `.lua`
  isolation preserved.
- Run/Stop/Restart through `bit8 host`, displayed in Explorer BIT8 GAME with
  a pixelated square framebuffer and focused gameplay input.
- Explicit Add to Bit8 / Apply Bit8 Name PNG actions, stable group/cell IDs,
  palette-mapped Map Workspace tiles and asset refresh.
- Map painting, pan/zoom, Node and Camera editing, collider/Solid presentation,
  script assignment and real WorkspaceEdit save/revert/undo/redo.
- Persisted Node Sprite assignment, static preview, read-only Sprite Inspector
  frames/FPS/loop information, and file-only Open Definition navigation.
  Legacy Visual remains a compatible fallback.
- Correct first script assignment after Collider creation to remain at Node
  scope in the map file.

Runtime execution, animation and fixed 30 Hz simulation remain in Rust.
No animation editor, Map Layers, large-image backgrounds or `img()` API is
included.
