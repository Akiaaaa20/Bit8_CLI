const assert = require("node:assert/strict");
const Module = require("node:module");
const vm = require("node:vm");
const { test } = require("node:test");

function loadHtmlGenerator() {
  const originalLoad = Module._load;
  Module._load = function (request, parent, isMain) {
    if (request === "vscode") return {};
    return originalLoad.call(this, request, parent, isMain);
  };
  try {
    return require("../out/map_editor").mapEditorHtml;
  } finally {
    Module._load = originalLoad;
  }
}

function launchWebview(script, onPostMessage = () => {}) {
  const messages = [];
  const elements = new Map();
  const windowListeners = new Map();
  const resizeObservers = [];
  const drawCalls = [];
  const drawImageCalls = [];
  const imageDataCalls = [];
  let cameraFrames = [];
  let frameRects = [];
  let gridPath = [];
  let renderTransform = { x: 1, y: 1, offsetX: 0, offsetY: 0 };
  const gridPaths = [];
  const context2d = {
    clearRect() { frameRects = []; cameraFrames = []; },
    setTransform(x, _b, _c, y, offsetX, offsetY) { renderTransform = { x, y, offsetX, offsetY }; },
    fillRect(...args) { drawCalls.push(args); frameRects.push(args); },
    fillText(...args) { drawCalls.push(args); },
    createImageData(width, height) { return { width, height, data: new Uint8ClampedArray(width * height * 4) }; },
    putImageData(data, x, y) { imageDataCalls.push({ data, x, y }); },
    drawImage(...args) { drawImageCalls.push(args); }, beginPath() { gridPath = []; },
    moveTo(x, y) { gridPath.push({ x: x * renderTransform.x + renderTransform.offsetX, y: y * renderTransform.y + renderTransform.offsetY }); },
    lineTo(x, y) { gridPath.push({ x: x * renderTransform.x + renderTransform.offsetX, y: y * renderTransform.y + renderTransform.offsetY }); },
    stroke() {
      // Canvas context coordinates are backing pixels; CSS displays the backing
      // store scaled back to the canvas style dimensions.
      gridPaths.push(gridPath.map((point) => ({
        x: point.x / renderTransform.x,
        y: point.y / renderTransform.y,
      }))); 
    },
    strokeRect(x, y, width, height) { cameraFrames.push({ x, y, width, height, color: this.strokeStyle, lineWidth: this.lineWidth }); },
  };
  class Element {
    constructor(tagName = "div") {
      this.tagName = tagName; this.style = {}; this.hidden = false; this.value = ""; this.children = [];
      this.clientWidth = 800; this.clientHeight = 600; this.listeners = new Map(); this.classes = new Set();
      this.classList = {
        toggle: (name, force) => {
          const enabled = force === undefined ? !this.classes.has(name) : force;
          if (enabled) this.classes.add(name); else this.classes.delete(name);
          return enabled;
        },
        contains: (name) => this.classes.has(name),
      };
      this.dataset = {};
      this.capturedPointers = new Set();
    }
    addEventListener(type, listener) { this.listeners.set(type, listener); }
    fire(type, event = {}) {
      if (!event.target) event.target = this;
      if (!event.preventDefault) event.preventDefault = () => { event.defaultPrevented = true; };
      this.listeners.get(type)?.(event);
    }
    append(...items) {
      this.children.push(...items);
      if (this.tagName === "select" && !this.value && items.length) this.value = items[0].value;
    }
    replaceChildren(...items) { this.children = items; this.value = items[0]?.value || ""; }
    querySelectorAll(selector) { return selector === ".tile" ? this.children.filter((child) => child.className === "tile") : []; }
    getContext() { return context2d; }
    getBoundingClientRect() { return { left: 0, top: 0, width: this.clientWidth, height: this.clientHeight }; }
    setPointerCapture(pointerId) { this.capturedPointers.add(pointerId); }
    hasPointerCapture(pointerId) { return this.capturedPointers.has(pointerId); }
    releasePointerCapture(pointerId) { this.capturedPointers.delete(pointerId); }
    focus() { document.activeElement = this; }
    closest(selector) {
      if (selector.includes("button") && this.tagName === "button") return this;
      if (selector.includes("select") && this.tagName === "select") return this;
      if (selector.includes("input") && this.tagName === "input") return this;
      if (selector.includes("textarea") && this.tagName === "textarea") return this;
      return null;
    }
    contains(target) { return target === this || target === elements.get("mapCanvas"); }
  }
  const document = {
    listeners: new Map(),
    addEventListener(type, listener) { this.listeners.set(type, listener); },
    fire(type, event = {}) { this.listeners.get(type)?.(event); },
    getElementById(id) {
      const tag = ["tilesheetPicker", "selectedTilePicker", "mapPicker", "nodeVisualPicker"].includes(id) ? "select"
        : ["nodeName", "nodeX", "nodeY", "nodeEnabled", "nodeColliderEnabled", "colliderOffsetX", "colliderOffsetY", "colliderWidth", "colliderHeight", "tileSolid"].includes(id) ? "input"
        : ["pencil", "eraser", "panTool", "nodeTool", "addNode", "deleteNode", "chooseNodeScript", "clearNodeScript", "clearNodeVisual", "zoomIn", "zoomOut"].includes(id) ? "button" : "div";
      if (!elements.has(id)) elements.set(id, new Element(tag));
      return elements.get(id);
    },
    createElement(tagName) { return new Element(tagName); },
  };
  class ResizeObserver {
    constructor(callback) { this.callback = callback; resizeObservers.push(this); }
    observe() { this.callback(); }
    trigger() { this.callback(); }
  }
  class Image {
    constructor() { this.complete = true; this.naturalWidth = 16; }
    set src(value) { this._src = value; queueMicrotask(() => this.onload?.()); }
    get src() { return this._src; }
  }
  const window = {
    devicePixelRatio: 2,
    addEventListener(type, listener) { windowListeners.set(type, listener); },
    fire(type, event) { windowListeners.get(type)?.(event); },
  };
  const sandbox = {
    document,
    window,
    Image,
    ResizeObserver,
    acquireVsCodeApi: () => ({ postMessage: (message) => { messages.push(message); onPostMessage(message); } }),
  };
  vm.runInNewContext(script, sandbox, { filename: "bit8-map-workspace-inline.js" });
  return {
    messages,
    elements,
    document,
    drawCalls,
    drawImageCalls,
    imageDataCalls,
    get frameRects() { return frameRects; },
    get cameraFrames() { return cameraFrames; },
    get renderTransform() { return renderTransform; },
    get finalMapRect() {
      const path = gridPaths.at(-1) || [];
      const vertical = [], horizontal = [];
      for (let index = 0; index + 1 < path.length; index += 2) {
        const start = path[index], end = path[index + 1];
        if (start.x === end.x) vertical.push(start.x);
        if (start.y === end.y) horizontal.push(start.y);
      }
      return {
        x: Math.min(...vertical), y: Math.min(...horizontal),
        width: Math.max(...vertical) - Math.min(...vertical),
        height: Math.max(...horizontal) - Math.min(...horizontal),
      };
    },
    get gridSegmentBounds() {
      const path = gridPaths.at(-1) || [];
      const vertical = [], horizontal = [];
      for (let index = 0; index + 1 < path.length; index += 2) {
        const start = path[index], end = path[index + 1];
        if (start.x === end.x) vertical.push([start, end]);
        if (start.y === end.y) horizontal.push([start, end]);
      }
      return { vertical, horizontal };
    },
    resizeObservers,
    windowBlur: () => window.fire("blur", {}),
    dispatchMessage: (data) => {
      if (data.type === "model" && Array.isArray(data.tilesheets)) {
        const defaultPalette = [0x000000, 0x1d2b53, 0x7e2553, 0x008751, 0xab5236, 0x5f574f, 0xc2c3c7, 0xfff1e8,
          0xff004d, 0xffa300, 0xffec27, 0x00e436, 0x29adff, 0x83769c, 0xff77a8, 0xffccaa];
        data = { ...data, tilesheets: data.tilesheets.map((sheet) => {
          const columns = sheet.columns || Math.max(...sheet.cells.map((cell) => cell.x)) + 1;
          const rows = sheet.rows || Math.max(...sheet.cells.map((cell) => cell.y)) + 1;
          return {
            ...sheet,
            width: sheet.width || columns * 8,
            height: sheet.height || rows * 8,
            palette: sheet.palette || defaultPalette,
            cells: sheet.cells.map((cell) => ({ ...cell, pixels: cell.pixels || Array(64).fill(1) })),
          };
        }) };
      }
      window.fire("message", { data });
    },
    evaluate: (source) => vm.runInNewContext(source, sandbox),
    flush: () => new Promise((resolve) => setImmediate(resolve)),
  };
}

function generatedScript() {
  const html = loadHtmlGenerator()({ cspSource: "vscode-webview-resource:" });
  const policy = html.match(/Content-Security-Policy" content="([^"]+)"/);
  const script = html.match(/<script nonce="([^"]+)">([\s\S]*?)<\/script>/);
  assert.ok(policy && script, "generated HTML should contain CSP and inline bootstrap");
  assert.equal(script[1].length, 32);
  assert.ok(policy[1].includes(`script-src 'nonce-${script[1]}'`));
  assert.ok(policy[1].includes(`style-src 'nonce-${script[1]}'`));
  assert.ok(policy[1].includes("img-src vscode-webview-resource:"));
  assert.doesNotMatch(script[2], /\b(const|let)\s+\w+\s*:\s*[A-Z]/, "inline JS must not contain TypeScript annotations");
  new vm.Script(script[2], { filename: "bit8-map-workspace-inline.js" });
  return { html, script: script[2] };
}

test("Visual selector messages persist through the actual CustomTextEditorProvider WorkspaceEdit path", async () => {
  const { source, loadProvider, waitFor } = require("./node_visual_fixture");
  const { parseMapDocument } = require("../out/map_editor_model");
  const ids = new Set(["A1", "A2"]);
  const uri = { scheme: "file", path: "/project/world.b8map", toString: () => "file:///project/world.b8map" };
  const root = { scheme: "file", path: "/project", toString: () => "file:///project" };
  let text = source, receive, documentChanged, scriptSaved, dispose;
  let scriptText = 'func draw() spr(A2,self.x,self.y) end';
  const edits = [];
  const document = { uri, isDirty: false, getText: () => text, positionAt: (offset) => offset };
  const vscode = {
    Range: class { constructor(start, end) { this.start = start; this.end = end; } },
    WorkspaceEdit: class { replace(target, range, replacement) { this.target = target; this.range = range; this.replacement = replacement; } },
    Uri: { joinPath: (_root, ...parts) => ({ toString: () => `file:///project/${parts.join('/')}` }) }, RelativePattern: class {},
    workspace: {
      getWorkspaceFolder: () => ({ uri: root }), textDocuments: [],
      fs: { stat: async () => ({}), readFile: async () => {
        if (scriptText === null) throw new Error("missing script");
        return new TextEncoder().encode(scriptText);
      } }, findFiles: async () => [uri],
      onDidChangeTextDocument(listener) { documentChanged = listener; return { dispose() {} }; },
      onDidSaveTextDocument(listener) { scriptSaved = listener; return { dispose() {} }; },
      async applyEdit(edit) {
        assert.equal(edit.target, uri);
        edits.push(edit);
        text = text.slice(0, edit.range.start) + edit.replacement + text.slice(edit.range.end);
        document.isDirty = true;
        documentChanged({ document });
        return true;
      },
    },
  };
  let webview;
  const panel = { webview: {
    cspSource: "vscode-webview-resource:",
    onDidReceiveMessage(listener) { receive = listener; return { dispose() {} }; },
    postMessage(message) { webview?.dispatchMessage(message); return Promise.resolve(true); },
  }, onDidDispose(listener) { dispose = listener; } };
  try {
    await new (loadProvider(vscode))().resolveCustomTextEditor(document, panel, {});
    const script = panel.webview.html.match(/<script nonce="[^"]+">([\s\S]*?)<\/script>/)[1];
    webview = launchWebview(script, (message) => receive(message));
    receive({ command: "ready" });
    await webview.flush();
    webview.elements.get("nodeTool").fire("click");
    webview.elements.get("nodeList").children[0].fire("click");
    const picker = webview.elements.get("nodeVisualPicker");
    const select = async (value, expected) => {
      picker.value = value; picker.fire("change");
      await waitFor(() => parseMapDocument(text, ids).nodes[0].visual === expected);
      await webview.flush();
    };
    await select("A1", "A1");
    assert.match(text, /visual = "A1"/);
    assert.equal(document.isDirty, true);
    await select("A2", "A2");
    assert.equal((text.match(/^visual = /gm) || []).length, 1);
    await select("", undefined);
    assert.equal(text, source, "clearing restores all tile, script and collider bytes");
    picker.value = "A1"; picker.fire("change");
    // A queued edit must retain the ID from the event, not the later selection.
    webview.elements.get("nodeList").children[1].fire("click");
    await waitFor(() => parseMapDocument(text, ids).nodes[0].visual === "A1");
    assert.equal(parseMapDocument(text, ids).nodes[1].visual, undefined);
    assert.equal(edits.length, 4);
    assert.equal(webview.messages.some((message) => message.type === "error"), false);
    await select("", undefined);
    assert.equal(webview.evaluate("model.nodes[0].scriptVisual"), "A2", "clear reveals the script-derived visual");
    const mapBeforeRefresh = text;
    webview.evaluate("setMode('node'); selectNode('N1'); zoomViewport(2, {x:120,y:80}); panViewport(17,-9)");
    const state = webview.evaluate("JSON.stringify({pan,zoom,mode,selectedNodeId,selectedTile,currentMapPath})");
    const scriptUri = { toString: () => "file:///project/player.b8" };
    scriptText = 'func draw() spr(A1,self.x,self.y) end';
    documentChanged({ document: { uri: scriptUri } });
    await waitFor(() => webview.evaluate("model.nodes[0].scriptVisual") === "A1");
    assert.equal(webview.evaluate("JSON.stringify({pan,zoom,mode,selectedNodeId,selectedTile,currentMapPath})"), state);
    assert.equal(text, mapBeforeRefresh);
    assert.equal(document.isDirty, true);
    scriptText = 'func draw() spr(A99,self.x,self.y) end';
    scriptSaved({ uri: scriptUri });
    await waitFor(() => webview.evaluate("model.nodes[0].scriptVisualIssue")?.includes("A99"));
    assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0])"), undefined);
    scriptText = null; scriptSaved({ uri: scriptUri });
    await waitFor(() => webview.evaluate("model.nodes[0].scriptVisualIssue")?.includes("unavailable"));
    assert.equal(text, mapBeforeRefresh, "missing/invalid script hints never write the map");
    receive({ command: "addNode", nodeType: "Camera", cell: { x: 0, y: 0 } });
    await waitFor(() => parseMapDocument(text, ids).nodes.length === 3);
    const camera = parseMapDocument(text, ids).nodes[2];
    assert.deepEqual([camera.type, camera.x, camera.y], ["Camera", 4, 4]);
    receive({ command: "moveNode", id: camera.id, x: 35, y: 25 });
    await waitFor(() => parseMapDocument(text, ids).nodes[2].x === 36);
    assert.equal(parseMapDocument(text, ids).nodes[2].y, 28, "WorkspaceEdit persists Camera tile centers without boundary resnapping");
  } finally { dispose?.(); }
});

