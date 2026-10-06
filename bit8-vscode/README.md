# Bit8 VS Code extension

Version 0.1.0 is a small development layer for the separately installed Bit8
runtime. Its Run Game command starts `bit8 host <workspace-folder>` and shows
the game in the Explorer's **BIT8 GAME** view; it does not bundle or implement the runtime.
Install Bit8 first and ensure `bit8` is available in VS Code's `PATH`. Running
`bit8 run <project>` in a terminal continues to open the native desktop game.

Licensed under the MIT License, copyright AKI. The package includes `LICENSE`
and `CHANGELOG.md` with the license text and 0.1.0 capabilities.
Source repository: [Bit8_CLI](https://github.com/Akiaaaa20/Bit8_CLI).

## Explorer Game Surface

Use **Bit8: Run Game** (or the editor-title play button) to start the current
workspace project in the Explorer's **BIT8 GAME** view. The view contains only
the framebuffer canvas. Run again replaces the running host session and starts
the newly selected project. Use **Bit8: Stop Game** to end the session.

Click the canvas to focus gameplay input. Arrow keys map to UP/DOWN/LEFT/RIGHT,
Z maps to A and X maps to B. Keys are sent as the complete held-button set once
per rendered frame; input is cleared when the canvas loses focus or the view is
hidden. Collapsing or hiding the view does not stop the host; the extension
retains the latest frame and continues the session. The canvas renders the
host's 64x64 framebuffer as the largest square that fits the view, aligned at
the top-left with pixelated scaling, and does not access project files itself.

## Code Templates

In a `.b8` file, type `Template` and choose **Bit8 Template: Basic Game**. The
completion inserts editable Bit8 source using the current runtime API.

## Source Files and Language Mode

Bit8 source is UTF-8 plain text with a `.b8` extension. VS Code automatically
assigns `.b8` files to Bit8 mode and starts the existing Bit8 Language Service.
The extension's grammar, Language Service completions and hover documentation,
templates, and Run button are available there.

`.lua` remains standard Lua by default. It is also accepted as a legacy Bit8
project entry when a project has no `main.b8`; to edit legacy Bit8 source with
Bit8 syntax highlighting and diagnostics, open the file and run **Bit8: Use
Bit8 Language Mode**. This switches only that document for the current editor
session; it does not change workspace or user settings.

## Bit8 Language Service

Install the editor-independent syntax service from the repository root:

```sh
cargo install --path language-service --locked
```

The extension starts `bit8-language-server` over standard LSP stdio only for
documents in Bit8 language mode. The service shares the runtime's `func`
preprocessor and compiles the translated source with Lua 5.4 without executing
it. It reports syntax errors and provides completion/hover for the implemented
Bit8 API. Other editors can launch `bit8-language-server` as an LSP stdio
server. Third-party Lua tooling continues to work normally on `.lua` documents;
native `.b8` documents use the Bit8 language ID instead.

## Tilesheet resources

PNG files use VS Code's normal image/text editors; Bit8 does not claim a
separate PNG workspace. In the Explorer, right-click a PNG inside a project
and choose **Bit8: Add to Bit8** to explicitly register it, or **Bit8: Apply
Bit8 Name** to canonically rename a registered sheet. Group/cell identity,
retired-ID allocation and collision handling remain authoritative in the Bit8
CLI and `bit8.assets.toml`. These actions require the current Bit8 CLI in PATH.

## Map Workspace

Open a `.b8map` file and choose **Bit8 Map Editor** from **Open With...**. Its
left sidebar lists only `.b8map` files in that document's workspace folder,
then provides Pencil, Eraser, Pan and all registered tilesheet cells. The
Selected Tile dropdown shares the palette's current selection. The right-side
viewport uses 64 CSS pixels per tile at 100% (512×512 for an 8×8 map), with
a dimension-based initial zoom (12.5% for 64×64 maps), pointer-centered wheel zoom and Pan-tool
drag, and uses nearest-neighbor tile rendering. Map dimensions stay variable.

Painting uses the active VS Code text document and WorkspaceEdit, preserving
save/revert, undo/redo and dirty state. Invalid maps are not rewritten; use
**Text Editor** from **Open With...** to repair them.

For ordinary Nodes, assign a script and project Sprite in Properties. Sprite
definitions are authored in `bit8.sprites.toml` using registered stable tile
IDs. The persisted Sprite binds before Node init, where `self:play("idle")`
can select playback; Node draw must explicitly call `self:spr()`. Sprite Info
shows the core-resolved static preview, ordered frames, FPS and loop state.
Open Definition opens the project's Sprite file without modifying it; exact
definition range navigation is not provided. Legacy Visual remains compatible
as a fallback when Sprite is unassigned. Camera and collider overlays are
independent of Sprite presentation.

When a `.b8` document opens, the extension records language-service startup
and connection status in the **Bit8** Output channel. If startup fails, the
channel and an error message show the install command or `BIT8_LANGUAGE_SERVER`
override to use.

## Develop

1. Open this `bit8-vscode` folder in VS Code.
2. Run `npm install` and `npm run compile` in its terminal.
3. Press F5 to launch the Extension Development Host.

Run `npm test` to compile and run the smoke tests in an Extension Development
Host. If VS Code is installed at a non-default location, set
`VSCODE_EXECUTABLE` to its executable path before running the tests.

## Package for local installation

Run:

```sh
npm install
npm run compile
npm run package
```

This creates `bit8-vscode-0.1.0.vsix`. In VS Code, use **Extensions:
Install from VSIX...** and select that file, or install it with
`code --install-extension bit8-vscode-0.1.0.vsix`.
