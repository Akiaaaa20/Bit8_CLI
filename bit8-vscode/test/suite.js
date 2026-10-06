const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");
const vscode = require("vscode");
const { NativeHostRuntimeBackend } = require("../out/native_host_runtime_backend");
const { parseHostMessage } = require("../out/runtime_protocol");
const { HeldButtonState } = require("../out/input_state");
const { parseMapDocument, replaceMapCell } = require("../out/map_editor_model");

async function activateBit8() {
  const extension = vscode.extensions.getExtension("bit8.bit8-vscode");
  assert.ok(extension, "the Bit8 extension should be loaded in the development host");
  await extension.activate();
}

async function waitForCompletion(uri, position, expectedLabel) {
  const deadline = Date.now() + 5000;
  let result;
  while (Date.now() < deadline) {
    result = await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider",
      uri,
      position,
    );
    if (result?.items.some((item) => item.label === expectedLabel)) return result;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.fail(`Language Service did not provide ${expectedLabel} completion in time`);
}

async function waitForHover(uri, position, expectedText) {
  const deadline = Date.now() + 5000;
  let result;
  while (Date.now() < deadline) {
    result = await vscode.commands.executeCommand("vscode.executeHoverProvider", uri, position);
    const text = (result ?? []).flatMap((hover) => hover.contents).map((item) =>
      typeof item === "string" ? item : item.value,
    ).join(" ");
    if (text.includes(expectedText)) return { result, text };
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  assert.fail(`Language Service did not provide hover text ${expectedText} in time`);
}

suite("Bit8 VS Code extension", () => {
  test("Node Visual provider edits use real VS Code dirty/save/revert and undo/redo", async function () {
    this.timeout(20000);
    const { source, loadProvider, waitFor } = require("./node_visual_fixture");
    const project = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-node-visual-"));
    const mapPath = path.join(project, "world.b8map");
    fs.writeFileSync(mapPath, source);
    const scriptPath = path.join(project, "player.b8");
    fs.writeFileSync(scriptPath, "func draw() spr(A1,self.x,self.y) end\n");
    fs.writeFileSync(path.join(project, "bit8.assets.toml"), "version = 1\n");
    const uri = vscode.Uri.file(mapPath);
    const folder = { uri: vscode.Uri.file(project), name: "Visual test", index: 0 };
    const proxy = {
      Uri: vscode.Uri, Range: vscode.Range, WorkspaceEdit: vscode.WorkspaceEdit,
      RelativePattern: vscode.RelativePattern, commands: vscode.commands, window: vscode.window,
      workspace: {
        fs: vscode.workspace.fs, applyEdit: (edit) => vscode.workspace.applyEdit(edit),
        onDidChangeTextDocument: (listener) => vscode.workspace.onDidChangeTextDocument(listener),
        onDidSaveTextDocument: (listener) => vscode.workspace.onDidSaveTextDocument(listener),
        get textDocuments() { return vscode.workspace.textDocuments; },
        getWorkspaceFolder: () => folder, findFiles: async () => [uri],
      },
    };
    const document = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(document);
    let receive, dispose;
    const errors = [], models = [];
    const panel = { webview: { cspSource: "vscode-webview-resource:",
      onDidReceiveMessage(listener) { receive = listener; return { dispose() {} }; },
      postMessage(message) {
        if (message.type === "error") errors.push(message.message);
        if (message.type === "model") models.push(message.model);
        return Promise.resolve(true);
      },
    }, onDidDispose(listener) { dispose = listener; } };
    const node = (id = "N1") => parseMapDocument(document.getText(), new Set(["A1", "A2"])).nodes.find((entry) => entry.id === id);
    const change = async (visual) => {
      receive({ command: "setNodeVisual", id: "N1", visual });
      await waitFor(() => node().visual === (visual ?? undefined));
    };
    try {
      await new (loadProvider(proxy))().resolveCustomTextEditor(document, panel, {});
      await waitFor(() => models.at(-1)?.nodes[0].scriptVisual === "A1");
      assert.equal(document.getText(), source, "script hints never persist a visual field");
      await change("A1");
      assert.equal(document.isDirty, true);
      assert.match(document.getText(), /visual = "A1"/);
      await vscode.commands.executeCommand("undo");
      await waitFor(() => node().visual === undefined);
      await vscode.commands.executeCommand("redo");
      await waitFor(() => node().visual === "A1");
      assert.equal(await document.save(), true);
      assert.equal(document.isDirty, false);
      assert.match(fs.readFileSync(mapPath, "utf8"), /visual = "A1"/);
      await change("A2");
      assert.equal(document.isDirty, true);
      await vscode.commands.executeCommand("workbench.action.files.revert");
      await waitFor(() => !document.isDirty && node().visual === "A1");
      await change(null);
      await document.save();
      assert.equal(fs.readFileSync(mapPath, "utf8"), source, "clear must preserve all script/collider/tile bytes");
      assert.equal(node("N2").visual, undefined);
      const scriptDocument = await vscode.workspace.openTextDocument(vscode.Uri.file(scriptPath));
      await vscode.window.showTextDocument(scriptDocument);
      const scriptEdit = new vscode.WorkspaceEdit();
      scriptEdit.replace(scriptDocument.uri, new vscode.Range(scriptDocument.positionAt(0), scriptDocument.positionAt(scriptDocument.getText().length)),
        "func draw() spr(A2,self.x,self.y) end\n");
      assert.equal(await vscode.workspace.applyEdit(scriptEdit), true);
      await waitFor(() => models.at(-1)?.nodes[0].scriptVisual === "A2");
      const countBeforeSave = models.length;
      await scriptDocument.save();
      await waitFor(() => models.length > countBeforeSave);
      assert.equal(document.isDirty, false);
      assert.equal(document.getText(), source, "live script edits and saves do not rewrite or dirty the map");
      assert.equal(fs.readFileSync(mapPath, "utf8"), source);
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
      await vscode.window.showTextDocument(document);
      assert.deepEqual(errors, []);
    } finally {
      dispose?.();
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
      fs.rmSync(project, { recursive: true, force: true });
    }
  });
  test("Node Sprite provider edits use real VS Code dirty/save/revert and undo/redo", async function () {
    this.timeout(20000);
    const { source, report, loadProvider, waitFor } = require("./node_visual_fixture");
    const project = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-node-visual-"));
    const mapPath = path.join(project, "world.b8map");
    fs.writeFileSync(mapPath, source);
    const scriptPath = path.join(project, "player.b8");
    fs.writeFileSync(scriptPath, "func draw() spr(A1,self.x,self.y) end\n");
    fs.writeFileSync(path.join(project, "bit8.assets.toml"), "version = 1\n");
    const uri = vscode.Uri.file(mapPath);
    const folder = { uri: vscode.Uri.file(project), name: "Visual test", index: 0 };
    const proxy = {
      Uri: vscode.Uri, Range: vscode.Range, WorkspaceEdit: vscode.WorkspaceEdit,
      RelativePattern: vscode.RelativePattern, commands: vscode.commands, window: vscode.window,
      workspace: {
        fs: vscode.workspace.fs, applyEdit: (edit) => vscode.workspace.applyEdit(edit),
        onDidChangeTextDocument: (listener) => vscode.workspace.onDidChangeTextDocument(listener),
        onDidSaveTextDocument: (listener) => vscode.workspace.onDidSaveTextDocument(listener),
        get textDocuments() { return vscode.workspace.textDocuments; },
        getWorkspaceFolder: () => folder, findFiles: async () => [uri],
      },
    };
    const document = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(document);
    let receive, dispose;
    const errors = [], models = [];
    const panel = { webview: { cspSource: "vscode-webview-resource:",
      onDidReceiveMessage(listener) { receive = listener; return { dispose() {} }; },
      postMessage(message) {
        if (message.type === "error") errors.push(message.message);
        if (message.type === "model") models.push(message.model);
        return Promise.resolve(true);
      },
    }, onDidDispose(listener) { dispose = listener; } };
    const node = (id = "N1") => parseMapDocument(document.getText(), new Set(["A1", "A2"])).nodes.find((entry) => entry.id === id);
    const change = async (sprite) => {
      receive({ command: "setNodeSprite", id: "N1", sprite });
      await waitFor(() => node().sprite === (sprite ?? undefined));
    };
    try {
      await new (loadProvider(proxy, { ...report, sprites: [{name:"Player",preview:"A1"},{name:"Slime",preview:"A2"}] }))().resolveCustomTextEditor(document, panel, {});
      await waitFor(() => models.at(-1)?.nodes[0].scriptVisual === "A1");
      assert.equal(document.getText(), source, "script hints never persist a visual field");
      await change("Player");
      assert.equal(document.isDirty, true);
      assert.match(document.getText(), /sprite = "Player"/);
      await vscode.commands.executeCommand("undo");
      await waitFor(() => node().sprite === undefined);
      await vscode.commands.executeCommand("redo");
      await waitFor(() => node().sprite === "Player");
      assert.equal(await document.save(), true);
      assert.equal(document.isDirty, false);
      assert.match(fs.readFileSync(mapPath, "utf8"), /sprite = "Player"/);
      await change("Slime");
      assert.equal(document.isDirty, true);
      await vscode.commands.executeCommand("workbench.action.files.revert");
      await waitFor(() => !document.isDirty && node().sprite === "Player");
      await change(null);
      await document.save();
      assert.equal(fs.readFileSync(mapPath, "utf8"), source, "clear must preserve all script/collider/tile bytes");
      assert.equal(node("N2").sprite, undefined);
      const scriptDocument = await vscode.workspace.openTextDocument(vscode.Uri.file(scriptPath));
      await vscode.window.showTextDocument(scriptDocument);
      const scriptEdit = new vscode.WorkspaceEdit();
      scriptEdit.replace(scriptDocument.uri, new vscode.Range(scriptDocument.positionAt(0), scriptDocument.positionAt(scriptDocument.getText().length)),
        "func draw() spr(A2,self.x,self.y) end\n");
      assert.equal(await vscode.workspace.applyEdit(scriptEdit), true);
      await waitFor(() => models.at(-1)?.nodes[0].scriptVisual === "A2");
      const countBeforeSave = models.length;
      await scriptDocument.save();
      await waitFor(() => models.length > countBeforeSave);
      assert.equal(document.isDirty, false);
      assert.equal(document.getText(), source, "live script edits and saves do not rewrite or dirty the map");
      assert.equal(fs.readFileSync(mapPath, "utf8"), source);
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
      await vscode.window.showTextDocument(document);
      assert.deepEqual(errors, []);
    } finally {
      dispose?.();
      await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
      fs.rmSync(project, { recursive: true, force: true });
    }
  });
  test("Sprite definition navigation opens the project file read-only in a real editor", async function () {
    this.timeout(20000);
    const { source, report, loadProvider, waitFor } = require('./node_visual_fixture');
    const project = fs.mkdtempSync(path.join(os.tmpdir(), 'bit8-sprite-navigation-'));
    const original = source.replace('name = "Player"', 'name = "Player"\nsprite = "Player"');
    const definitions = 'version=1\n[sprite.Player]\npreview="A1"\n';
    const files = { 'world.b8map':original, 'bit8.sprites.toml':definitions, 'bit8.assets.toml':'version=1\n', 'player.b8':'func draw() end\n' };
    for (const [name, text] of Object.entries(files)) fs.writeFileSync(path.join(project,name),text);
    const uri = vscode.Uri.file(path.join(project,'world.b8map'));
    const folder = { uri:vscode.Uri.file(project), name:'Navigation test', index:0 };
    const proxy = {
      Uri:vscode.Uri, Range:vscode.Range, WorkspaceEdit:vscode.WorkspaceEdit, RelativePattern:vscode.RelativePattern, window:vscode.window,
      workspace: {
        fs:vscode.workspace.fs, openTextDocument:(uri)=>vscode.workspace.openTextDocument(uri),
        onDidChangeTextDocument:(listener)=>vscode.workspace.onDidChangeTextDocument(listener),
        onDidSaveTextDocument:(listener)=>vscode.workspace.onDidSaveTextDocument(listener),
        get textDocuments(){ return vscode.workspace.textDocuments; },
        getWorkspaceFolder:()=>folder, findFiles:async()=>[uri],
        applyEdit:()=>{ throw new Error('Navigation must never edit'); }
      }
    };
    let receive, dispose;
    const errors=[];
    const panel={webview:{cspSource:'vscode-webview-resource:',onDidReceiveMessage(listener){receive=listener;return{dispose(){}};},
      postMessage(message){if(message.type==='error') errors.push(message.message);return Promise.resolve(true);}},onDidDispose(listener){dispose=listener;}};
    try {
      const document=await vscode.workspace.openTextDocument(uri);
      await vscode.window.showTextDocument(document);
      await new (loadProvider(proxy,{...report,sprites:[{name:'Player',preview:'A1',animations:[]}]}))().resolveCustomTextEditor(document,panel,{});
      receive({command:'openSpriteDefinition',id:'N1'});
      await waitFor(()=>vscode.window.activeTextEditor?.document.uri.fsPath===path.join(project,'bit8.sprites.toml'));
      assert.equal(vscode.window.activeTextEditor.document.getText(),definitions);
      assert.equal(vscode.window.activeTextEditor.document.isDirty,false);
      assert.equal(document.isDirty,false);
      for(const [name,text] of Object.entries(files)) assert.equal(fs.readFileSync(path.join(project,name),'utf8'),text);
      assert.deepEqual(errors,[]);
    } finally {
      dispose?.();
      await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
      fs.rmSync(project,{recursive:true,force:true});
    }
  });
  test("registers Bit8 commands and the native .b8 language mode", async () => {
    await activateBit8();
    const extension = vscode.extensions.getExtension("bit8.bit8-vscode");
    const commands = await vscode.commands.getCommands(true);
    for (const command of [
      "bit8.runGame",
      "bit8.stopGame",
      "bit8.restartGame",
      "bit8.insertGameLoop",
      "bit8.useBit8LanguageMode",
    ]) {
      assert.ok(commands.includes(command), `${command} should be registered`);
    }
    const runButton = extension.packageJSON.contributes.menus["editor/title"].find(
      (item) => item.command === "bit8.runGame",
    );
    assert.ok(runButton, "Run Game should be contributed to the editor title");
    assert.equal(runButton.icon, "$(play)");
    assert.equal(runButton.when, "editorLangId == lua || editorLangId == bit8");

    const manifest = extension.packageJSON.contributes;
    assert.ok(!manifest.configurationDefaults, "the extension must not add global language defaults");
    const bit8Language = manifest.languages.find((language) => language.id === "bit8");
    assert.ok(bit8Language, "the Bit8 language mode should be contributed");
    assert.deepEqual(bit8Language.extensions, [".b8"], "only .b8 files should naturally use Bit8 mode");
    assert.ok(
      !fs.existsSync(path.join(extension.extensionPath, "..", ".vscode", "settings.json")),
      "the old repository-wide Lua association workaround should be removed",
    );
    const grammar = manifest.grammars.find((entry) => entry.language === "bit8");
    assert.equal(grammar.scopeName, "source.bit8");
    const grammarSource = JSON.parse(
      fs.readFileSync(path.join(extension.extensionPath, grammar.path), "utf8"),
    );
    assert.ok(grammarSource.patterns.some((pattern) => pattern.name === "keyword.control.function.bit8"));

    const bit8File = await vscode.workspace.openTextDocument(
      path.join(extension.extensionPath, "test/fixtures/example.b8"),
    );
    assert.equal(bit8File.languageId, "bit8", "a .b8 file should automatically use Bit8 mode");

    const associationsBefore = vscode.workspace.getConfiguration("files").inspect("associations");
    const ordinaryLua = await vscode.workspace.openTextDocument({ language: "lua", content: "func draw()" });
    assert.equal(ordinaryLua.languageId, "lua", "ordinary Lua documents must keep their existing mode");
    const editor = await vscode.window.showTextDocument(ordinaryLua);
    await vscode.commands.executeCommand("bit8.useBit8LanguageMode");
    assert.equal(editor.document.languageId, "bit8", "the explicit command should switch only the active document");
    assert.deepEqual(
      vscode.workspace.getConfiguration("files").inspect("associations"),
      associationsBefore,
      "switching one document must not modify file associations or settings",
    );
  });

  test("keeps PNGs in host editors and exposes explicit tilesheet registration commands", async () => {
    await activateBit8();
    const extension = vscode.extensions.getExtension("bit8.bit8-vscode");
    assert.ok(!extension.packageJSON.contributes.customEditors.some((item) => item.selector.some((selector) => selector.filenamePattern.endsWith(".png"))));
    const commands = await vscode.commands.getCommands(true);
    assert.ok(commands.includes("bit8.addTilesheet"));
    assert.ok(commands.includes("bit8.applyTilesheetName"));
    assert.equal(extension.packageJSON.contributes.menus["explorer/context"].length, 2);
  });

  test("contributes an optional text-backed .b8map editor without disturbing other editors", async () => {
    await activateBit8();
    const extension = vscode.extensions.getExtension("bit8.bit8-vscode");
    const editor = extension.packageJSON.contributes.customEditors.find(
      (item) => item.viewType === "bit8.mapEditor",
    );
    assert.ok(editor, "Bit8 Map Editor should be contributed");
    assert.equal(editor.displayName, "Bit8 Map Editor");
    assert.equal(editor.priority, "option");
    assert.deepEqual(editor.selector, [{ filenamePattern: "**/*.b8map" }]);
    assert.ok(!extension.packageJSON.contributes.customEditors.some((item) => item.viewType.includes("world")));
    const commands = await vscode.commands.getCommands(true);
    assert.ok(fs.existsSync(path.join(extension.extensionPath, "out/map_editor.js")));
    const activation = fs.readFileSync(path.join(extension.extensionPath, "out/extension.js"), "utf8");
    assert.ok(activation.includes("bit8.mapEditor"), "the compiled extension should register the contributed viewType");
  });

  test("map editor model preserves stable cells and paints/erases without changing dimensions", () => {
    const ids = new Set(["A1", "A4", "B1"]);
    const source = "version = 1\nwidth = 3\nheight = 2\n\nA1 -- A4\nB1 A1 --\n";
    const model = parseMapDocument(source, ids);
    assert.equal(model.width, 3);
    assert.equal(model.height, 2);
    assert.deepEqual(model.cells, [["A1", "--", "A4"], ["B1", "A1", "--"]]);

    const painted = replaceMapCell(source, 1, 0, "B1", ids);
    const erased = replaceMapCell(painted, 0, 1, null, ids);
    const updated = parseMapDocument(erased, ids);
    assert.equal(updated.width, 3);
    assert.equal(updated.height, 2);
    assert.deepEqual(updated.cells, [["A1", "B1", "A4"], ["--", "A1", "--"]]);
  });

  test("map editor rejects malformed and unregistered tile references without rewriting source", () => {
    const ids = new Set(["A1", "A4", "B1"]);
    const malformed = "version = 1\nwidth = 3\nheight = 1\nA1 A4\n";
    assert.throws(() => parseMapDocument(malformed, ids), /row 1 has 2 cells; expected 3/);
    const unknown = "version = 1\nwidth = 1\nheight = 1\nB99\n";
    assert.throws(() => parseMapDocument(unknown, ids), /B99/);
    const unsupported = "version = 2\nwidth = 1\nheight = 1\nA1\n";
    assert.throws(() => replaceMapCell(unsupported, 0, 0, "A1", ids), /unsupported map version/);
    assert.equal(unsupported, "version = 2\nwidth = 1\nheight = 1\nA1\n");
  });

  test("painted map serialization remains valid to the existing Rust Map Core", function () {
    const extension = vscode.extensions.getExtension("bit8.bit8-vscode");
    const repository = path.resolve(extension.extensionPath, "..");
    const bit8 = path.join(repository, "target", "debug", process.platform === "win32" ? "bit8.exe" : "bit8");
    if (!fs.existsSync(bit8)) this.skip();

    const fixtureProject = path.join(repository, "games", "tilesheet_demo");
    const project = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-map-editor-rust-"));
    try {
      fs.writeFileSync(path.join(project, "bit8.assets.toml"),
        'version = 1\n\n[[tilesheets]]\ngroup = "A"\nfile = "tilesheet (A).png"\n');
      fs.copyFileSync(path.join(fixtureProject, "tilesheet (A).png"), path.join(project, "tilesheet (A).png"));
      const ids = new Set(["A1", "A2", "A3", "A4"]);
      const rows = Array.from({ length: 8 }, () => Array(8).fill("A1").join(" "));
      const source = `version = 1\nwidth = 8\nheight = 8\n\n${rows.join("\n")}\n`;
      const painted = replaceMapCell(source, 2, 2, "A4", ids);
      const erased = replaceMapCell(painted, 1, 1, null, ids);
      const mapPath = path.join(project, "world.b8map");
      fs.writeFileSync(mapPath, erased);
      const report = execFileSync(bit8, ["inspect", "map", project, mapPath], { encoding: "utf8" });
      assert.match(report, /Size: 8 × 8 tiles/);
      assert.match(report, /A4/);
    } finally {
      fs.rmSync(project, { recursive: true, force: true });
    }
  });

  test("gets Bit8 API completion and hover from the Language Service", async () => {
    await activateBit8();
    const document = await vscode.workspace.openTextDocument({ language: "bit8", content: "rec" });
    await vscode.window.showTextDocument(document);
    const completions = await waitForCompletion(document.uri, new vscode.Position(0, 3), "rect");
    const labels = completions.items.map((item) => item.label);
    assert.ok(labels.includes("rect"), "rect should be suggested for `rec`");
    assert.ok(labels.includes("rectfill"), "rectfill should be suggested for `rec`");

    const buttonDocument = await vscode.workspace.openTextDocument({ language: "bit8", content: "btn" });
    await vscode.window.showTextDocument(buttonDocument);
    const buttonCompletions = await waitForCompletion(
      buttonDocument.uri,
      new vscode.Position(0, 3),
      "btnp",
    );
    const buttonLabels = buttonCompletions.items.map((item) => item.label);
    assert.ok(buttonLabels.includes("btn"));
    assert.ok(buttonLabels.includes("btnp"));

    const hoverDocument = await vscode.workspace.openTextDocument({ language: "bit8", content: "btnp(A)" });
    await vscode.window.showTextDocument(hoverDocument);
    const { result: hovers, text: hoverText } = await waitForHover(
      hoverDocument.uri,
      new vscode.Position(0, 1),
      "press-transition frame",
    );
    assert.ok(hovers.length > 0, "btnp should have hover documentation from the Language Service");
    assert.match(hoverText, /press-transition frame/);

    const rectangleHoverDocument = await vscode.workspace.openTextDocument({
      language: "bit8",
      content: "rectfill(1, 1, 8, 4, 2)",
    });
    await vscode.window.showTextDocument(rectangleHoverDocument);
    const { text: rectangleHoverText } = await waitForHover(
      rectangleHoverDocument.uri,
      new vscode.Position(0, 2),
      "rectfill(x, y, width, height, color)",
    );
    assert.match(rectangleHoverText, /rectfill\(x, y, width, height, color\)/);
    assert.match(rectangleHoverText, /pixel counts/);

    const unknown = await vscode.workspace.openTextDocument({ language: "bit8", content: "playerX" });
    await vscode.window.showTextDocument(unknown);
    const unknownHovers = await vscode.commands.executeCommand(
      "vscode.executeHoverProvider",
      unknown.uri,
      new vscode.Position(0, 2),
    );
    assert.equal(unknownHovers.length, 0, "unknown identifiers should not get fake Bit8 hover docs");

    const ordinaryLua = await vscode.workspace.openTextDocument({ language: "lua", content: "cls" });
    const luaCompletions = await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider",
      ordinaryLua.uri,
      new vscode.Position(0, 3),
    );
    assert.ok(
      !luaCompletions.items.some((item) => item.label === "cls"),
      "ordinary Lua documents should not receive Bit8 API completions",
    );
  });

  test("Bit8 Language Service diagnoses malformed structure and accepts current source", async () => {
    await activateBit8();
    const extension = vscode.extensions.getExtension("bit8.bit8-vscode");
    const document = await vscode.workspace.openTextDocument(
      path.join(extension.extensionPath, "test/fixtures/syntax_error.b8"),
    );
    assert.equal(document.languageId, "bit8", ".b8 diagnostics should use the Bit8 language ID");
    await vscode.window.showTextDocument(document);

    const deadline = Date.now() + 10000;
    let diagnostics = [];
    while (Date.now() < deadline) {
      diagnostics = vscode.languages.getDiagnostics(document.uri).filter((item) => item.source === "bit8");
      if (diagnostics.length > 0) break;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    assert.ok(diagnostics.length > 0, "missing `end` should produce a Bit8 syntax diagnostic");
    assert.ok(
      diagnostics.some((item) => item.message.includes("syntax_error.b8")),
      "syntax diagnostics should retain the .b8 filename",
    );

    const editor = vscode.window.activeTextEditor;
    assert.ok(editor);
    await editor.edit((edit) => edit.replace(
      new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)),
      [
        "func init()",
        "    playerX = 32",
        "end",
        "",
        "func update()",
        "    if btn(LEFT) and playerX > 0 then",
        "        playerX = playerX - 1",
        "    end",
        "    if btn(RIGHT) and playerX < 63 then",
        "        playerX = playerX + 1",
        "    end",
        "end",
        "",
        "func draw()",
        "    cls(0)",
        "    pix(playerX, 32, 1)",
        "end",
      ].join("\n"),
    ));

    const validDeadline = Date.now() + 5000;
    while (Date.now() < validDeadline) {
      diagnostics = vscode.languages.getDiagnostics(document.uri).filter((item) => item.source === "bit8");
      if (diagnostics.length === 0) break;
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    assert.equal(diagnostics.length, 0, "the current Bit8 sample should be accepted");
  });

  test("offers the Basic Game template and inserts editable runtime-supported source", async () => {
    const document = await vscode.workspace.openTextDocument({
      language: "bit8",
      content: "keep\nTemplate\nafter",
    });
    const position = new vscode.Position(1, "Template".length);
    const completions = await vscode.commands.executeCommand(
      "vscode.executeCompletionItemProvider",
      document.uri,
      position,
      "e",
    );
    const template = completions.items.find(
      (item) => item.label === "Bit8 Template: Basic Game",
    );
    assert.ok(template, "the Basic Game template completion should be available");
    assert.equal(template.filterText, "Template");

    const editor = await vscode.window.showTextDocument(document);
    const source = typeof template.insertText === "string"
      ? template.insertText
      : template.insertText.value;
    const range = new vscode.Range(1, 0, 1, "Template".length);
    await editor.edit((edit) => edit.replace(range, source));

    assert.ok(document.getText().startsWith("keep\nfunc init()"));
    assert.ok(document.getText().endsWith("\nafter"));
    assert.match(document.getText(), /func update\(\)[\s\S]*btn\(LEFT\)/);
    assert.match(document.getText(), /func draw\(\)[\s\S]*cls\(0\)[\s\S]*pix\(playerX, 32, 1\)/);

    const calls = Array.from(source.matchAll(/\b([A-Za-z_][A-Za-z0-9_]*)\s*\(/g), (match) => match[1]);
    const supportedCalls = new Set(["init", "update", "draw", "btn", "cls", "pix"]);
    assert.ok(calls.length > 0);
    assert.ok(calls.every((name) => supportedCalls.has(name)), "the template must use only supported Bit8 APIs");
    assert.match(source, /\bLEFT\b/);
    assert.match(source, /\bRIGHT\b/);
  });

  test("inserts the game loop at the cursor without replacing surrounding text", async () => {
    const document = await vscode.workspace.openTextDocument({ language: "lua", content: "keep" });
    const editor = await vscode.window.showTextDocument(document);
    editor.selection = new vscode.Selection(new vscode.Position(0, 2), new vscode.Position(0, 2));

    await vscode.commands.executeCommand("bit8.insertGameLoop");

    const content = document.getText();
    assert.ok(content.startsWith("kefunc init()"));
    assert.ok(content.endsWith("ep"));
    assert.ok(content.includes("func update()"));
    assert.ok(content.includes("func draw()\n    cls(0)"));
  });

  test("host protocol parsing and held-key mapping stay minimal and strict", () => {
    const ready = parseHostMessage('{"type":"ready","width":64,"height":64}');
    assert.deepEqual(ready, { type: "ready", width: 64, height: 64 });
    assert.throws(() => parseHostMessage('{"type":"frame","width":64,"height":64,"seq":1,"pixels":[]}'), /invalid 64x64/);
    assert.throws(() => parseHostMessage('{"type":"future"}'), /unknown or invalid/);

    const held = new HeldButtonState();
    assert.deepEqual(held.buttonsForKeys(["ArrowRight", "KeyZ", "KeyZ", "F1"]), ["RIGHT", "A"]);
    assert.deepEqual(held.buttonsForKeys([]), []);
  });

  test("native host backend handles ready, one frame at a time, and graceful cleanup", async () => {
    const bin = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-vscode-bin-"));
    const project = vscode.workspace.workspaceFolders[0].uri.fsPath;
    const executable = path.join(bin, "bit8");
    fs.copyFileSync(path.join(__dirname, "fixtures", "mock-bit8.js"), executable);
    fs.chmodSync(executable, 0o755);
    const output = { appendLine: () => {} };
    process.env.BIT8_TEST_FRAME_DELAY = "100";

    try {
      const client = await NativeHostRuntimeBackend.start(require("node:url").pathToFileURL(project).toString(), output, executable);
      const firstFrame = client.step(["RIGHT"]);
      await assert.rejects(client.step(["RIGHT", "A"]), /already in flight/);
      const frame = await firstFrame;
      assert.equal(frame.type, "frame");
      assert.equal(frame.seq, 1);
      assert.equal(frame.pixels.length, 4096);
      await client.stop();
      assert.equal(client.running, false, "stop should wait for the child host to exit");
    } finally {
      delete process.env.BIT8_TEST_FRAME_DELAY;
      fs.rmSync(bin, { recursive: true, force: true });
    }
  });

  test("Run and Restart launch bit8 host for the active workspace project", async () => {
    const bin = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-vscode-bin-"));
    const project = vscode.workspace.workspaceFolders[0].uri.fsPath;
    const executable = path.join(bin, "bit8");
    const argsLog = path.join(bin, "args.txt");
    fs.copyFileSync(path.join(__dirname, "fixtures", "mock-bit8.js"), executable);
    fs.chmodSync(executable, 0o755);
    process.env.PATH = `${bin}${path.delimiter}${process.env.PATH ?? ""}`;
    process.env.BIT8_TEST_ARGS = argsLog;

    try {
      const projectDocument = await vscode.workspace.openTextDocument(
        path.join(project, "test/fixtures/main.lua"),
      );
      await vscode.window.showTextDocument(projectDocument);
      await vscode.commands.executeCommand("bit8.runGame");
      const deadline = Date.now() + 5000;
      while ((!fs.existsSync(argsLog) || fs.readFileSync(argsLog, "utf8").trim().split("\n").length < 2) && Date.now() < deadline) {
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      assert.deepEqual(fs.readFileSync(argsLog, "utf8").trim().split("\n"), ["host", project]);

      await vscode.commands.executeCommand("bit8.restartGame");
      const restartDeadline = Date.now() + 3000;
      while (fs.readFileSync(argsLog, "utf8").trim().split("\n").length < 4 && Date.now() < restartDeadline) {
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      assert.deepEqual(fs.readFileSync(argsLog, "utf8").trim().split("\n"), ["host", project, "host", project]);
      await vscode.commands.executeCommand("bit8.stopGame");
    } finally {
      await vscode.commands.executeCommand("bit8.stopGame");
      delete process.env.BIT8_TEST_ARGS;
      fs.rmSync(bin, { recursive: true, force: true });
    }
  });
});