test("Sprite selector messages persist through the actual CustomTextEditorProvider WorkspaceEdit path", async () => {
  const { source, report, loadProvider, waitFor } = require("./node_visual_fixture");
  const { parseMapDocument } = require("../out/map_editor_model");
  const ids = new Set(["A1", "A2"]);
  const uri = { scheme: "file", path: "/project/world.b8map", toString: () => "file:///project/world.b8map" };
  const root = { scheme: "file", path: "/project", toString: () => "file:///project" };
  let text = source, receive, documentChanged, scriptSaved, dispose;
  let scriptText = 'func draw() spr(A2,self.x,self.y) end';
  const edits = [];
  const openedDefinitions = [], warnings = [];
  let missingDefinition = false;
  const document = { uri, isDirty: false, getText: () => text, positionAt: (offset) => offset };
  const vscode = {
    window: { showTextDocument: async (doc) => openedDefinitions.push(doc.uri.toString()), showWarningMessage: (message) => warnings.push(message) },
    Range: class { constructor(start, end) { this.start = start; this.end = end; } },
    WorkspaceEdit: class { replace(target, range, replacement) { this.target = target; this.range = range; this.replacement = replacement; } },
    Uri: { joinPath: (_root, ...parts) => ({ toString: () => `file:///project/${parts.join('/')}` }) }, RelativePattern: class {},
    workspace: {
      getWorkspaceFolder: () => ({ uri: root }), textDocuments: [],
      openTextDocument: async (uri) => { if (missingDefinition) throw new Error('missing file'); return { uri }; },
      fs: { stat: async () => ({}), readFile: async () => {
        if (scriptText === null) throw new Error("missing script");
        return new TextEncoder().encode(scriptText);
      } }, findFiles: async () => [uri],
      onDidChangeTextDocument(listener) { documentChanged = listener; return { dispose() {} }; },
      onDidSaveTextDocument(listener) { scriptSaved = listener; return { dispose() {} }; },
      async applyEdit(edit) {
        assert.equal(edit.target, uri);
        edits.push(edit);
        text = text.slice(0, edit.range.start) + edit.replacement + text.slice(edit.range.end);
        document.isDirty = true;
        documentChanged({ document });
        return true;
      },
    },
  };
  let webview;
  const panel = { webview: {
    cspSource: "vscode-webview-resource:",
    onDidReceiveMessage(listener) { receive = listener; return { dispose() {} }; },
    postMessage(message) { webview?.dispatchMessage(message); return Promise.resolve(true); },
  }, onDidDispose(listener) { dispose = listener; } };
  try {
    await new (loadProvider(vscode, { ...report, sprites: [{name:"Player",preview:"A1"},{name:"Slime",preview:"A2"}] }))().resolveCustomTextEditor(document, panel, {});
    const script = panel.webview.html.match(/<script nonce="[^"]+">([\s\S]*?)<\/script>/)[1];
    webview = launchWebview(script, (message) => receive(message));
    receive({ command: "ready" });
    await webview.flush();
    webview.elements.get("nodeTool").fire("click");
    webview.elements.get("nodeList").children[0].fire("click");
    const picker = webview.elements.get("nodeSpritePicker");
    const select = async (value, expected) => {
      picker.value = value; picker.fire("change");
      await waitFor(() => parseMapDocument(text, ids).nodes[0].sprite === expected);
      await webview.flush();
    };
    await select("Player", "Player");
    assert.match(text, /sprite = "Player"/);
    assert.equal(document.isDirty, true);
    const beforeNavigation = text, editsBeforeNavigation = edits.length;
    const navigation = webview.elements.get('openSpriteDefinition');
    assert.equal(navigation.hidden, false);
    navigation.fire('click');
    await waitFor(() => openedDefinitions.length === 1);
    assert.deepEqual(openedDefinitions, ['file:///project/bit8.sprites.toml']);
    assert.equal(text, beforeNavigation);
    assert.equal(edits.length, editsBeforeNavigation);
    missingDefinition = true;
    navigation.fire('click');
    await waitFor(() => warnings.length === 1);
    assert.match(warnings[0], /definition unavailable/i);
    assert.equal(text, beforeNavigation);
    assert.equal(edits.length, editsBeforeNavigation);
    missingDefinition = false;
    await select("Slime", "Slime");
    assert.equal((text.match(/^sprite = /gm) || []).length, 1);
    await select("", undefined);
    assert.equal(text, source, "clearing restores all tile, script and collider bytes");
    assert.equal(navigation.hidden,true);
    receive({ command:'openSpriteDefinition',id:'N1' });
    receive({ command:'openSpriteDefinition',id:'missing' });
    await webview.flush();
    assert.equal(openedDefinitions.length,1,'no Sprite or missing Node cannot navigate');
    picker.value = "Player"; picker.fire("change");
    // A queued edit must retain the ID from the event, not the later selection.
    webview.elements.get("nodeList").children[1].fire("click");
    await waitFor(() => parseMapDocument(text, ids).nodes[0].sprite === "Player");
    assert.equal(parseMapDocument(text, ids).nodes[1].sprite, undefined);
    assert.equal(edits.length, 4);
    assert.equal(webview.messages.some((message) => message.type === "error"), false);
    await select("", undefined);
    assert.equal(webview.evaluate("model.nodes[0].scriptVisual"), "A2", "clear reveals the script-derived visual");
    const mapBeforeRefresh = text;
    webview.evaluate("setMode('node'); selectNode('N1'); zoomViewport(2, {x:120,y:80}); panViewport(17,-9)");
    const state = webview.evaluate("JSON.stringify({pan,zoom,mode,selectedNodeId,selectedTile,currentMapPath})");
    const scriptUri = { toString: () => "file:///project/player.b8" };
    scriptText = 'func draw() spr(A1,self.x,self.y) end';
    documentChanged({ document: { uri: scriptUri } });
    await waitFor(() => webview.evaluate("model.nodes[0].scriptVisual") === "A1");
    assert.equal(webview.evaluate("JSON.stringify({pan,zoom,mode,selectedNodeId,selectedTile,currentMapPath})"), state);
    assert.equal(text, mapBeforeRefresh);
    assert.equal(document.isDirty, true);
    scriptText = 'func draw() spr(A99,self.x,self.y) end';
    scriptSaved({ uri: scriptUri });
    await waitFor(() => webview.evaluate("model.nodes[0].scriptVisualIssue")?.includes("A99"));
    assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0])"), undefined);
    scriptText = null; scriptSaved({ uri: scriptUri });
    await waitFor(() => webview.evaluate("model.nodes[0].scriptVisualIssue")?.includes("unavailable"));
    assert.equal(text, mapBeforeRefresh, "missing/invalid script hints never write the map");
    receive({ command: "addNode", nodeType: "Camera", cell: { x: 0, y: 0 } });
    await waitFor(() => parseMapDocument(text, ids).nodes.length === 3);
    const camera = parseMapDocument(text, ids).nodes[2];
    assert.deepEqual([camera.type, camera.x, camera.y], ["Camera", 4, 4]);
    receive({ command: "moveNode", id: camera.id, x: 35, y: 25 });
    await waitFor(() => parseMapDocument(text, ids).nodes[2].x === 36);
    assert.equal(parseMapDocument(text, ids).nodes[2].y, 28, "WorkspaceEdit persists Camera tile centers without boundary resnapping");
  } finally { dispose?.(); }
});

function renderedTileCss(zoom) { return 64 * zoom; }

test("production Sprite selector previews core data, overrides both legacy hints, and uses generic for empty Sprite", async () => {
  const {script}=generatedScript();
  const webview=launchWebview(script);
  const node={id:'N1',type:'Node',name:'Player',x:16,y:16,enabled:true,sprite:'Player',visual:'A2',scriptVisual:'A2'};
  const message={type:'model',model:{...emptyMap(8,8),nodes:[node]},sprites:[{name:'Player',preview:'A1'},{name:'Empty',preview:null}],tilesheets:[{group:'A',file:'tilesheet.png',cells:[{id:'A1',x:0,y:0},{id:'A2',x:1,y:0}]}],groups:[{group:'A',file:'tilesheet.png'}],projectMaps:['world.b8map'],currentMap:'world.b8map'};
  webview.dispatchMessage(message); await webview.flush();
  webview.evaluate("setMode('node'); selectNode('N1')");
  assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0]).cell.id"),'A1');
  assert.deepEqual(webview.elements.get('nodeSpritePicker').children.map(item=>item.value),['','Player','Empty']);
  assert.equal(webview.elements.get('nodeSpritePicker').value,'Player');
  assert.equal(webview.elements.get('nodeSpritePreview').hidden,false);
  webview.dispatchMessage({...message,model:{...message.model,nodes:[{...node,sprite:'Empty'}]}});await webview.flush();
  assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0])"),undefined);
  assert.equal(webview.elements.get('nodeSpritePreview').hidden,true);
  const {sprite,...cleared}=node;
  webview.dispatchMessage({...message,model:{...message.model,nodes:[cleared]}});await webview.flush();
  assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0]).cell.id"),'A2');
});

