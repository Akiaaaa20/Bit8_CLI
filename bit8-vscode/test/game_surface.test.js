const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { test } = require("node:test");

const root = path.resolve(__dirname, "..");
const manifest = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const source = fs.readFileSync(path.join(root, "src/game_surface.ts"), "utf8");
const extensionSource = fs.readFileSync(path.join(root, "src/extension.ts"), "utf8");

test("Bit8 Game is contributed as a webview inside the existing Explorer container", () => {
  const view = manifest.contributes.views.explorer.find((item) => item.id === "bit8.gameSurface");
  assert.deepEqual(view, { id: "bit8.gameSurface", name: "BIT8 GAME", type: "webview" });
  assert.equal(manifest.contributes.viewsContainers, undefined);
  assert.ok(manifest.activationEvents.includes("onView:bit8.gameSurface"));
});

test("Run Game uses the Explorer provider instead of opening an editor Game View", () => {
  assert.match(extensionSource, /registerWebviewViewProvider\(\s*"bit8\.gameSurface"/);
  assert.match(extensionSource, /gameSurface\.run\(folder\.uri\.toString\(\)\)/);
  assert.doesNotMatch(extensionSource, /createWebviewPanel/);
  assert.doesNotMatch(source, /createWebviewPanel/);
  assert.match(source, /createRuntimeBackend\(projectUri/);
  assert.match(source, /RuntimeBackend/);
  assert.doesNotMatch(source, /node:child_process|spawn\(/);
  assert.match(source, /await client\.step\(\[\.\.\.this\.heldButtons\]\)/);
});

test("native host implementation is isolated behind the frontend-neutral runtime interface", () => {
  const backend = fs.readFileSync(path.join(root, "src/runtime_backend.ts"), "utf8");
  const nativeBackend = fs.readFileSync(path.join(root, "src/native_host_runtime_backend.ts"), "utf8");
  assert.doesNotMatch(backend, /from ["'](vscode|node:)/);
  assert.match(backend, /interface RuntimeBackend/);
  assert.match(nativeBackend, /spawn\(executable, \["host", projectPath\]/);
  assert.match(extensionSource, /nativeHostRuntimeBackendFactory/);
});

test("Explorer canvas keeps a 64x64 framebuffer and fits the largest top-aligned square", () => {
  assert.match(source, /<canvas id="framebuffer" width="64" height="64"/);
  assert.match(source, /createImageData\(64, 64\)/);
  assert.match(source, /imageSmoothingEnabled = false/);
  assert.match(source, /image-rendering: pixelated/);
  assert.match(source, /const size = Math\.max\(0, Math\.min\(surface\.clientWidth, surface\.clientHeight\)\)/);
  assert.match(source, /canvas\.style\.width = size \+ 'px'/);
  assert.match(source, /canvas\.style\.height = size \+ 'px'/);
  assert.match(source, /align-items: flex-start; justify-content: flex-start/);
  assert.match(source, /new ResizeObserver\(resizeCanvas\)\.observe\(surface\)/);
  assert.doesNotMatch(source, /canvas\.style\.width = surface\.clientWidth|canvas\.style\.height = surface\.clientHeight/);
  assert.doesNotMatch(source, /<button|<p|>Running<|>Stopped<|FPS|Click to focus/);
});

test("keyboard focus and blur report complete held state without stealing editor input", () => {
  assert.match(source, /canvas\.addEventListener\('keydown'/);
  assert.match(source, /document\.activeElement !== canvas/);
  assert.match(source, /canvas\.addEventListener\('blur', clearInput\)/);
  assert.match(source, /window\.addEventListener\('blur', clearInput\)/);
  assert.match(source, /function clearInput\(\) \{ held\.clear\(\); sendInput\(\); \}/);
  assert.match(source, /view\.onDidChangeVisibility\(\(\) => \{\s*if \(!view\.visible\) this\.clearInput\(\)/);
});

test("Stop and hidden/disposed views do not own or destroy the host surface", () => {
  assert.match(source, /async stop\(\): Promise<void>/);
  assert.match(source, /await this\.stopCurrentSession\(\)/);
  assert.match(source, /this\.post\(\{ type: "clearInput" \}\)/);
  assert.match(source, /message\.type === 'clearInput'/);
  assert.match(source, /Hiding\/disposal of the view is not a request to stop the host session/);
  assert.match(extensionSource, /retainContextWhenHidden: true/);
  assert.match(source, /private latestFrame:/);
  assert.match(source, /this\.latestFrame = frame/);
});

test("Run replaces the previous process and the host frame loop preserves one in-flight step", () => {
  assert.match(source, /const generation = \+\+this\.generation/);
  assert.match(source, /await this\.stopCurrentSession\(\)/);
  assert.match(source, /while \(this\.client === client && generation === this\.generation && client\.running\)/);
  assert.match(source, /FRAME_MS = 1000 \/ 60/);
  assert.match(extensionSource, /registerBit8MapEditor\(context\)/);
});
