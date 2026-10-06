const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { test } = require("node:test");
const { pathToFileURL } = require("node:url");
const { parseTilesheetInspection, safeProjectRelativePath } = require("../out/project_model");
const { frontendCapabilities } = require("../out/frontend_capabilities");
const { nativeFilePath } = require("../out/native_cli");

function tilesheetReport() {
  const palette = [0x000000, 0x1d2b53, 0x7e2553, 0x008751, 0xab5236, 0x5f574f, 0xc2c3c7, 0xfff1e8,
    0xff004d, 0xffa300, 0xffec27, 0x00e436, 0x29adff, 0x83769c, 0xff77a8, 0xffccaa];
  return { tilesheets: [{
    group: "A", file: "tilesheet (A).png", width: 16, height: 8, columns: 2, rows: 1,
    palette,
    cells: [{ id: "A1", x: 0, y: 0, pixels: Array(64).fill(12) }, { id: "A2", x: 1, y: 0, pixels: Array(64).fill(14) }],
  }] };
}

test("parses registered tilesheet project DTOs without depending on VS Code, Node paths, or DOM", () => {
  const sheets = parseTilesheetInspection(tilesheetReport());
  assert.deepEqual(sheets[0].cells.map((cell) => cell.id), ["A1", "A2"]);
  assert.deepEqual(sheets[0].palette.slice(12, 15), [0x29adff, 0x83769c, 0xff77a8]);
  assert.equal(sheets[0].cells[0].pixels[0], 12);
  assert.equal(sheets[0].cells[0].solid, false, "legacy inspection payloads default tiles to non-solid");
  assert.equal(sheets[0].file, "tilesheet (A).png");
  const source = fs.readFileSync(path.join(__dirname, "../src/project_model.ts"), "utf8");
  assert.doesNotMatch(source, /from ["'](vscode|node:)/);
  assert.doesNotMatch(source, /document\.|window\.|Webview/);
});

test("parses Solid metadata by stable tile ID without altering cell order", () => {
  const report = tilesheetReport();
  report.tilesheets[0].cells[1].solid = true;
  const [sheet] = parseTilesheetInspection(report);
  assert.deepEqual(sheet.cells.map(({ id, solid }) => [id, solid]), [["A1", false], ["A2", true]]);
});

test("rejects unsafe project paths and inconsistent tile inspection data", () => {
  for (const value of ["../outside.png", "/outside.png", "C:/outside.png", "a\\b.png"]) {
    assert.throws(() => safeProjectRelativePath(value), /outside the project/);
  }
  assert.throws(() => parseTilesheetInspection({ tilesheets: [{ ...tilesheetReport().tilesheets[0], width: 15 }] }), /inconsistent cell dimensions/);
  assert.throws(() => parseTilesheetInspection({ tilesheets: [{ ...tilesheetReport().tilesheets[0], cells: [] }] }), /invalid cells/);
  assert.throws(() => parseTilesheetInspection({ tilesheets: [{ ...tilesheetReport().tilesheets[0], palette: [0] }] }), /invalid Runtime palette/);
  assert.throws(() => parseTilesheetInspection({ tilesheets: [{ ...tilesheetReport().tilesheets[0], cells: [{ ...tilesheetReport().tilesheets[0].cells[0], pixels: [16] }, tilesheetReport().tilesheets[0].cells[1]] }] }), /invalid cell metadata/);
});

test("frontend capabilities explicitly allow a headless editor frontend without native runtime", () => {
  const editorOnly = frontendCapabilities({ canEditWorkspace: true, canUseCustomEditors: true });
  assert.deepEqual(editorOnly, {
    canRunNativeRuntime: false,
    canEditWorkspace: true,
    canUseCustomEditors: true,
  });
  assert.equal(Object.isFrozen(editorOnly), true);
  assert.equal(frontendCapabilities({ canRunNativeRuntime: true }).canRunNativeRuntime, true);
});

test("native CLI path conversion accepts file URIs and refuses virtual workspace resources", () => {
  const localPath = path.resolve("/tmp/bit8-project");
  assert.equal(nativeFilePath(pathToFileURL(localPath).toString()), localPath);
  assert.throws(() => nativeFilePath("vscode-remote://ssh-remote+host/project"), /local file resources/);
});

test("tilesheet registration actions live on normal PNG Explorer commands, not a PNG editor", () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, "../package.json"), "utf8"));
  assert.equal(manifest.contributes.customEditors.some((editor) => editor.selector.some((selector) => selector.filenamePattern.endsWith(".png"))), false);
  assert.ok(manifest.contributes.commands.some((command) => command.command === "bit8.addTilesheet"));
  assert.ok(manifest.contributes.commands.some((command) => command.command === "bit8.applyTilesheetName"));
  assert.deepEqual(manifest.contributes.menus["explorer/context"].map((entry) => entry.when), [
    "resourceExtname == .png", "resourceExtname == .png",
  ]);
});

test("unreleased world layout contribution and CLI/docs surface are retired", () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, "../package.json"), "utf8"));
  assert.ok(!manifest.contributes.customEditors.some((editor) => editor.viewType.includes("world")));
  assert.ok(!manifest.activationEvents.some((event) => event.includes("worldEditor")));
  assert.equal(fs.existsSync(path.join(__dirname, "../../src/world.rs")), false);
  assert.equal(fs.existsSync(path.join(__dirname, "../src/world_editor.ts")), false);
  assert.equal(fs.existsSync(path.join(__dirname, "../../games/tilesheet_demo/world.b8world")), false);
});