for (const [kind,preview] of [['explicit','A1'],['idle inferred','A2'],['first declared','A1'],['unavailable',null]]) {
  test(`production read-only Sprite Inspector: ${kind} preview and ordered metadata`, async()=>{
    const {script}=generatedScript(); const webview=launchWebview(script);
    const node={id:'N1',type:'Node',name:'Player',x:16,y:16,enabled:true,sprite:'Player',visual:'A2',scriptVisual:'A2',collider:{enabled:true,offset_x:0,offset_y:0,width:8,height:8}};
    const message={type:'model',model:{...emptyMap(8,8),nodes:[node]},sprites:[{name:'Player',preview,animations:[{name:'walk',frames:['A2','A1'],fps:8,loop:false},{name:'idle',frames:['A1'],fps:2,loop:true}]}],tilesheets:[{group:'A',file:'tilesheet.png',cells:[{id:'A1',x:0,y:0},{id:'A2',x:1,y:0}]}],groups:[{group:'A',file:'tilesheet.png'}],projectMaps:['world.b8map'],currentMap:'world.b8map'};
    const unchanged=JSON.stringify(message);
    webview.dispatchMessage(message); await webview.flush();
    webview.evaluate("setMode('node'); selectNode('N1')");
    assert.equal(webview.elements.get('nodeSpritePreviewLabel').textContent,preview || 'Preview unavailable');
    const info=webview.elements.get('nodeSpriteInfo');
    assert.equal(info.hidden,false);
    assert.equal(webview.elements.get('openSpriteDefinition').hidden,false);
    assert.deepEqual(info.children.map(el=>el.textContent),['SPRITE INFO','Player','Animations','walk\nFrames: A2 A1\nFPS: 8\nLoop: No','idle\nFrames: A1\nFPS: 2\nLoop: Yes']);
    assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0])?.cell.id"),preview || undefined);
    assert.equal(webview.elements.get('nodeColliderEnabled').checked,true);
    assert.equal(webview.elements.get('colliderWidth').value,'8');
    assert.equal(JSON.stringify(message),unchanged);
    assert.equal(webview.messages.some(m=>m.command==='setNodeSprite'||m.command==='setNodeVisual'),false,'inspection never edits the document');
    webview.dispatchMessage({...message,model:{...message.model,nodes:[{...node,sprite:undefined}]}}); await webview.flush();
    assert.equal(info.hidden,true);
    assert.equal(webview.elements.get('openSpriteDefinition').hidden,true);
    assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0]).cell.id"),'A2');
    webview.dispatchMessage({...message,model:{...message.model,nodes:[{...node,type:'Camera'}]}}); await webview.flush();
    assert.equal(webview.elements.get('nodeSpriteRow').hidden,true);
    assert.equal(webview.elements.get('openSpriteDefinition').hidden,true);
    assert.equal(webview.elements.get('nodeViewportRow').hidden,false);
  });
}

test('production Sprite navigation hides unavailable controls and safely handles no animations/preview', async()=>{
  const webview=launchWebview(generatedScript().script);
  const node={id:'N1',type:'Node',name:'Player',x:0,y:0,enabled:true};
  const message={type:'model',model:{...emptyMap(8,8),nodes:[node]},sprites:[],tilesheets:[],groups:[],projectMaps:['world.b8map'],currentMap:'world.b8map'};
  webview.dispatchMessage(message); await webview.flush();
  webview.evaluate("setMode('node'); selectNode('N1')");
  const button=webview.elements.get('openSpriteDefinition');
  assert.equal(button.hidden,true);
  button.fire('click');
  webview.dispatchMessage({...message,model:{...message.model,nodes:[{...node,sprite:'Missing'}]}}); await webview.flush();
  assert.equal(button.hidden,true);
  button.fire('click');
  assert.equal(webview.messages.some(m=>m.command==='openSpriteDefinition'),false);
  webview.dispatchMessage({...message,sprites:[{name:'Empty',preview:null,animations:[]}],model:{...message.model,nodes:[{...node,sprite:'Empty'}]}}); await webview.flush();
  assert.equal(button.hidden,false);
  assert.equal(webview.elements.get('nodeSpritePreviewLabel').textContent,'Preview unavailable');
  assert.ok(webview.elements.get('nodeSpriteInfo').children.some(el=>el.textContent==='No animations'));
  button.fire('click');
  assert.deepEqual(JSON.parse(JSON.stringify(webview.messages.at(-1))),{command:'openSpriteDefinition',id:'N1'});
});

function emptyMap(width, height) {
  return { width, height, cells: Array.from({ length: height }, () => Array.from({ length: width }, () => "--")) };
}

function inputEvent(properties = {}) {
  return {
    defaultPrevented: false,
    propagationStopped: false,
    preventDefault() { this.defaultPrevented = true; },
    stopPropagation() { this.propagationStopped = true; },
    ...properties,
  };
}

async function readyMapWebview(width = 8, height = 8) {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const sheet = { group: "A", file: "tilesheet (A).png", imageUri: "webview://A.png", cells: [{ id: "A1", x: 0, y: 0 }] };
  const model = emptyMap(width, height);
  model.cells = Array.from({ length: height }, () => Array.from({ length: width }, () => "A1"));
  webview.dispatchMessage({ type: "model", model, tilesheets: [sheet], groups: [{ group: "A", file: "tilesheet" }], projectMaps: ["world.b8map"], currentMap: "world.b8map" });
  await webview.flush();
  return webview;
}

async function readyNodeWebview(nodes, currentMap = "world.b8map", width = 8, height = 8) {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const sheet = { group: "A", file: "tilesheet (A).png", imageUri: "webview://A.png", cells: [{ id: "A1", x: 0, y: 0 }] };
  const model = { ...emptyMap(width, height), nodes };
  webview.dispatchMessage({ type: "model", model, tilesheets: [sheet], groups: [{ group: "A", file: "tilesheet" }], projectMaps: [currentMap, "east.b8map"], currentMap });
  await webview.flush();
  return webview;
}

test("generated Map Workspace bootstrap uses its CSP nonce and sends ready", () => {
  const { html, script } = generatedScript();
  assert.match(html, /id="tilesheetPicker" aria-label="Registered tilesheet group"/);
  assert.match(html, /id="selectedTilePicker" aria-label="Selected tile"/);
  assert.match(html, /id="cellCoordinate" aria-label="Map cell coordinate">Cell: --, --</);
  const webview = launchWebview(script);
  assert.equal(webview.messages.length, 1);
  assert.equal(webview.messages[0].command, "ready");
});

