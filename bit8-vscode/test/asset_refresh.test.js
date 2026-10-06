const assert = require("node:assert/strict");
const Module = require("node:module");
const path = require("node:path");
const { test } = require("node:test");
const {
  notifyAssetRegistryChanged,
  onAssetRegistryChanged,
  prepareAssetSnapshot,
} = require("../out/asset_refresh");

function sheet(group, ids) {
  return {
    group,
    file: `${group}.png`,
    width: 8,
    height: 8,
    columns: 1,
    rows: 1,
    palette: Array(16).fill(0),
    cells: ids.map((id) => ({ id, x: 0, y: 0, pixels: Array(64).fill(0) })),
  };
}

test("asset registry notifications refresh every open map editor for that project only", () => {
  let first = 0;
  let second = 0;
  let otherProject = 0;
  const firstSubscription = onAssetRegistryChanged("file:///project", () => { first++; });
  const secondSubscription = onAssetRegistryChanged("file:///project", () => { second++; });
  const otherSubscription = onAssetRegistryChanged("file:///other", () => { otherProject++; });

  notifyAssetRegistryChanged("file:///project");
  assert.deepEqual([first, second, otherProject], [1, 1, 0]);
  firstSubscription.dispose();
  notifyAssetRegistryChanged("file:///project");
  assert.deepEqual([first, second, otherProject], [1, 2, 0]);
  secondSubscription.dispose();
  otherSubscription.dispose();
  notifyAssetRegistryChanged("file:///project");
  assert.deepEqual([first, second, otherProject], [1, 2, 0]);
});

test("asset refresh prepares a complete snapshot and rejects removal of an in-use ID", () => {
  const oldSnapshot = prepareAssetSnapshot(
    [sheet("A", ["A1"])],
    "version = 1\nwidth = 1\nheight = 1\nA1\n",
  );
  const addedSnapshot = prepareAssetSnapshot(
    [sheet("A", ["A1"]), sheet("B", ["B1"])],
    "version = 1\nwidth = 1\nheight = 1\nB1\n",
  );
  assert.deepEqual([...addedSnapshot.assetIds], ["A1", "B1"]);
  assert.deepEqual(addedSnapshot.map.cells, [["B1"]], "the refresh model is built from the current edited document text");

  assert.throws(() => prepareAssetSnapshot(
    [sheet("B", ["B1"])],
    "version = 1\nwidth = 1\nheight = 1\nA1\n",
  ), /A1/);
  assert.deepEqual([...oldSnapshot.assetIds], ["A1"], "the prior usable snapshot remains untouched on validation failure");
  assert.deepEqual(oldSnapshot.map.cells, [["A1"]]);
});

test("asset refresh carries updated Solid metadata with stable cell identity", () => {
  const updatedSheet = sheet("B", ["B6"]);
  updatedSheet.cells[0].solid = true;
  const snapshot = prepareAssetSnapshot([updatedSheet], "version = 1\nwidth = 1\nheight = 1\nB6\n");
  assert.equal(snapshot.tilesheets[0].cells[0].id, "B6");
  assert.equal(snapshot.tilesheets[0].cells[0].solid, true);
  assert.deepEqual(snapshot.map.cells, [["B6"]]);
});

test("successful Add to Bit8 command notifies all open maps after the CLI registration succeeds", async () => {
  const originalLoad = Module._load;
  const commands = new Map();
  const calls = [];
  const projectUri = { scheme: "file", fsPath: "/project", path: "/project", toString: () => "file:///project" };
  const targetUri = { scheme: "file", fsPath: "/project/new.png", path: "/project/new.png", toString: () => "file:///project/new.png" };
  const vscode = {
    commands: {
      registerCommand(name, callback) { commands.set(name, callback); return { dispose() {} }; },
      async executeCommand() {},
    },
    workspace: { getWorkspaceFolder: () => ({ uri: projectUri }) },
    Uri: { joinPath: (_root, ...parts) => ({ toString: () => `file:///project/${parts.join("/")}` }) },
    window: { showInformationMessage() {}, showErrorMessage() {}, tabGroups: { activeTabGroup: { activeTab: undefined } } },
  };
  const nativeCli = {
    nativeFilePath: (uri) => uri.includes("new.png") ? "/project/new.png" : "/project",
    runNativeBit8Cli: async (...args) => { calls.push(args); return JSON.stringify({ group: "B", file: "new (B).png" }); },
  };
  let firstRefresh = 0;
  let secondRefresh = 0;
  const first = onAssetRegistryChanged(projectUri.toString(), () => { firstRefresh++; });
  const second = onAssetRegistryChanged(projectUri.toString(), () => { secondRefresh++; });
  let register;
  Module._load = function (request, parent, isMain) {
    if (request === "vscode") return vscode;
    if (request === "./project_model") return { parseTilesheetInspection: () => [] };
    if (request === "./native_cli") return nativeCli;
    if (request === "./asset_refresh") return originalLoad.call(this, request, parent, isMain);
    return originalLoad.call(this, request, parent, isMain);
  };
  try {
    const modulePath = path.resolve(__dirname, "../out/tilesheet_commands.js");
    delete require.cache[modulePath];
    const tilesheetCommands = require(modulePath);
    register = tilesheetCommands.registerTilesheetCommands;
    register();
    await commands.get("bit8.addTilesheet")(targetUri);
    await tilesheetCommands.setTilesheetCellSolid(projectUri, "B6", true);
  } finally {
    Module._load = originalLoad;
    first.dispose();
    second.dispose();
  }
  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0][1], ["tilesheet", "add", "/project", "/project/new.png", "--json"]);
  assert.deepEqual(calls[1][1], ["tilesheet", "solid", "/project", "B6", "true"]);
  assert.deepEqual([firstRefresh, secondRefresh], [2, 2], "successful registration and metadata edits refresh every open Map Workspace");
});
