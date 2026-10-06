const Module = require("node:module");

const source = 'version = 1\nwidth = 1\nheight = 1\nA1\n\n[node_state]\nnext_id = 3\n\n' +
  '[[node_state.nodes]]\nid = "N1"\nname = "Player"\nx = 32\ny = 40\nenabled = true\nscript = "player.b8"\n\n' +
  '[node_state.nodes.collider]\nenabled = true\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n\n' +
  '[[node_state.nodes]]\nid = "N2"\nname = "Other"\nx = 8\ny = 16\nenabled = true\n';
const report = { tilesheets: [{ group: "A", file: "tilesheet.png", width: 16, height: 8, columns: 2, rows: 1,
  palette: Array(16).fill(0), cells: [0, 1].map((x) => ({ id: `A${x + 1}`, x, y: 0, solid: false, pixels: Array(64).fill(1) })) }] };

// Only platform inspection is stubbed; all editor/provider/model code is production code.
function loadProvider(vscode, inspection = report) {
  const originalLoad = Module._load;
  const modulePath = require.resolve("../out/map_editor");
  const cached = require.cache[modulePath];
  delete require.cache[modulePath];
  Module._load = function (request, parent, isMain) {
    if (request === "vscode") return vscode;
    if (request === "./native_cli") return {
      nativeFilePath: (uri) => decodeURIComponent(uri.replace(/^file:\/\//, "")),
      runNativeBit8Cli: async (_project, args) => args[1] === "tilesheets" ? JSON.stringify(inspection) : "",
    };
    return originalLoad.call(this, request, parent, isMain);
  };
  try { return require(modulePath).Bit8MapEditorProvider; }
  finally {
    Module._load = originalLoad;
    delete require.cache[modulePath];
    if (cached) require.cache[modulePath] = cached;
  }
}

async function waitFor(predicate) {
  const deadline = Date.now() + 5000;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error("Timed out waiting for Node Visual document edit");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}
module.exports = { source, report, loadProvider, waitFor };