test("initial map uses 64 CSS px per cell at 100%, centers, and keeps grid inside map bounds", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const viewport = webview.elements.get("viewport");
  viewport.clientWidth = 800; viewport.clientHeight = 600;
  const sheet = {
    group: "A", file: "tilesheet (A).png", imageUri: "webview://A.png",
    cells: [{ id: "A1", x: 0, y: 0 }],
  };
  const model = emptyMap(8, 8);
  model.cells = Array.from({ length: 8 }, () => Array.from({ length: 8 }, () => "A1"));
  webview.dispatchMessage({
    type: "model", model, tilesheets: [sheet], groups: [{ group: "A", file: "tilesheet" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();

  assert.equal(webview.elements.get("zoomLabel").textContent, "100%");
  const rendered = webview.finalMapRect;
  assert.ok(Math.abs(rendered.x - 144.25) < 1e-8, JSON.stringify(rendered));
  assert.ok(Math.abs(rendered.y - 44.25) < 1e-8);
  assert.ok(Math.abs(rendered.width - 512) < 1e-8);
  assert.ok(Math.abs(rendered.height - 512) < 1e-8, "final post-transform map bounds should be 512x512 CSS pixels");
  const initialMapGeometry = JSON.parse(webview.evaluate("JSON.stringify({ x: pan.x, y: pan.y, width: model.width * renderedTileSize(), height: model.height * renderedTileSize() })"));
  assert.ok(Math.abs(initialMapGeometry.x + initialMapGeometry.width / 2 - 400) < 1e-8, "the initial map is centered horizontally");
  assert.ok(Math.abs(initialMapGeometry.y + initialMapGeometry.height / 2 - 300) < 1e-8, "the initial map is centered vertically");
  assert.equal(webview.elements.get("zoomLabel").textContent, "100%", "the initial user zoom is 100%");
  const geometry = JSON.parse(webview.evaluate("JSON.stringify({ viewport: [viewport.clientWidth, viewport.clientHeight], canvasCss: [canvas.style.width, canvas.style.height], backing: [canvas.width, canvas.height], dpr: window.devicePixelRatio, map: [model.width, model.height], cellCss: renderedTileSize(), zoom })"));
  assert.deepEqual(geometry, {
    viewport: [800, 600], canvasCss: ["800px", "600px"], backing: [1600, 1200], dpr: 2,
    map: [8, 8], cellCss: 64, zoom: 1,
  });
  assert.deepEqual([webview.renderTransform.x, webview.renderTransform.y], [2, 2], "render transform scales into the DPR backing store");
  const grid = webview.gridSegmentBounds;
  assert.equal(grid.vertical.length, 9, "an 8-cell-wide map has exactly nine vertical grid boundaries");
  assert.equal(grid.horizontal.length, 9, "an 8-cell-high map has exactly nine horizontal grid boundaries");
  assert.ok(grid.vertical.every(([start, end]) => Math.abs(start.y - initialMapGeometry.y) < 1e-8 && Math.abs(end.y - (initialMapGeometry.y + initialMapGeometry.height)) < 1e-8), "vertical grid lines end at the map bounds");
  assert.ok(grid.horizontal.every(([start, end]) => Math.abs(start.x - initialMapGeometry.x) < 1e-8 && Math.abs(end.x - (initialMapGeometry.x + initialMapGeometry.width)) < 1e-8), "horizontal grid lines end at the map bounds");

  webview.evaluate("zoomViewport(.5, { x: 400, y: 300 })");
  assert.equal(webview.elements.get("zoomLabel").textContent, "50%");
  assert.ok(Math.abs(webview.finalMapRect.width - 256) < 1e-8, "8x8 at 50% is 256 CSS px");
  assert.ok(Math.abs(webview.finalMapRect.height - 256) < 1e-8);
  webview.evaluate("zoomViewport(2, { x: 400, y: 300 })");
  assert.equal(webview.elements.get("zoomLabel").textContent, "200%");
  assert.ok(Math.abs(webview.finalMapRect.width - 1024) < 1e-8, "8x8 at 200% is 1024 CSS px");
  assert.ok(Math.abs(webview.finalMapRect.height - 1024) < 1e-8);

  const paintedCellCenter = JSON.parse(webview.evaluate("JSON.stringify(mapPoint({ x: 4.5, y: 4.5 }))"));
  assert.deepEqual({ x: webview.evaluate("cellAt({ x: mapPoint({ x: 4.5, y: 4.5 }).x, y: mapPoint({ x: 4.5, y: 4.5 }).y }).x"), y: webview.evaluate("cellAt({ x: mapPoint({ x: 4.5, y: 4.5 }).x, y: mapPoint({ x: 4.5, y: 4.5 }).y }).y") }, { x: 4, y: 4 }, "zoomed point-to-cell hit testing remains aligned");
  viewport.fire("pointerdown", { button: 0, pointerId: 2, clientX: paintedCellCenter.x, clientY: paintedCellCenter.y });
  viewport.fire("pointerup");
  assert.ok(webview.messages.some((message) => message.command === "paint" && message.x === 4 && message.y === 4), "inverse hit testing follows the zoomed map transform");

  webview.elements.get("panTool").fire("click");
  viewport.fire("pointerdown", { button: 0, pointerId: 1, clientX: 100, clientY: 100 });
  viewport.fire("pointermove", { clientX: 117, clientY: 133 });
  viewport.fire("pointerup");
  const beforeResize = [...webview.frameRects[0]];
  const zoomBeforeResize = webview.elements.get("zoomLabel").textContent;
  viewport.clientWidth = 1000; viewport.clientHeight = 700;
  webview.resizeObservers[0].trigger();
  const afterResize = webview.frameRects[0];
  assert.ok(Math.abs(afterResize[0] - beforeResize[0]) < 1e-8, "resize preserves deliberate pan and zoom");
  assert.ok(Math.abs(afterResize[1] - beforeResize[1]) < 1e-8, "resize does not recenter after pan");
  assert.equal(webview.elements.get("zoomLabel").textContent, zoomBeforeResize);
});

test("64x64 default map initializes at 12.5%, renders 65 bounded grid lines, and hit-tests both edges", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const viewport = webview.elements.get("viewport");
  viewport.clientWidth = 800; viewport.clientHeight = 600;
  const sheet = { group: "A", file: "tilesheet (A).png", imageUri: "webview://A.png", cells: [{ id: "A1", x: 0, y: 0 }] };
  webview.dispatchMessage({
    type: "model", model: emptyMap(64, 64), tilesheets: [sheet], groups: [{ group: "A", file: "tilesheet" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();

  assert.equal(webview.elements.get("zoomLabel").textContent, "12.5%");
  const metrics = JSON.parse(webview.evaluate("JSON.stringify({ width: model.width, height: model.height, zoom, cell: renderedTileSize(), x: pan.x, y: pan.y })"));
  assert.deepEqual(metrics, { width: 64, height: 64, zoom: 0.125, cell: 8, x: 144, y: 44 });
  const rect = webview.finalMapRect;
  assert.ok(Math.abs(rect.width - 512) < 1e-8 && Math.abs(rect.height - 512) < 1e-8);
  const grid = webview.gridSegmentBounds;
  assert.equal(grid.vertical.length, 65);
  assert.equal(grid.horizontal.length, 65);
  assert.ok(grid.vertical.every(([start, end]) => Math.abs(start.y - metrics.y) < 1e-8 && Math.abs(end.y - (metrics.y + 512)) < 1e-8));
  assert.ok(grid.horizontal.every(([start, end]) => Math.abs(start.x - metrics.x) < 1e-8 && Math.abs(end.x - (metrics.x + 512)) < 1e-8));

  for (const cell of [{ x: 0, y: 0 }, { x: 63, y: 63 }]) {
    const point = JSON.parse(webview.evaluate(`JSON.stringify(mapPoint({ x: ${cell.x + 0.5}, y: ${cell.y + 0.5} }))`));
    viewport.fire("pointerdown", { button: 0, pointerId: 10 + cell.x, clientX: point.x, clientY: point.y });
    viewport.fire("pointerup");
    assert.ok(webview.messages.some((message) => message.command === "paint" && message.x === cell.x && message.y === cell.y), `painting reaches edge cell (${cell.x},${cell.y})`);
  }

  webview.evaluate("zoomViewport(.25, { x: 400, y: 300 }); panViewport(-13, 17)");
  const zoomedPoint = JSON.parse(webview.evaluate("JSON.stringify(mapPoint({ x: 31.5, y: 42.5 }))"));
  assert.deepEqual(JSON.parse(webview.evaluate(`JSON.stringify(cellAt({ x: ${zoomedPoint.x}, y: ${zoomedPoint.y} }))`)), { x: 31, y: 42 });
  viewport.fire("pointerdown", { button: 0, pointerId: 99, clientX: zoomedPoint.x, clientY: zoomedPoint.y });
  viewport.fire("pointerup");
  assert.ok(webview.messages.some((message) => message.command === "paint" && message.x === 31 && message.y === 42), "painting remains correct after pan and zoom on the large map");
});

test("Pencil, Eraser and Pan retain left-drag behavior; Space and middle drag temporarily pan without changing tools", async () => {
  const webview = await readyMapWebview();
  const viewport = webview.elements.get("viewport");
  const point = (x, y) => JSON.parse(webview.evaluate(`JSON.stringify(mapPoint({ x: ${x}, y: ${y} }))`));
  const begin = (button, x, y, pointerId) => { const local = point(x, y); viewport.fire("pointerdown", inputEvent({ button, pointerId, clientX: local.x, clientY: local.y })); };
  const move = (x, y, pointerId) => { const local = point(x, y); viewport.fire("pointermove", inputEvent({ pointerId, clientX: local.x, clientY: local.y })); };
  const end = (pointerId) => viewport.fire("pointerup", inputEvent({ pointerId }));

  begin(0, 1.5, 1.5, 1); move(2.5, 1.5, 1); end(1);
  assert.ok(webview.messages.some((message) => message.command === "paint" && message.x === 2 && message.y === 1 && message.tile === "A1"), "Pencil left drag paints cells");
  const pencilPaintCount = webview.messages.filter((message) => message.command === "paint").length;

  webview.evaluate("setMode('eraser')");
  begin(0, 3.5, 3.5, 2); move(4.5, 3.5, 2); end(2);
  assert.ok(webview.messages.some((message) => message.command === "paint" && message.x === 4 && message.y === 3 && message.tile === null), "Eraser left drag clears cells");

  webview.evaluate("setMode('pan')");
  const beforePan = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  begin(0, 1, 1, 3); move(2, 2, 3); end(3);
  const afterToolPan = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  assert.deepEqual(afterToolPan, { x: beforePan.x + 64, y: beforePan.y + 64 }, "Pan tool left drag moves the map");
  assert.equal(webview.messages.filter((message) => message.command === "paint").length, pencilPaintCount + 2, "Pan tool never paints");

  webview.evaluate("setMode('pencil')");
  const spaceDown = inputEvent({ code: "Space", target: viewport });
  webview.document.fire("keydown", spaceDown);
  assert.equal(spaceDown.defaultPrevented, true);
  const beforeSpacePan = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  begin(0, 2, 2, 4); move(3, 4, 4); end(4);
  assert.deepEqual(JSON.parse(webview.evaluate("JSON.stringify(pan)")), { x: beforeSpacePan.x + 64, y: beforeSpacePan.y + 128 });
  assert.equal(webview.evaluate("mode"), "pencil", "temporary Space pan does not mutate the selected tool");
  assert.equal(webview.messages.filter((message) => message.command === "paint").length, pencilPaintCount + 2, "Space-pan does not accidentally paint");
  const spaceUp = inputEvent({ code: "Space", target: viewport });
  webview.document.fire("keyup", spaceUp);
  assert.equal(spaceUp.defaultPrevented, true);
  assert.equal(webview.evaluate("spaceHeld"), false, "keyup clears temporary Space state");

  webview.evaluate("setMode('eraser')");
  webview.document.fire("keydown", inputEvent({ code: "Space", target: viewport }));
  const beforeEraserSpace = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  begin(0, 5, 5, 5); move(6, 4, 5); end(5);
  webview.document.fire("keyup", inputEvent({ code: "Space", target: viewport }));
  assert.deepEqual(JSON.parse(webview.evaluate("JSON.stringify(pan)")), { x: beforeEraserSpace.x + 64, y: beforeEraserSpace.y - 64 });
  assert.equal(webview.evaluate("mode"), "eraser", "temporary Space pan restores the unchanged Eraser selection");

  webview.evaluate("setMode('pencil')");
  const beforeMiddle = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  begin(1, 1, 1, 6); move(3, 2, 6); end(6);
  assert.deepEqual(JSON.parse(webview.evaluate("JSON.stringify(pan)")), { x: beforeMiddle.x + 128, y: beforeMiddle.y + 64 }, "middle drag pans regardless of selected tool");
  assert.equal(webview.evaluate("mode"), "pencil");
  assert.equal(webview.messages.filter((message) => message.command === "paint").length, pencilPaintCount + 2, "middle drag never paints");
});

test("wheel pans smoothly on both axes; Cmd/Ctrl-wheel zooms around the pointer", async () => {
  const webview = await readyMapWebview();
  const viewport = webview.elements.get("viewport");
  const originalZoom = webview.evaluate("zoom");
  const beforePan = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  const wheel = inputEvent({ deltaX: 3.25, deltaY: -7.75, deltaMode: 0, clientX: 220, clientY: 180 });
  viewport.fire("wheel", wheel);
  assert.equal(wheel.defaultPrevented, true, "consumed wheel input prevents host/page scrolling");
  assert.deepEqual(JSON.parse(webview.evaluate("JSON.stringify(pan)")), { x: beforePan.x - 3.25, y: beforePan.y + 7.75 }, "smooth two-axis deltas are preserved without quantization");
  assert.equal(webview.evaluate("zoom"), originalZoom, "ordinary wheel does not zoom");
  const afterTwoAxis = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  viewport.fire("wheel", inputEvent({ deltaX: 11.5, deltaY: 0, deltaMode: 0, clientX: 220, clientY: 180 }));
  assert.equal(webview.evaluate("pan.x"), afterTwoAxis.x - 11.5, "horizontal wheel input pans horizontally");
  assert.equal(webview.evaluate("pan.y"), afterTwoAxis.y);

  for (const modifier of ["metaKey", "ctrlKey"]) {
    const anchor = { x: 275, y: 215 };
    const before = JSON.parse(webview.evaluate(`JSON.stringify({ x: ((${anchor.x}) - pan.x) / renderedTileSize(), y: ((${anchor.y}) - pan.y) / renderedTileSize() })`));
    const event = inputEvent({ deltaX: 0, deltaY: -30.5, deltaMode: 0, clientX: anchor.x, clientY: anchor.y, [modifier]: true });
    viewport.fire("wheel", event);
    assert.equal(event.defaultPrevented, true);
    const after = JSON.parse(webview.evaluate(`JSON.stringify({ zoom, x: ((${anchor.x}) - pan.x) / renderedTileSize(), y: ((${anchor.y}) - pan.y) / renderedTileSize() })`));
    assert.ok(after.zoom > originalZoom, `${modifier} wheel zooms in`);
    assert.ok(Math.abs(after.x - before.x) < 1e-10 && Math.abs(after.y - before.y) < 1e-10, "zoom keeps the pointer's map coordinate fixed");
  }
  assert.equal(webview.messages.filter((message) => message.command === "paint").length, 0, "wheel navigation never paints");
});

test("Space ignores form controls, keyup and blur clear it, and pointercancel safely ends a captured gesture", async () => {
  const webview = await readyMapWebview();
  const viewport = webview.elements.get("viewport");
  for (const control of [webview.elements.get("mapPicker"), webview.elements.get("tilesheetPicker"), webview.elements.get("selectedTilePicker"), webview.elements.get("pencil")]) {
    const event = inputEvent({ code: "Space", target: control });
    webview.document.fire("keydown", event);
    assert.equal(event.defaultPrevented, false);
    assert.equal(webview.evaluate("spaceHeld"), false, "form/button interaction does not arm temporary pan");
  }
  webview.document.fire("keydown", inputEvent({ code: "Space", target: viewport }));
  assert.equal(webview.evaluate("spaceHeld"), true);
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 72, clientX: 200, clientY: 200 }));
  assert.equal(viewport.hasPointerCapture(72), true);
  assert.equal(webview.evaluate("gesture"), "pan");
  viewport.fire("pointercancel", inputEvent({ pointerId: 72 }));
  assert.equal(viewport.hasPointerCapture(72), false, "cancel releases pointer capture");
  const panAfterCancel = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  viewport.fire("pointermove", inputEvent({ pointerId: 72, clientX: 280, clientY: 260 }));
  assert.deepEqual(JSON.parse(webview.evaluate("JSON.stringify(pan)")), panAfterCancel, "moves after cancel do not continue the gesture");
  webview.document.fire("keyup", inputEvent({ code: "Space", target: viewport }));
  assert.equal(webview.evaluate("spaceHeld"), false, "keyup clears Space state");

  webview.document.fire("keydown", inputEvent({ code: "Space", target: viewport }));
  viewport.fire("pointerdown", inputEvent({ button: 1, pointerId: 73, clientX: 100, clientY: 120 }));
  webview.windowBlur();
  assert.equal(webview.evaluate("spaceHeld"), false, "blur clears Space state");
  assert.equal(webview.evaluate("gesture"), null, "blur ends an active middle-button gesture");
  const afterBlur = JSON.parse(webview.evaluate("JSON.stringify(pan)"));
  viewport.fire("pointermove", inputEvent({ pointerId: 73, clientX: 150, clientY: 180 }));
  assert.deepEqual(JSON.parse(webview.evaluate("JSON.stringify(pan)")), afterBlur, "blur prevents a stuck pointer gesture");
});

test("cell coordinate shows logical hover, neutral/outside state, and last Pencil/Eraser cell", async () => {
  const webview = await readyMapWebview(64, 64);
  const viewport = webview.elements.get("viewport");
  const coordinate = () => webview.elements.get("cellCoordinate").textContent;
  const point = (x, y) => JSON.parse(webview.evaluate(`JSON.stringify(mapPoint({ x: ${x}, y: ${y} }))`));
  const move = (x, y, pointerId = 50) => {
    const p = point(x, y);
    viewport.fire("pointermove", inputEvent({ pointerId, clientX: p.x, clientY: p.y }));
  };
  const clickCell = (x, y, pointerId) => {
    const p = point(x + 0.5, y + 0.5);
    viewport.fire("pointerdown", inputEvent({ button: 0, pointerId, clientX: p.x, clientY: p.y }));
    viewport.fire("pointerup", inputEvent({ pointerId }));
  };

  assert.equal(coordinate(), "Cell: --, --", "no prior valid cell starts neutral");
  move(0.5, 0.5);
  assert.equal(coordinate(), "Cell: 0, 0", "top-left uses logical tile coordinates");
  move(32.5, 28.5);
  assert.equal(coordinate(), "Cell: 32, 28", "center hover does not expose pixels or canvas coordinates");
  move(63.5, 63.5);
  assert.equal(coordinate(), "Cell: 63, 63", "bottom-right is the last cell in a 64x64 map");
  viewport.fire("pointermove", inputEvent({ pointerId: 50, clientX: -10, clientY: -10 }));
  assert.equal(coordinate(), "Cell: --, --", "outside-map pointer positions are never reported");
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: --, --");

  clickCell(7, 9, 61);
  assert.equal(coordinate(), "Cell: 7, 9", "Pencil interaction remembers its logical cell");
  move(20.5, 22.5);
  assert.equal(coordinate(), "Cell: 20, 22", "valid hover temporarily takes precedence");
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: 7, 9", "leaving restores the last interacted cell");

  webview.evaluate("setMode('eraser')");
  clickCell(63, 63, 62);
  assert.equal(coordinate(), "Cell: 63, 63", "Eraser interaction remembers its logical cell");
  assert.ok(webview.messages.some((message) => message.command === "paint" && message.x === 63 && message.y === 63 && message.tile === null));
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: 63, 63");
});

test("coordinate identity survives supported zoom and pan; navigation gestures do not replace last interaction", async () => {
  const webview = await readyMapWebview(64, 64);
  const viewport = webview.elements.get("viewport");
  const coordinate = () => webview.elements.get("cellCoordinate").textContent;
  const hover = (x, y, pointerId = 70) => {
    const point = JSON.parse(webview.evaluate(`JSON.stringify(mapPoint({ x: ${x}, y: ${y} }))`));
    viewport.fire("pointermove", inputEvent({ pointerId, clientX: point.x, clientY: point.y }));
  };
  const click = (x, y, pointerId) => {
    const point = JSON.parse(webview.evaluate(`JSON.stringify(mapPoint({ x: ${x + 0.5}, y: ${y + 0.5} }))`));
    viewport.fire("pointerdown", inputEvent({ button: 0, pointerId, clientX: point.x, clientY: point.y }));
    viewport.fire("pointerup", inputEvent({ pointerId }));
  };

  click(32, 28, 71);
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: 32, 28");
  for (const zoom of [0.125, 0.5, 1, 2, 1.37]) {
    const anchor = JSON.parse(webview.evaluate("JSON.stringify(mapPoint({ x: 32.5, y: 28.5 }))"));
    webview.evaluate(`zoomViewport(${zoom}, { x: ${anchor.x}, y: ${anchor.y} })`);
    hover(32.5, 28.5);
    assert.equal(coordinate(), "Cell: 32, 28", `same logical cell at ${zoom * 100}%`);
  }
  webview.evaluate("panViewport(37, -23)");
  hover(32.5, 28.5);
  assert.equal(coordinate(), "Cell: 32, 28", "pan does not change the cell's logical identity");
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: 32, 28", "hover navigation does not overwrite last interaction");

  webview.evaluate("setMode('pan')");
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 72, clientX: 200, clientY: 200 }));
  viewport.fire("pointermove", inputEvent({ pointerId: 72, clientX: 219, clientY: 211 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 72 }));
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: 32, 28", "Pan tool gesture preserves last interacted cell");

  webview.document.fire("keydown", inputEvent({ code: "Space", target: viewport }));
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 73, clientX: 200, clientY: 200 }));
  viewport.fire("pointermove", inputEvent({ pointerId: 73, clientX: 225, clientY: 220 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 73 }));
  webview.document.fire("keyup", inputEvent({ code: "Space", target: viewport }));
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: 32, 28", "Space-pan does not remember a fake selection");

  viewport.fire("pointerdown", inputEvent({ button: 1, pointerId: 74, clientX: 200, clientY: 200 }));
  viewport.fire("pointermove", inputEvent({ pointerId: 74, clientX: 230, clientY: 230 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 74 }));
  viewport.fire("pointerleave");
  assert.equal(coordinate(), "Cell: 32, 28", "middle-button pan does not remember a fake selection");
});

test("Node mode shows only current-map nodes in stable ID order and synchronizes sidebar/marker selection", async () => {
  const webview = await readyNodeWebview([
    { id: "N10", name: "Door", x: 32, y: 32, enabled: true },
    { id: "N2", name: "Player", x: 16, y: 16, enabled: true },
  ]);
  const nodeTool = webview.elements.get("nodeTool"), nodeSection = webview.elements.get("nodeSection");
  nodeTool.fire("click");
  assert.equal(nodeSection.hidden, false);
  assert.deepEqual(webview.elements.get("nodeList").children.map((item) => item.textContent), ["Player", "Door"]);
  assert.deepEqual(webview.drawCalls.filter((call) => typeof call[0] === "string").map((call) => call[0]), ["Player", "Door"], "editor-only node markers are rendered above the map");
  assert.equal(webview.elements.get("nodeProperties").hidden, true);
  const originalCells = JSON.stringify(webview.evaluate("model.cells"));

  webview.elements.get("nodeList").children[1].fire("click");
  assert.equal(webview.evaluate("selectedNodeId"), "N10");
  assert.equal(webview.elements.get("nodeProperties").hidden, false);
  assert.equal(webview.elements.get("nodeName").value, "Door");
  assert.equal(webview.elements.get("nodeTypeValue").textContent, "Node");
  const marker = JSON.parse(webview.evaluate("JSON.stringify(mapNodePosition(model.nodes[0]))"));
  const viewport = webview.elements.get("viewport");
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 501, clientX: marker.x, clientY: marker.y }));
  viewport.fire("pointerup", inputEvent({ pointerId: 501 }));
  assert.equal(webview.evaluate("selectedNodeId"), "N2", "clicking the rendered marker selects that same node");
  assert.equal(JSON.stringify(webview.evaluate("model.cells")), originalCells, "node selection does not alter tile state");

  webview.dispatchMessage({
    type: "model", model: { ...emptyMap(8, 8), nodes: [{ id: "N2", name: "Other Map Node", x: 8, y: 8, enabled: true }] },
    tilesheets: [{ group: "A", file: "tilesheet.png", imageUri: "webview://A.png", cells: [{ id: "A1", x: 0, y: 0 }] }],
    groups: [{ group: "A", file: "tilesheet" }], projectMaps: ["east.b8map"], currentMap: "east.b8map",
  });
  await webview.flush();
  assert.equal(webview.evaluate("selectedNodeId"), null, "selection does not leak when map changes, even when an ID is reused");
  assert.deepEqual(webview.elements.get("nodeList").children.map((item) => item.textContent), ["Other Map Node"]);
});

test("Node Type property displays registered built-in identity without an editor control", async () => {
  const webview = await readyNodeWebview([
    { id: "N1", type: "Camera", name: "View", x: 8, y: 8, enabled: true },
  ]);
  webview.elements.get("nodeTool").fire("click");
  webview.elements.get("nodeList").children[0].fire("click");
  const typeValue = webview.elements.get("nodeTypeValue");
  assert.equal(typeValue.textContent, "Camera");
  assert.equal(typeValue.tagName, "div", "Type is read-only and not an input/select control");
  assert.equal(webview.messages.some((message) => message.command === "setNodeType"), false);
  webview.dispatchMessage({
    type: "model", model: { ...emptyMap(8, 8), nodes: [{ id: "N1", type: "Node", name: "Ordinary", x: 0, y: 0, enabled: true }] },
    tilesheets: [{ group: "A", file: "tilesheet.png", imageUri: "webview://A.png", cells: [{ id: "A1", x: 0, y: 0 }] }],
    groups: [{ group: "A", file: "tilesheet" }], projectMaps: ["other.b8map"], currentMap: "other.b8map",
  });
  await webview.flush();
  assert.equal(webview.evaluate("selectedNodeId"), null);
  assert.equal(typeValue.textContent, "Node", "map switching cannot retain a prior type selection");
});

test("ordinary Node visual uses the mapped 8x8 tile, stays selectable/draggable, and is independent of collider overlay", async () => {
  const webview = await readyNodeWebview([{
    id: "N1", type: "Node", name: "Player", x: 16, y: 16, enabled: true, visual: "A1",
    collider: { enabled: true, offset_x: 0, offset_y: 0, width: 8, height: 8 },
  }]);
  const spriteCall = webview.drawImageCalls.at(-1);
  assert.deepEqual(spriteCall.slice(1), [0, 0, 8, 8, 272, 172, 64, 64], "A1 is drawn at Node game-pixel origin using one map cell of display size");
  assert.equal(webview.cameraFrames.some(({ color }) => color === "rgba(255, 196, 0, .95)"), true, "collider remains an independent overlay around the visual");

  webview.elements.get("nodeTool").fire("click");
  webview.elements.get("nodeList").children[0].fire("click");
  const visualPicker = webview.elements.get("nodeVisualPicker");
  assert.deepEqual(visualPicker.children.map((option) => option.value), ["", "A1"]);
  assert.equal(visualPicker.value, "A1");
  visualPicker.value = ""; visualPicker.fire("change");
  assert.ok(webview.messages.some((message) => message.command === "setNodeVisual" && message.id === "N1" && message.visual === null));
  visualPicker.value = "A1"; visualPicker.fire("change");
  assert.ok(webview.messages.some((message) => message.command === "setNodeVisual" && message.id === "N1" && message.visual === "A1"));

  const viewport = webview.elements.get("viewport");
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 809, clientX: 300, clientY: 200 }));
  assert.equal(webview.evaluate("selectedNodeId"), "N1", "clicking within the visible sprite selects the Node");
  viewport.fire("pointermove", inputEvent({ pointerId: 809, clientX: 364, clientY: 200 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 809 }));
  const move = webview.messages.filter((message) => message.command === "moveNode").at(-1);
  assert.deepEqual([move.x, move.y], [24, 16], "dragging a visual still uses the normal snapped Node move");
  // Model updates arrive through the normal WorkspaceEdit/document-change
  // path; simulate its latest in-progress position for this canvas-only test.
  webview.evaluate("nodeDrag = {id:'N1',x:24,y:16}; zoomViewport(2, {x:400,y:300}); panViewport(7,-9); drawMap()");
  const expectedPosition = JSON.parse(webview.evaluate("JSON.stringify(mapGamePixelPoint({x:24,y:16}))"));
  assert.deepEqual(webview.drawImageCalls.at(-1).slice(1), [0, 0, 8, 8, expectedPosition.x, expectedPosition.y, 128, 128], "visual follows Node coordinates and the shared pan/zoom transform");
});

test("script visual uses registered pixels independently of palette selection and manual override wins", async () => {
  const webview = launchWebview(generatedScript().script);
  const sheets = [
    { group: "A", file: "A.png", cells: Array.from({ length: 7 }, (_, x) => ({ id: `A${x + 1}`, x, y: 0 })) },
    { group: "B", file: "B.png", cells: [{ id: "B12", x: 0, y: 0 }] },
  ];
  let nodes = [{ id: "N1", type: "Node", name: "Player", x: 16, y: 24, enabled: true, scriptVisual: "A4",
    collider: { enabled: true, offset_x: 8, offset_y: 0, width: 8, height: 8 } }];
  const send = async () => {
    webview.dispatchMessage({ type: "model", model: { ...emptyMap(8, 8), nodes }, tilesheets: sheets,
      groups: sheets.map((sheet) => ({ group: sheet.group, file: sheet.file })), projectMaps: ["world.b8map"], currentMap: "world.b8map" });
    await webview.flush();
  };
  await send();
  webview.evaluate("setMode('node'); selectNode('N1'); showTilesheetGroup('B'); drawMap()");
  assert.deepEqual(webview.drawImageCalls.at(-1).slice(1), [24, 0, 8, 8, 272, 236, 64, 64]);
  assert.match(webview.elements.get("nodeVisualSource").textContent, /A4.*Source: Script/);
  assert.equal(webview.elements.get("nodeVisualPicker").value, "", "derived visual does not masquerade as a manual override");
  const overlay = webview.cameraFrames.find((frame) => frame.color === "rgba(255, 196, 0, .95)");
  assert.deepEqual([overlay.x, overlay.y, overlay.width, overlay.height], [336, 236, 64, 64]);
  nodes = [{ ...nodes[0], visual: "A1" }]; await send();
  assert.deepEqual(webview.drawImageCalls.at(-1).slice(1, 5), [0, 0, 8, 8]);
  assert.equal(webview.elements.get("nodeVisualSource").textContent, "Source: Manual");
  nodes = [{ ...nodes[0], visual: "A99" }]; await send();
  assert.deepEqual(webview.drawImageCalls.at(-1).slice(1, 5), [24, 0, 8, 8], "invalid override falls back to a valid script hint");
  assert.match(webview.elements.get("nodeVisualStatus").textContent, /A99/);
  nodes = [{ ...nodes[0], visual: undefined, scriptVisual: "A7" }]; await send();
  assert.deepEqual(webview.drawImageCalls.at(-1).slice(1, 5), [48, 0, 8, 8]);
  assert.equal(webview.evaluate("selectedGroup"), "B");
  assert.equal(webview.evaluate("selectedNodeId"), "N1");
  nodes = [{ ...nodes[0], type: "Camera", scriptVisual: "A4" }]; await send();
  assert.equal(webview.evaluate("resolvedNodeVisual(model.nodes[0])"), undefined);
  assert.equal(webview.elements.get("nodeVisualRow").hidden, true);
  assert.equal(webview.cameraFrames.some((frame) => frame.color === "rgba(255, 196, 0, .95)"), false);
  assert.equal(webview.messages.some((message) => message.command === "setNodeVisual"), false, "refresh/render never persist a derived Visual");
});

test("missing visual IDs remain visible and clearable, generic markers stay compact, and Camera visuals are ignored", async () => {
  const webview = await readyNodeWebview([
    { id: "N1", type: "Node", name: "Missing", x: 16, y: 16, enabled: true, visual: "A99" },
    { id: "N2", type: "Node", name: "Plain", x: 32, y: 32, enabled: true },
    { id: "N3", type: "Camera", name: "View", x: 40, y: 40, enabled: true, visual: "A1" },
  ]);
  webview.elements.get("nodeTool").fire("click");
  webview.elements.get("nodeList").children[0].fire("click");
  const visualPicker = webview.elements.get("nodeVisualPicker");
  assert.deepEqual(visualPicker.children.map((option) => option.textContent), ["None", "A1", "(Missing: A99)"]);
  assert.equal(visualPicker.value, "A99");
  assert.equal(webview.elements.get("nodeVisualStatus").hidden, false);
  webview.elements.get("clearNodeVisual").fire("click");
  assert.ok(webview.messages.some((message) => message.command === "setNodeVisual" && message.visual === null));

  const plainPosition = JSON.parse(webview.evaluate("JSON.stringify(mapGamePixelPoint({x:32,y:32}))"));
  assert.ok(webview.drawCalls.some((call) => JSON.stringify(call) === JSON.stringify([plainPosition.x - 4, plainPosition.y - 4, 8, 8])), "plain Node marker is small and centered on x/y");
  webview.elements.get("nodeList").children[2].fire("click");
  assert.equal(webview.elements.get("nodeVisualRow").hidden, true, "Camera has no sprite selector");
  assert.equal(webview.drawImageCalls.some((call) => call[5] === 40 * 8 + 144 && call[6] === 40 * 8 + 44), false, "Camera does not render a Node sprite");
});

test("Camera creation choices post explicit types while generic Add Node remains Node", async () => {
  const webview = await readyNodeWebview([]);
  webview.elements.get("nodeTool").fire("click");
  const menu = webview.elements.get("addNodeMenu");
  menu.hidden = true;
  webview.elements.get("addNode").fire("click");
  assert.equal(menu.hidden, false);
  webview.elements.get("addCameraNode").fire("click");
  assert.equal(menu.hidden, true);
  assert.ok(webview.messages.some((message) => message.command === "addNode" && message.nodeType === "Camera"));
  webview.elements.get("addNode").fire("click");
  webview.elements.get("addGenericNode").fire("click");
  assert.ok(webview.messages.some((message) => message.command === "addNode" && message.nodeType === "Node"));
});

test("Camera frames use centered 64x64 game-pixel bounds, map transform, multiple nodes, and subdued disabled style", async () => {
  const webview = await readyNodeWebview([
    { id: "N1", type: "Camera", name: "Camera", x: 32, y: 32, enabled: true },
    { id: "N2", type: "Camera", name: "Camera2", x: 8, y: 8, enabled: false },
  ]);
  assert.equal(webview.cameraFrames.length, 2);
  assert.deepEqual(webview.cameraFrames.map(({ x, y, width, height }) => [x, y, width, height]), [
    [144, 44, 512, 512],
    [-48, -148, 512, 512],
  ]);
  assert.match(webview.cameraFrames[0].color, /\.95/);
  assert.match(webview.cameraFrames[1].color, /\.38/);
  assert.ok(webview.drawCalls.some((call) => JSON.stringify(call) === JSON.stringify([390, 290, 20, 20])), "visible Camera handle is centered, not a game-pixel body");
  assert.ok(webview.drawCalls.some((call) => JSON.stringify(call) === JSON.stringify([380, 299.5, 40, 1])), "Camera horizontal crosshair is centered at exact game-pixel x/y");
  assert.ok(webview.drawCalls.some((call) => JSON.stringify(call) === JSON.stringify([399.5, 280, 1, 40])), "Camera vertical crosshair is centered at exact game-pixel x/y");
  webview.elements.get("nodeTool").fire("click");
  webview.elements.get("nodeList").children[0].fire("click");
  assert.equal(webview.elements.get("nodeViewportRow").hidden, false);
  assert.match(generatedScript().html, /id="nodeViewportValue">64 × 64</);
  assert.equal(webview.cameraFrames.at(-2).lineWidth, 1, "selection uses the existing stronger marker/frame emphasis");
  const cameraCenter = JSON.parse(webview.evaluate("JSON.stringify(mapNodePosition(model.nodes[0]))"));
  webview.elements.get("viewport").fire("pointerdown", inputEvent({ button: 0, pointerId: 709, clientX: cameraCenter.x, clientY: cameraCenter.y }));
  webview.elements.get("viewport").fire("pointerup", inputEvent({ pointerId: 709 }));
  assert.equal(webview.evaluate("selectedNodeId"), "N1", "the Camera center marker uses normal Node selection");
  webview.elements.get("nodeList").children[1].fire("click");
  assert.equal(webview.elements.get("nodeViewportRow").hidden, false, "disabled Camera remains visible/editable");
});

test("Camera viewport follows pan, zoom, and center-marker dragging immediately without map-edge clamping", async () => {
  const webview = await readyNodeWebview([
    { id: "N1", type: "Camera", name: "Camera", x: 32, y: 32, enabled: true },
  ]);
  const originalCells = JSON.stringify(webview.evaluate("model.cells"));
  const viewport = webview.elements.get("viewport");
  webview.elements.get("panTool").fire("click");
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 710, clientX: 700, clientY: 500 }));
  viewport.fire("pointermove", inputEvent({ pointerId: 710, clientX: 720, clientY: 530 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 710 }));
  assert.deepEqual([webview.cameraFrames.at(-1).x, webview.cameraFrames.at(-1).y], [164, 74]);

  webview.evaluate("zoomViewport(2, { x: 400, y: 300 })");
  assert.deepEqual([webview.cameraFrames.at(-1).width, webview.cameraFrames.at(-1).height], [1024, 1024]);
  const center = JSON.parse(webview.evaluate("JSON.stringify(mapNodePosition(model.nodes[0]))"));
  webview.elements.get("nodeTool").fire("click");
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 711, clientX: center.x, clientY: center.y }));
  viewport.fire("pointermove", inputEvent({ pointerId: 711, clientX: center.x + 128, clientY: center.y + 128 }));
  const draggedFrame = webview.cameraFrames.at(-1);
  assert.deepEqual([draggedFrame.x, draggedFrame.y, draggedFrame.width, draggedFrame.height], [120, 40, 1024, 1024]);
  viewport.fire("pointerup", inputEvent({ pointerId: 711 }));
  const move = webview.messages.filter((message) => message.command === "moveNode").at(-1);
  assert.deepEqual([move.x, move.y], [44, 44]);
  assert.equal(JSON.stringify(webview.evaluate("model.cells")), originalCells, "viewport overlay/drag does not edit map tiles");
  assert.equal(webview.messages.some((message) => message.command === "paint"), false);
});

test("Camera pointer snapping is in game pixels at every zoom and leaves old centers untouched until dragged", async () => {
  for (const zoom of [.125, .5, 1, 2, 4]) {
    const webview = await readyNodeWebview([{ id: "N1", type: "Camera", name: "View", x: 32, y: 32, enabled: true }]);
    webview.evaluate(`setMode('node'); zoomViewport(${zoom}, {x:100,y:80}); panViewport(31,-17)`);
    const viewport = webview.elements.get("viewport");
    const point = (x, y) => JSON.parse(webview.evaluate(`JSON.stringify(mapGamePixelPoint({x:${x},y:${y}}))`));
    const old = point(32, 32);
    viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 800, clientX: old.x, clientY: old.y }));
    viewport.fire("pointerup", inputEvent({ pointerId: 800 }));
    assert.equal(webview.messages.some((message) => message.command === "moveNode"), false, "selection alone must not migrate old Camera coordinates");
    viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 801, clientX: old.x, clientY: old.y }));
    const target = point(10, 3);
    viewport.fire("pointermove", inputEvent({ pointerId: 801, clientX: target.x, clientY: target.y }));
    assert.deepEqual(JSON.parse(webview.evaluate("JSON.stringify([nodeDrag.x,nodeDrag.y])")), [12, 4]);
    const center = point(12, 4);
    const frame = webview.cameraFrames.at(-1);
    assert.equal(frame.width, 64 * 8 * zoom);
    assert.equal(frame.height, 64 * 8 * zoom);
    assert.ok(Math.abs(frame.x + frame.width / 2 - center.x) < 1e-8);
    assert.ok(Math.abs(frame.y + frame.height / 2 - center.y) < 1e-8);
    assert.ok(webview.frameRects.some(([x,y,w,h]) => w === 20 && h === 20 && x + 10 === center.x && y + 10 === center.y), "accepted Camera handle follows the snapped center");
    viewport.fire("pointerup", inputEvent({ pointerId: 801 }));
    const move = webview.messages.find((message) => message.command === "moveNode");
    assert.deepEqual([move.x, move.y], [12, 4]);
  }
});

test("Add Node uses the current logical cell or a safe origin fallback", async () => {
  const webview = await readyNodeWebview([]);
  const viewport = webview.elements.get("viewport");
  webview.elements.get("nodeTool").fire("click");
  const pointer = JSON.parse(webview.evaluate("JSON.stringify(mapPoint({ x: 3.5, y: 2.5 }))"));
  viewport.fire("pointermove", inputEvent({ clientX: pointer.x, clientY: pointer.y }));
  webview.elements.get("addNode").fire("click");
  webview.elements.get("addGenericNode").fire("click");
  assert.ok(webview.messages.some((message) => message.command === "addNode" && message.cell.x === 3 && message.cell.y === 2));

  const fallback = await readyNodeWebview([]);
  fallback.elements.get("nodeTool").fire("click");
  fallback.elements.get("addNode").fire("click");
  fallback.elements.get("addGenericNode").fire("click");
  const addMessage = fallback.messages.find((message) => message.command === "addNode");
  assert.ok(addMessage);
  assert.equal(Object.hasOwn(addMessage, "cell"), false, "no valid cell means extension-model fallback (0,0)");
});

test("node movement snaps game pixels under pan/zoom while pure, Space, and middle pan never move nodes", async () => {
  const webview = await readyNodeWebview([{ id: "N1", name: "Player", x: 16, y: 16, enabled: true }]);
  const viewport = webview.elements.get("viewport");
  webview.elements.get("nodeTool").fire("click");
  webview.evaluate("panViewport(37, -23); zoomViewport(.5, { x: 400, y: 300 })");
  const start = JSON.parse(webview.evaluate("JSON.stringify(mapNodePosition(model.nodes[0]))"));
  const target = JSON.parse(webview.evaluate("JSON.stringify(mapPoint({ x: 4.4, y: 5.2 }))"));
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 601, clientX: start.x, clientY: start.y }));
  viewport.fire("pointermove", inputEvent({ pointerId: 601, clientX: target.x, clientY: target.y }));
  viewport.fire("pointerup", inputEvent({ pointerId: 601 }));
  const moved = webview.messages.find((message) => message.command === "moveNode");
  assert.deepEqual([moved.x, moved.y], [32, 40], "inverse pan/zoom mapping snaps to game-pixel multiples of eight");

  const before = webview.messages.filter((message) => message.command === "moveNode").length;
  const panStart = JSON.parse(webview.evaluate("JSON.stringify(mapNodePosition(model.nodes[0]))"));
  viewport.fire("pointerdown", inputEvent({ button: 1, pointerId: 602, clientX: panStart.x, clientY: panStart.y }));
  viewport.fire("pointermove", inputEvent({ pointerId: 602, clientX: panStart.x + 50, clientY: panStart.y + 20 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 602 }));
  webview.document.fire("keydown", inputEvent({ code: "Space", target: viewport }));
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 603, clientX: 200, clientY: 200 }));
  viewport.fire("pointermove", inputEvent({ pointerId: 603, clientX: 230, clientY: 220 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 603 }));
  webview.document.fire("keyup", inputEvent({ code: "Space", target: viewport }));
  webview.evaluate("setMode('pan')");
  viewport.fire("pointerdown", inputEvent({ button: 0, pointerId: 604, clientX: 200, clientY: 200 }));
  viewport.fire("pointermove", inputEvent({ pointerId: 604, clientX: 240, clientY: 220 }));
  viewport.fire("pointerup", inputEvent({ pointerId: 604 }));
  assert.equal(webview.messages.filter((message) => message.command === "moveNode").length, before, "navigation gestures do not emit node moves");
});

test("node properties post rename, snapped coordinates, enabled, and delete actions", async () => {
  const webview = await readyNodeWebview([
    { id: "N1", name: "Player", x: 16, y: 24, enabled: true },
    { id: "N2", name: "Door", x: 32, y: 32, enabled: false },
  ]);
  webview.elements.get("nodeTool").fire("click");
  webview.elements.get("nodeList").children[0].fire("click");
  const name = webview.elements.get("nodeName"); name.value = "Hero"; name.fire("change");
  const x = webview.elements.get("nodeX"), y = webview.elements.get("nodeY");
  x.value = "27"; x.fire("change");
  y.value = "35"; y.fire("change");
  const enabled = webview.elements.get("nodeEnabled"); enabled.checked = false; enabled.fire("change");
  const commands = webview.messages.filter((message) => ["renameNode", "moveNode", "setNodeEnabled"].includes(message.command));
  assert.deepEqual(commands.map((message) => message.command), ["renameNode", "moveNode", "moveNode", "setNodeEnabled"]);
  assert.deepEqual([commands[0].id, commands[0].name], ["N1", "Hero"], "rename retains stable ID");
  assert.deepEqual([commands[1].x, commands[1].y], [27, 24]);
  assert.deepEqual([commands[2].x, commands[2].y], [27, 35]);
  assert.equal(commands[3].enabled, false);
  webview.elements.get("deleteNode").fire("click");
  assert.ok(webview.messages.some((message) => message.command === "deleteNode" && message.id === "N1"));
  assert.equal(webview.evaluate("selectedNodeId"), null);
});

test("Node Script property follows selection/map changes and posts choose/clear commands without executing scripts", async () => {
  const webview = await readyNodeWebview([
    { id: "N1", name: "Player", x: 16, y: 24, enabled: true, script: "scripts/missing.b8" },
    { id: "N2", name: "Door", x: 32, y: 32, enabled: false },
  ]);
  webview.elements.get("nodeTool").fire("click");
  webview.elements.get("nodeList").children[0].fire("click");
  const scriptValue = webview.elements.get("nodeScriptValue");
  const clear = webview.elements.get("clearNodeScript");
  assert.equal(scriptValue.textContent, "scripts/missing.b8", "missing persisted paths remain visible rather than being cleared");
  assert.equal(clear.hidden, false);
  webview.elements.get("chooseNodeScript").fire("click");
  clear.fire("click");
  assert.ok(webview.messages.some((message) => message.command === "chooseNodeScript" && message.id === "N1"));
  assert.ok(webview.messages.some((message) => message.command === "clearNodeScript" && message.id === "N1"));

  webview.elements.get("nodeList").children[1].fire("click");
  assert.equal(scriptValue.textContent, "None", "selection updates an unassigned Node to None");
  assert.equal(clear.hidden, true);
  webview.dispatchMessage({
    type: "model", model: { ...emptyMap(8, 8), nodes: [{ id: "N2", name: "East Node", x: 8, y: 8, enabled: true, script: "east.b8" }] },
    tilesheets: [{ group: "A", file: "tilesheet.png", imageUri: "webview://A.png", cells: [{ id: "A1", x: 0, y: 0 }] }],
    groups: [{ group: "A", file: "tilesheet" }], projectMaps: ["east.b8map"], currentMap: "east.b8map",
  });
  await webview.flush();
  assert.equal(webview.evaluate("selectedNodeId"), null, "map switching clears the prior selection");
  assert.equal(scriptValue.textContent, "None", "the prior map's script is not left in the properties UI");
  webview.elements.get("nodeList").children[0].fire("click");
  assert.equal(scriptValue.textContent, "east.b8", "selecting a Node on the new map displays its persisted script");
});

test("Node Script assignment and clear are serialized through the normal WorkspaceEdit document path", () => {
  const source = require("node:fs").readFileSync(require("node:path").join(__dirname, "../src/map_editor.ts"), "utf8");
  assert.match(source, /message\.command === "chooseNodeScript"[\s\S]*queueNodeEdit\(\{ type: "setScript", id: message\.id as string, path \}\)/);
  assert.match(source, /if \(message\.command === "clearNodeScript"\) return \{ type: "setScript", id: message\.id, path: null \}/);
  assert.match(source, /const queueNodeEdit = \(action: MapNodeEdit\)[\s\S]*new vscode\.WorkspaceEdit\(\)[\s\S]*workspaceEdit\.replace\([\s\S]*vscode\.workspace\.applyEdit\(workspaceEdit\)/);
  assert.match(source, /message\.command === "setNodeCollider"[\s\S]*type: "setCollider"/);
  assert.match(source, /"setNodeCollider"[\s\S]*mapNodeAction\(message\)/);
  assert.match(source, /message\.command === "setNodeVisual"[\s\S]*type: "setVisual"/);
  assert.match(source, /"setNodeVisual"[\s\S]*mapNodeAction\(message\)/);
  assert.match(source, /const queueNodeEdit = \(action: MapNodeEdit\)[\s\S]*new vscode\.WorkspaceEdit\(\)/, "visual edits use the same undoable document edit path");
  assert.match(source, /findFiles\(new vscode\.RelativePattern\(folder, "\*\*\/\*\.b8"\)\)/);
  assert.match(source, /fs\.realpath\(projectUri\.fsPath\)[\s\S]*fs\.realpath\(uri\.fsPath\)/);
  assert.doesNotMatch(source, /writeFileSync|writeFile\(/, "assignment must edit the VS Code document instead of writing the map file directly");
});

test("Solid tile metadata is shown in properties/palette and overlays map cells without changing the tile ID", async () => {
  const webview = launchWebview(generatedScript().script);
  const sheet = {
    group: "B", file: "dungeon (B).png", imageUri: "webview://B.png",
    cells: [{ id: "B6", x: 0, y: 0, solid: true }],
  };
  const model = { ...emptyMap(1, 1), cells: [["B6"]], nodes: [] };
  webview.dispatchMessage({ type: "model", model, tilesheets: [sheet], groups: [{ group: "B", file: "dungeon" }], projectMaps: ["world.b8map"], currentMap: "world.b8map" });
  await webview.flush();
  assert.equal(webview.elements.get("selectedTileId").textContent, "B6");
  assert.equal(webview.elements.get("tileSolid").checked, true);
  assert.equal(webview.elements.get("tiles").children[0].children[1].textContent, "B6 · Solid");
  const overlay = webview.cameraFrames.find((frame) => frame.color === "rgba(255, 196, 0, .9)");
  assert.ok(overlay, "the authoritative Solid cell receives a subtle editor-only outline");
  webview.evaluate("zoomViewport(2, { x: 120, y: 80 }); panViewport(13, -7); drawMap()");
  const transformedOverlay = webview.cameraFrames.find((frame) => frame.color === "rgba(255, 196, 0, .9)");
  const expectedOverlay = JSON.parse(webview.evaluate("JSON.stringify(mapPoint({ x: 0, y: 0 }))"));
  assert.deepEqual([transformedOverlay.x, transformedOverlay.y], [expectedOverlay.x + 1, expectedOverlay.y + 1], "Solid visualization follows the shared pan/zoom transform");
  webview.elements.get("tileSolid").checked = false;
  webview.elements.get("tileSolid").fire("change");
  assert.ok(webview.messages.some((message) => message.command === "setTileSolid" && message.id === "B6" && message.solid === false));
  assert.deepEqual(model.cells, [["B6"]], "metadata edits do not rewrite map cell identity");
});

test("enabled Box Collider renders through map pan/zoom transform, follows Node drag, and edits through messages", async () => {
  const collider = { enabled: true, offset_x: -2, offset_y: 3, width: 16, height: 12 };
  const webview = await readyNodeWebview([{ id: "N1", name: "Player", x: 16, y: 24, enabled: true, collider }]);
  webview.evaluate("setMode('node'); selectNode('N1')");
  assert.equal(webview.elements.get("nodeColliderEnabled").checked, true);
  assert.equal(webview.elements.get("colliderFields").hidden, false);
  assert.equal(webview.elements.get("colliderWidth").value, "16");
  let rectangle = webview.cameraFrames.find((frame) => frame.color === "rgba(255, 196, 0, .95)");
  assert.deepEqual([rectangle.x, rectangle.y, rectangle.width, rectangle.height], [256, 260, 128, 96]);
  webview.evaluate("nodeDrag = { id: 'N1', x: 24, y: 32 }; drawMap()");
  rectangle = webview.cameraFrames.find((frame) => frame.color === "rgba(255, 196, 0, .95)");
  assert.deepEqual([rectangle.x, rectangle.y], [320, 324], "the collider follows dragged Node coordinates with its offset");
  webview.evaluate("zoomViewport(2, { x: 120, y: 80 }); panViewport(13, -7); drawMap()");
  rectangle = webview.cameraFrames.find((frame) => frame.color === "rgba(255, 196, 0, .95)");
  const expectedCollider = JSON.parse(webview.evaluate("JSON.stringify(mapPoint({ x: (nodeDrag.x - 2) / 8, y: (nodeDrag.y + 3) / 8 }))"));
  assert.deepEqual([rectangle.x, rectangle.y], [expectedCollider.x, expectedCollider.y], "Box Collider uses the same pan/zoom transform as the map");
  webview.elements.get("colliderOffsetX").value = "4";
  webview.elements.get("colliderOffsetX").fire("change");
  assert.ok(webview.messages.some((message) => message.command === "setNodeCollider" && message.id === "N1" && message.collider.offset_x === 4));
  webview.elements.get("nodeColliderEnabled").checked = false;
  webview.elements.get("nodeColliderEnabled").fire("change");
  assert.ok(webview.messages.some((message) => message.command === "setNodeCollider" && message.collider.enabled === false));
});

test("Box Collider game-pixel bounds map to exact tile sizes and offsets at multiple zoom levels", async () => {
  const nodes = [
    { id: "N1", name: "One Cell", x: 32, y: 48, enabled: true, collider: { enabled: true, offset_x: 0, offset_y: 0, width: 8, height: 8 } },
    { id: "N2", name: "Two Cells", x: 32, y: 48, enabled: true, collider: { enabled: true, offset_x: 0, offset_y: 0, width: 16, height: 8 } },
    { id: "N3", name: "Offset X", x: 32, y: 48, enabled: true, collider: { enabled: true, offset_x: 8, offset_y: 0, width: 8, height: 8 } },
    { id: "N4", name: "Offset Y", x: 32, y: 48, enabled: true, collider: { enabled: true, offset_x: 0, offset_y: 8, width: 8, height: 8 } },
  ];
  const webview = await readyNodeWebview(nodes);
  for (const zoom of [0.5, 1, 2]) {
    webview.evaluate(`zoomViewport(${zoom}, { x: 120, y: 80 }); drawMap()`);
    const expected = JSON.parse(webview.evaluate("JSON.stringify({ origin: mapGamePixelPoint({ x: 32, y: 48 }), unit: renderedTileSize() / 8 })"));
    const rectangles = webview.cameraFrames.filter((frame) => frame.color === "rgba(255, 196, 0, .95)");
    assert.equal(rectangles.length, 4);
    assert.deepEqual(rectangles.map(({ x, y, width, height }) => [x, y, width, height]), [
      [expected.origin.x, expected.origin.y, 8 * expected.unit, 8 * expected.unit],
      [expected.origin.x, expected.origin.y, 16 * expected.unit, 8 * expected.unit],
      [expected.origin.x + 8 * expected.unit, expected.origin.y, 8 * expected.unit, 8 * expected.unit],
      [expected.origin.x, expected.origin.y + 8 * expected.unit, 8 * expected.unit, 8 * expected.unit],
    ], `game-pixel bounds should map once at zoom ${zoom}`);
    assert.deepEqual([rectangles[0].width / renderedTileCss(zoom), rectangles[0].height / renderedTileCss(zoom)], [1, 1], "8×8 game pixels occupy exactly one map tile cell");
  }
});

test("tilesheet selector filters cells by stable group and synchronizes the selected tile control", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const sheet = (group, file, prefix) => ({
    group, file, imageUri: `webview://${group}.png`,
    cells: [{ id: `${prefix}1`, x: 0, y: 0 }, { id: `${prefix}2`, x: 1, y: 0 }],
  });
  const sheets = [sheet("A", "tilesheet (A).png", "A"), sheet("B", "dungeon (B).png", "B")];
  webview.dispatchMessage({
    type: "model", model: emptyMap(1, 1), tilesheets: sheets,
    groups: [{ group: "A", file: "tilesheet" }, { group: "B", file: "dungeon" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();

  const picker = webview.elements.get("tilesheetPicker");
  const selected = webview.elements.get("selectedTilePicker");
  const palette = webview.elements.get("tiles");
  assert.deepEqual(picker.children.map((option) => option.textContent), ["A · tilesheet", "B · dungeon"]);
  assert.equal(picker.disabled, false);
  assert.deepEqual(palette.children.map((button) => button.dataset.id), ["A1", "A2"]);
  assert.deepEqual(selected.children.map((option) => option.value), ["A1", "A2"]);
  assert.equal(selected.value, "A1");

  picker.value = "B"; picker.fire("change");
  assert.deepEqual(palette.children.map((button) => button.dataset.id), ["B1", "B2"]);
  assert.deepEqual(selected.children.map((option) => option.value), ["B1", "B2"]);
  assert.equal(selected.value, "B1", "switching groups chooses that group's first tile");

  palette.children[1].fire("click");
  assert.equal(selected.value, "B2", "palette selection updates the top tile selector");
  selected.value = "B1"; selected.fire("change");
  assert.equal(palette.children[0].classList.contains("selected"), true, "top selector updates palette highlight");
  assert.equal(palette.children[1].classList.contains("selected"), false);

  picker.value = "A"; picker.fire("change");
  assert.deepEqual(palette.children.map((button) => button.dataset.id), ["A1", "A2"]);
  assert.equal(selected.value, "A1", "an invalid previous group tile falls back to the first tile");
});

test("asset refresh adds groups while preserving selection, map edits, pan, zoom, tool and selected Node", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const sheet = (group, file, count = 2) => ({
    group, file, width: count * 8, height: 8, columns: count, rows: 1,
    palette: [0x000000, 0x1d2b53, 0x7e2553, 0x008751, 0xab5236, 0x5f574f, 0xc2c3c7, 0xfff1e8,
      0xff004d, 0xffa300, 0xffec27, 0x00e436, 0x29adff, 0x83769c, 0xff77a8, 0xffccaa],
    cells: Array.from({ length: count }, (_, index) => ({ id: `${group}${index + 1}`, x: index, y: 0, pixels: Array(64).fill(13) })),
  });
  const model = { ...emptyMap(2, 1), cells: [["A1", "B1"]], nodes: [{ id: "N1", type: "Node", name: "Player", x: 0, y: 0, enabled: true }] };
  webview.dispatchMessage({
    type: "model", model, tilesheets: [sheet("A", "a.png"), sheet("B", "b.png")],
    groups: [{ group: "A", file: "a" }, { group: "B", file: "b" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();

  webview.elements.get("tilesheetPicker").value = "B";
  webview.elements.get("tilesheetPicker").fire("change");
  webview.elements.get("tiles").children[1].fire("click");
  webview.evaluate("selectNode('N1'); setMode('pan'); zoomViewport(.5, { x: 250, y: 190 }); panViewport(-23, 17)");
  const before = JSON.parse(webview.evaluate("JSON.stringify({ pan, zoom, mode, selectedTile, selectedGroup, selectedNodeId })"));

  const editedModel = { ...model, cells: [["A1", "B1"]] };
  webview.dispatchMessage({
    type: "model", model: editedModel,
    tilesheets: [sheet("A", "a.png"), sheet("B", "b.png"), sheet("C", "new.png", 3)],
    groups: [{ group: "A", file: "a" }, { group: "B", file: "b" }, { group: "C", file: "new" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();

  const after = JSON.parse(webview.evaluate("JSON.stringify({ pan, zoom, mode, selectedTile, selectedGroup, selectedNodeId })"));
  assert.deepEqual(after, before, "asset refresh must preserve active editing state");
  assert.deepEqual(webview.elements.get("tilesheetPicker").children.map((option) => option.value), ["A", "B", "C"]);
  assert.deepEqual(webview.elements.get("tiles").children.map((button) => button.dataset.id), ["B1", "B2"]);
  assert.deepEqual(webview.elements.get("selectedTilePicker").children.map((option) => option.value), ["B1", "B2"]);
  assert.deepEqual(editedModel.cells, [["A1", "B1"]], "refresh does not rewrite the current dirty map model");

  const newPaletteImage = webview.imageDataCalls.at(-1).data;
  assert.deepEqual(Array.from(newPaletteImage.data.slice(0, 4)), [0x83, 0x76, 0x9c, 255], "new sheet previews retain Runtime palette-mapped pixels");
});

test("asset refresh uses the first remaining stable group when the selected group disappears", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const sheet = (group) => ({
    group, file: `${group}.png`,
    cells: [{ id: `${group}1`, x: 0, y: 0 }],
  });
  webview.dispatchMessage({
    type: "model", model: emptyMap(1, 1), tilesheets: [sheet("A"), sheet("B")],
    groups: [{ group: "A", file: "A" }, { group: "B", file: "B" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();
  webview.evaluate("showTilesheetGroup('B')");
  webview.dispatchMessage({
    type: "model", model: emptyMap(1, 1), tilesheets: [sheet("A"), sheet("C")],
    groups: [{ group: "A", file: "A" }, { group: "C", file: "C" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();
  assert.equal(webview.elements.get("tilesheetPicker").value, "A");
  assert.equal(webview.evaluate("selectedGroup"), "A");
  assert.equal(webview.evaluate("selectedTile"), "A1");
  assert.equal(webview.elements.get("selectedTilePicker").value, "A1");
});

test("an asset refresh error leaves the prior map and palette usable", async () => {
  const webview = await readyMapWebview();
  webview.dispatchMessage({ type: "error", message: "temporary registry inspection failure" });
  assert.equal(webview.elements.get("error").hidden, false);
  assert.equal(webview.evaluate("model.cells[0][0]"), "A1");
  assert.deepEqual(webview.elements.get("tiles").children.map((button) => button.dataset.id), ["A1"]);
});

test("32x32 registered sheet exposes all 16 row-major cell previews and paints the selected stable ID", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const cells = Array.from({ length: 16 }, (_, index) => ({
    id: `A${index + 1}`,
    x: index % 4,
    y: Math.floor(index / 4),
  }));
  webview.dispatchMessage({
    type: "model",
    model: emptyMap(1, 1),
    tilesheets: [{
      group: "A", file: "tilesheet (A).png", imageUri: "webview://A.png",
      width: 32, height: 32, columns: 4, rows: 4, cells,
    }],
    groups: [{ group: "A", file: "tilesheet" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();

  const palette = webview.elements.get("tiles");
  const selected = webview.elements.get("selectedTilePicker");
  assert.deepEqual(palette.children.map((button) => button.dataset.id), cells.map((cell) => cell.id));
  assert.deepEqual(
    webview.drawImageCalls.slice(0, 16).map((args) => args.slice(1, 5)),
    cells.map((cell) => [cell.x * 8, cell.y * 8, 8, 8]),
    "each palette canvas samples precisely its row-major 8x8 source rectangle",
  );

  palette.children[15].fire("click");
  assert.equal(selected.value, "A16");
  const viewport = webview.elements.get("viewport");
  viewport.fire("pointerdown", { button: 0, pointerId: 41, clientX: 400, clientY: 300 });
  assert.ok(webview.messages.some((message) =>
    message.command === "paint" && message.x === 0 && message.y === 0 && message.tile === "A16",
  ));
});

test("tile previews and map canvas use Runtime-mapped palette colors with transparency", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  const palette = [0x000000, 0x1d2b53, 0x7e2553, 0x008751, 0xab5236, 0x5f574f, 0xc2c3c7, 0xfff1e8,
    0xff004d, 0xffa300, 0xffec27, 0x00e436, 0x29adff, 0x83769c, 0xff77a8, 0xffccaa];
  const sky = Array(64).fill(12);
  sky[0] = 0;
  const pink = Array(64).fill(14);
  const mappedSheet = {
    group: "A", file: "sky (A).png", width: 16, height: 8, columns: 2, rows: 1, palette,
    cells: [{ id: "A1", x: 0, y: 0, pixels: sky }, { id: "A2", x: 1, y: 0, pixels: pink }],
  };
  webview.dispatchMessage({
    type: "model", model: { ...emptyMap(2, 1), cells: [["A1", "A2"]] },
    tilesheets: [mappedSheet], groups: [{ group: "A", file: "sky" }],
    projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();

  const mappedCanvas = webview.drawImageCalls[0][0];
  assert.equal(mappedCanvas.width, 16);
  assert.equal(mappedCanvas.height, 8);
  const rgba = webview.imageDataCalls[0].data.data;
  assert.deepEqual(Array.from(rgba.slice(0, 4)), [0, 0, 0, 0], "palette index 0 stays transparent");
  assert.deepEqual(Array.from(rgba.slice(4, 8)), [41, 173, 255, 255], "off-palette sky blue uses Rust Runtime cyan-blue");
  assert.deepEqual(Array.from(rgba.slice(8 * 4, 8 * 4 + 4)), [255, 119, 168, 255], "a distinct mapped cell color is retained");

  const mapDraws = webview.drawImageCalls.slice(-2);
  assert.ok(mapDraws.every((call) => call[0] === mappedCanvas), "map tiles and palette previews share the mapped source canvas");
  assert.deepEqual(mapDraws.map((call) => call.slice(1, 5)), [[0, 0, 8, 8], [8, 0, 8, 8]]);
});

test("a one-group project shows its group and uses a valid first tile", async () => {
  const { script } = generatedScript();
  const webview = launchWebview(script);
  webview.dispatchMessage({
    type: "model", model: emptyMap(1, 1),
    tilesheets: [{ group: "A", file: "tilesheet (A).png", imageUri: "webview://A.png", cells: [{ id: "A1", x: 0, y: 0 }] }],
    groups: [{ group: "A", file: "tilesheet" }], projectMaps: ["world.b8map"], currentMap: "world.b8map",
  });
  await webview.flush();
  assert.equal(webview.elements.get("tilesheetPicker").value, "A");
  assert.equal(webview.elements.get("tilesheetPicker").disabled, true);
  assert.equal(webview.elements.get("selectedTilePicker").value, "A1");
  assert.deepEqual(webview.elements.get("tiles").children.map((button) => button.dataset.id), ["A1"]);
});
