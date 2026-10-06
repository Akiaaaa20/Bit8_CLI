const assert = require("node:assert/strict");
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { test } = require("node:test");
const { generatedTilesheetPng } = require("./fixtures/generated_tilesheet");
const {
  MAP_TILE_CSS_SIZE,
  CAMERA_VIEWPORT_HEIGHT,
  CAMERA_VIEWPORT_WIDTH,
  cameraViewportBounds,
  cameraViewportRect,
  initialMapZoom,
  editMapNode,
  mapCellAtViewportPoint,
  mapCellTokenRange,
  mapDisplaySize,
  mapGridLines,
  mapPickerPaths,
  normalizeNodeScriptPath,
  parseMapDocument,
  replaceMapCell,
  selectedTileForGroup,
  snapCameraCoordinate,
  snapNodeCoordinate,
  tilesheetGroupOptions,
  zoomMapAt,
} = require("../out/map_editor_model");

test("editor Camera snapping uses nearest tile centers without migrating existing positions", () => {
  assert.equal(snapCameraCoordinate(4), 4);
  assert.equal(snapCameraCoordinate(12), 12);
  for (const [pointer, expected] of [[1, 4], [10, 12], [15, 12], [16, 20], [35, 36], [-9, -12]]) {
    assert.equal(snapCameraCoordinate(pointer), expected);
  }
  for (const value of [0, 8, 16, 32]) assert.equal(snapNodeCoordinate(value), value);
  const original = 'version = 1\nwidth = 1\nheight = 1\n--\n[node_state]\nnext_id = 2\n' +
    '[[node_state.nodes]]\nid = "N1"\ntype = "Camera"\nname = "View"\nx = 32\ny = 32\nenabled = true\n';
  const ids = new Set();
  assert.deepEqual([parseMapDocument(original, ids).nodes[0].x, parseMapDocument(original, ids).nodes[0].y], [32, 32]);
  const renamed = editMapNode(original, { type: "rename", id: "N1", name: "Renamed" });
  const renamedText = original.slice(0, renamed.startOffset) + renamed.replacement;
  assert.deepEqual([parseMapDocument(renamedText, ids).nodes[0].x, parseMapDocument(renamedText, ids).nodes[0].y], [32, 32]);
  const move = editMapNode(original, { type: "move", id: "N1", x: 35, y: 25 });
  const persisted = parseMapDocument(original.slice(0, move.startOffset) + move.replacement, ids).nodes[0];
  assert.deepEqual([persisted.x, persisted.y], [36, 28]);
  for (const [cell, expected] of [[{ x: 0, y: 0 }, [4, 4]], [{ x: 1, y: 0 }, [12, 4]]]) {
    const added = editMapNode(original, { type: "add", nodeType: "Camera", cell });
    const node = parseMapDocument(original.slice(0, added.startOffset) + added.replacement, ids).nodes[1];
    assert.deepEqual([node.x, node.y], expected);
  }
});

const assetIds = new Set(["A1", "A2", "A3", "A4", "B1", "B3", "B6", "B7", "B9", "B17", "B18", "B19", "B20", "B28", "C1", "C2", "C3", "C4"]);
const validMap = "version = 1\nwidth = 3\nheight = 2\n\nA1 -- A4\nB1 A1 --\n";

test("parses empty cells and stable IDs across groups", () => {
  const model = parseMapDocument(validMap, assetIds);
  assert.deepEqual(model.cells, [["A1", "--", "A4"], ["B1", "A1", "--"]]);
  assert.equal(model.width, 3);
  assert.equal(model.height, 2);
});

test("paint and erase replace only a token and preserve map dimensions", () => {
  const painted = replaceMapCell(validMap, 1, 0, "B1", assetIds);
  const erased = replaceMapCell(painted, 0, 1, null, assetIds);
  const model = parseMapDocument(erased, assetIds);
  assert.match(erased, /width = 3\nheight = 2/);
  assert.deepEqual(model.cells, [["A1", "B1", "A4"], ["--", "A1", "--"]]);
  assert.deepEqual(mapCellTokenRange(validMap, 2, 0, assetIds), {
    line: 4,
    startCharacter: 6,
    endCharacter: 8,
  });
});

test("Map Workspace tile parsing leaves the trailing Rust node section untouched", () => {
  const source = `${validMap}\n[node_state]\nnext_id = 2\n\n[[node_state.nodes]]\nid = \"N1\"\nname = \"Player\"\nx = 256\ny = 224\nenabled = true\n`;
  const model = parseMapDocument(source, assetIds);
  assert.deepEqual(model.cells, [["A1", "--", "A4"], ["B1", "A1", "--"]]);
  const updated = replaceMapCell(source, 1, 0, "B1", assetIds);
  assert.match(updated, /\[node_state\]\nnext_id = 2/);
  assert.match(updated, /id = "N1"\nname = "Player"\nx = 256\ny = 224\nenabled = true/);
  assert.deepEqual(parseMapDocument(updated, assetIds).cells[0], ["A1", "B1", "A4"]);
});

test("node model parses by stable ID order and edits only node data while preserving scripts and reserved fields", () => {
  const source = `${validMap}\n[node_state]\nnext_id = 4\n\n` +
    `[[node_state.nodes]]\nid = "N3"\nname = "Door"\nx = 16\ny = 24\nenabled = false\nscript = "door.b8"\nfuture = "keep exactly"\n\n` +
    `[[node_state.nodes]]\nid = "N1"\nname = "Player"\nx = 256\ny = 224\nenabled = true\nscript = "player.b8"\n`;
  const model = parseMapDocument(source, assetIds);
  assert.deepEqual(model.nodes.map(({ id, type }) => [id, type]), [["N1", "Node"], ["N3", "Node"]], "missing type defaults to Node");
  assert.deepEqual(model.nodes.map(({ id, name }) => [id, name]), [["N1", "Player"], ["N3", "Door"]]);
  assert.equal(model.nodes[0].script, "player.b8");

  const moved = editMapNode(source, { type: "move", id: "N3", x: 259, y: 31 });
  const movedText = source.slice(0, moved.startOffset) + moved.replacement;
  const movedModel = parseMapDocument(movedText, assetIds);
  assert.deepEqual([movedModel.nodes[1].x, movedModel.nodes[1].y], [256, 32]);
  assert.match(movedText, /script = "door\.b8"\nfuture = "keep exactly"/);
  assert.match(movedText, /script = "player\.b8"/);
  assert.deepEqual(movedModel.cells, [["A1", "--", "A4"], ["B1", "A1", "--"]]);

  const renamed = editMapNode(movedText, { type: "rename", id: "N3", name: "Gate" });
  const renamedText = movedText.slice(0, renamed.startOffset) + renamed.replacement;
  assert.deepEqual(parseMapDocument(renamedText, assetIds).nodes.map((node) => [node.id, node.name]), [["N1", "Player"], ["N3", "Gate"]]);
  assert.throws(() => editMapNode(renamedText, { type: "rename", id: "N3", name: "Player" }), /already exists/);

  const disabled = editMapNode(renamedText, { type: "setEnabled", id: "N3", enabled: true });
  const disabledText = renamedText.slice(0, disabled.startOffset) + disabled.replacement;
  assert.equal(parseMapDocument(disabledText, assetIds).nodes[1].enabled, true);
});

test("optional Box Collider parses, validates, survives Node edits, and remains reversible", () => {
  const source = `${validMap}\n[node_state]\nnext_id = 2\n\n` +
    `[[node_state.nodes]]\nid = "N1"\nname = "Player"\nx = 16\ny = 24\nenabled = true\nscript = "player.b8"\n`;
  assert.equal(parseMapDocument(source, assetIds).nodes[0].collider, undefined, "old maps have no collider by default");
  const collider = { enabled: true, offset_x: -2, offset_y: 3, width: 16, height: 12 };
  const apply = (text, action) => {
    const edit = editMapNode(text, action);
    return text.slice(0, edit.startOffset) + edit.replacement;
  };
  const withCollider = apply(source, { type: "setCollider", id: "N1", collider });
  assert.match(withCollider, /\[node_state\.nodes\.collider\]\nenabled = true\noffset_x = -2\noffset_y = 3\nwidth = 16\nheight = 12/);
  assert.deepEqual(parseMapDocument(withCollider, assetIds).nodes[0].collider, collider);
  const renamed = apply(withCollider, { type: "rename", id: "N1", name: "Hero" });
  const moved = apply(renamed, { type: "move", id: "N1", x: 32, y: 40 });
  assert.deepEqual(parseMapDocument(moved, assetIds).nodes[0], {
    id: "N1", type: "Node", name: "Hero", x: 32, y: 40, enabled: true, script: "player.b8", collider,
  });
  const disabled = apply(moved, { type: "setCollider", id: "N1", collider: { ...collider, enabled: false } });
  assert.equal(parseMapDocument(disabled, assetIds).nodes[0].collider.enabled, false);
  assert.throws(() => editMapNode(source, { type: "setCollider", id: "N1", collider: { ...collider, width: 0 } }), /width must be a positive integer/);
  const deleted = apply(withCollider, { type: "delete", id: "N1" });
  assert.deepEqual(parseMapDocument(deleted, assetIds).nodes, []);
  assert.equal(source.includes("[node_state.nodes.collider]"), false, "the source remains available unchanged for document undo/revert");
});

test("first script assignment after a collider stays at Node scope and preserves all other data", () => {
  const {source}=require('./node_visual_fixture');
  const original=source.replace('script = "player.b8"\n','');
  const ids=new Set(['A1','A2']);
  const apply=(text,path)=>{const edit=editMapNode(text,{type:'setScript',id:'N1',path});return text.slice(0,edit.startOffset)+edit.replacement;};
  const before=parseMapDocument(original,ids);
  const assigned=apply(original,'player.b8');
  assert.ok(assigned.indexOf('script = "player.b8"')<assigned.indexOf('[node_state.nodes.collider]'));
  const parsed=parseMapDocument(assigned,ids);
  assert.equal(parsed.nodes[0].script,'player.b8');
  assert.deepEqual(parsed.nodes[0].collider,before.nodes[0].collider);
  assert.deepEqual(parsed.nodes[1],before.nodes[1]);
  assert.deepEqual(parsed.cells,before.cells);
  const replaced=apply(assigned,'enemy.b8');
  assert.equal(parseMapDocument(replaced,ids).nodes[0].script,'enemy.b8');
  assert.equal(apply(replaced,null),original);
});

test("Node visual ID persists through edits, tolerates missing assets, and clears independently of collider data", () => {
  let source = `${validMap}\n[node_state]\nnext_id = 2\n\n` +
    `[[node_state.nodes]]\nid = "N1"\nname = "Player"\nx = 16\ny = 24\nenabled = true\n` +
    `visual = "A99"\n\n[node_state.nodes.collider]\nenabled = true\noffset_x = 0\noffset_y = 0\nwidth = 8\nheight = 8\n`;
  const apply = (action) => {
    const edit = editMapNode(source, action);
    source = source.slice(0, edit.startOffset) + edit.replacement;
    return parseMapDocument(source, assetIds).nodes[0];
  };
  assert.equal(parseMapDocument(source, assetIds).nodes[0].visual, "A99", "unknown/out-of-range visual IDs remain loadable for missing-state UI");
  const moved = apply({ type: "move", id: "N1", x: 24, y: 32 });
  assert.equal(moved.visual, "A99");
  assert.match(source, /visual = "A99"\n\n\[node_state\.nodes\.collider\]/, "visual remains top-level before the collider table");

  const assigned = apply({ type: "setVisual", id: "N1", visual: "B1" });
  assert.equal(assigned.visual, "B1");
  assert.equal(assigned.collider.width, 8);
  assert.match(source, /visual = "B1"[\s\S]*\[node_state\.nodes\.collider\]/);
  const cleared = apply({ type: "setVisual", id: "N1", visual: null });
  assert.equal(cleared.visual, undefined);
  assert.equal(cleared.collider.enabled, true);
  assert.doesNotMatch(source, /^visual\s*=/m);
  assert.match(source, /\[node_state\.nodes\.collider\]/);
});

test("Add Node allocates monotonic IDs, unique names, snapped positions, and deterministic fallback; delete never reuses IDs", () => {
  let source = `${validMap}\n[node_state]\nnext_id = 3\n\n` +
    `[[node_state.nodes]]\nid = "N1"\nname = "Node"\nx = 0\ny = 0\nenabled = true\n\n` +
    `[[node_state.nodes]]\nid = "N2"\nname = "Node2"\nx = 8\ny = 8\nenabled = true\n`;
  const apply = (text, action) => {
    const edit = editMapNode(text, action);
    return { text: text.slice(0, edit.startOffset) + edit.replacement, edit };
  };
  let result = apply(source, { type: "add", cell: { x: 32, y: 28 } });
  source = result.text;
  assert.equal(result.edit.selectedNodeId, "N3");
  assert.deepEqual(parseMapDocument(source, assetIds).nodes.at(-1), {
    id: "N3", type: "Node", name: "Node3", x: 256, y: 224, enabled: true,
  });
  assert.match(source, /id = "N3"\ntype = "Node"/);
  assert.match(source, /next_id = 4/);

  result = apply(source, { type: "delete", id: "N3" });
  source = result.text;
  assert.deepEqual(parseMapDocument(source, assetIds).nodes.map((node) => node.id), ["N1", "N2"]);
  assert.match(source, /next_id = 4/);
  result = apply(source, { type: "add" });
  assert.equal(result.edit.selectedNodeId, "N4");
  const added = parseMapDocument(result.text, assetIds).nodes.at(-1);
  assert.deepEqual([added.id, added.name, added.x, added.y, added.enabled], ["N4", "Node3", 0, 0, true]);
  assert.equal(added.script, undefined);
});

test("Add Camera uses stable IDs, centered game-pixel coordinates, unique names, and allows multiple cameras", () => {
  let source = `${validMap}\n[node_state]\nnext_id = 2\n\n` +
    `[[node_state.nodes]]\nid = "N1"\nname = "Camera"\nx = 0\ny = 0\nenabled = true\n`;
  const apply = (action) => {
    const edit = editMapNode(source, action);
    source = source.slice(0, edit.startOffset) + edit.replacement;
    return parseMapDocument(source, assetIds).nodes;
  };
  let nodes = apply({ type: "add", nodeType: "Camera", cell: { x: 3, y: 5 } });
  assert.deepEqual(nodes[1], { id: "N2", type: "Camera", name: "Camera2", x: 28, y: 44, enabled: true });
  assert.equal(nodes[1].x, 28, "Camera.x is stored at the tile center");
  assert.match(source, /type = "Camera"\nname = "Camera2"/);
  assert.doesNotMatch(source, /camera_(?:width|height)|viewport_(?:width|height)/i, "fixed viewport size is not persisted as editable fields");
  nodes = apply({ type: "add", nodeType: "Camera" });
  assert.deepEqual([nodes[2].id, nodes[2].type, nodes[2].name, nodes[2].x, nodes[2].y], ["N3", "Camera", "Camera3", 4, 4]);
  assert.equal(parseMapDocument(source, assetIds).nodes.filter((node) => node.type === "Camera").length, 2);
  const deleted = editMapNode(source, { type: "delete", id: "N2" });
  source = source.slice(0, deleted.startOffset) + deleted.replacement;
  nodes = parseMapDocument(source, assetIds).nodes;
  assert.deepEqual(nodes.map((node) => node.id), ["N1", "N3"]);
  assert.match(source, /next_id = 4/);
});

test("Camera viewport bounds are centered, fixed at 64 game pixels, and never clamped to map edges", () => {
  assert.deepEqual([CAMERA_VIEWPORT_WIDTH, CAMERA_VIEWPORT_HEIGHT], [64, 64]);
  assert.deepEqual(cameraViewportBounds(256, 256), { left: 224, top: 224, right: 288, bottom: 288 });
  assert.deepEqual(cameraViewportBounds(8, 8), { left: -24, top: -24, right: 40, bottom: 40 });
  assert.deepEqual(cameraViewportRect(256, 256, { x: 13, y: 27 }, 1, 64), {
    x: 1805, y: 1819, width: 512, height: 512,
  });
  assert.deepEqual(cameraViewportRect(8, 8, { x: 10, y: 20 }, 0.5, 64), {
    x: -86, y: -76, width: 256, height: 256,
  });
});

test("Node type defaults old records to Node, preserves explicit Camera identity, and rejects unknown values", () => {
  const source = `${validMap}\n[node_state]\nnext_id = 3\n\n` +
    `[[node_state.nodes]]\nid = "N1"\nname = "Camera"\nx = 0\ny = 0\nenabled = true\n\n` +
    `[[node_state.nodes]]\nid = "N2"\ntype = "Camera"\nname = "View"\nx = 8\ny = 8\nenabled = true\n`;
  const model = parseMapDocument(source, assetIds);
  assert.deepEqual(model.nodes.map(({ type, name }) => [type, name]), [["Node", "Camera"], ["Camera", "View"]]);
  const moved = editMapNode(source, { type: "move", id: "N2", x: 16, y: 24 });
  const updated = source.slice(0, moved.startOffset) + moved.replacement;
  assert.match(updated, /type = "Camera"/);
  assert.equal(parseMapDocument(updated, assetIds).nodes[1].type, "Camera");
  assert.throws(() => parseMapDocument(source.replace('type = "Camera"', 'type = "Banana"'), assetIds), /N2.*View.*Banana.*Node or Camera/);
});

test("rename, move, enabled, script assignment and clear preserve Camera type through reparse", () => {
  let source = `${validMap}\n[node_state]\nnext_id = 2\n\n` +
    `[[node_state.nodes]]\nid = "N1"\ntype = "Camera"\nname = "View"\nx = 0\ny = 0\nenabled = true\n`;
  const apply = (action) => {
    const edit = editMapNode(source, action);
    source = source.slice(0, edit.startOffset) + edit.replacement;
    assert.equal(parseMapDocument(source, assetIds).nodes[0].type, "Camera");
  };
  apply({ type: "rename", id: "N1", name: "MainView" });
  apply({ type: "move", id: "N1", x: 24, y: 32 });
  apply({ type: "setEnabled", id: "N1", enabled: false });
  apply({ type: "setScript", id: "N1", path: "camera.b8" });
  apply({ type: "setScript", id: "N1", path: null });
  const reparsed = parseMapDocument(source, assetIds).nodes[0];
  assert.equal(reparsed.name, "MainView");
  assert.equal(reparsed.script, undefined);
  assert.equal(reparsed.enabled, false);
});

test("node script assignment, replacement, and clear preserve all unrelated map and node data", () => {
  const source = `${validMap}\n[node_state]\nnext_id = 9\n\n` +
    `[[node_state.nodes]]\nid = "N1"\nname = "Player"\nx = 16\ny = 24\nenabled = false\nscript = "scripts/missing.b8"\ncustom = "keep"\n\n` +
    `[[node_state.nodes]]\nid = "N8"\nname = "Door"\nx = 40\ny = 48\nenabled = true\n`;
  const apply = (text, action) => {
    const edit = editMapNode(text, action);
    return { text: text.slice(0, edit.startOffset) + edit.replacement, edit };
  };
  const assigned = apply(source, { type: "setScript", id: "N1", path: "scripts\\player.b8" });
  const assignedModel = parseMapDocument(assigned.text, assetIds);
  assert.equal(assignedModel.nodes[0].script, "scripts/player.b8");
  assert.equal(assigned.edit.selectedNodeId, "N1");
  assert.deepEqual(
    [assignedModel.nodes[0].id, assignedModel.nodes[0].name, assignedModel.nodes[0].x, assignedModel.nodes[0].y, assignedModel.nodes[0].enabled],
    ["N1", "Player", 16, 24, false],
  );
  assert.deepEqual(assignedModel.cells, parseMapDocument(source, assetIds).cells);
  assert.deepEqual(assignedModel.nodes[1], parseMapDocument(source, assetIds).nodes[1]);
  assert.match(assigned.text, /next_id = 9/);
  assert.match(assigned.text, /custom = "keep"/);

  const replaced = apply(assigned.text, { type: "setScript", id: "N1", path: "actors/player.b8" });
  assert.equal(parseMapDocument(replaced.text, assetIds).nodes[0].script, "actors/player.b8");
  const cleared = apply(replaced.text, { type: "setScript", id: "N1", path: null });
  assert.equal(parseMapDocument(cleared.text, assetIds).nodes[0].script, undefined);
  assert.equal(parseMapDocument(cleared.text, assetIds).nodes[1].script, undefined);
  assert.match(cleared.text, /custom = "keep"/);
});

test("node script paths accept normalized project-relative .b8 paths and reject unsafe/non-script paths", () => {
  assert.equal(normalizeNodeScriptPath("player.b8"), "player.b8");
  assert.equal(normalizeNodeScriptPath("scripts/./actors/../player.b8"), "scripts/player.b8");
  assert.equal(normalizeNodeScriptPath("scripts\\player.b8"), "scripts/player.b8");
  assert.throws(() => normalizeNodeScriptPath("player.lua"), /\.b8/);
  assert.throws(() => normalizeNodeScriptPath("/outside/player.b8"), /project-relative/);
  assert.throws(() => normalizeNodeScriptPath("../outside/player.b8"), /inside the Bit8 project/);
  assert.throws(() => normalizeNodeScriptPath("file:///outside/player.b8"), /project-relative/);
});

test("map geometry helpers use an explicit display tile size", () => {
  assert.equal(MAP_TILE_CSS_SIZE, 64);
  assert.deepEqual(mapDisplaySize(8, 8, MAP_TILE_CSS_SIZE), { x: 512, y: 512 });
  assert.deepEqual(mapDisplaySize(8, 8, MAP_TILE_CSS_SIZE, 0.5), { x: 256, y: 256 });
  assert.deepEqual(mapDisplaySize(8, 8, MAP_TILE_CSS_SIZE, 2), { x: 1024, y: 1024 });
  assert.equal(initialMapZoom(64, 64), 0.125);
  assert.equal(initialMapZoom(8, 8), 1);
  assert.equal(initialMapZoom(256, 32), 0.125, "larger variable maps respect the existing minimum zoom");
  assert.deepEqual(mapDisplaySize(64, 64, MAP_TILE_CSS_SIZE, initialMapZoom(64, 64)), { x: 512, y: 512 });
  assert.deepEqual(mapDisplaySize(16, 10, 32), { x: 512, y: 320 });
  const grid = mapGridLines(8, 8, { x: 0, y: 0 }, 1, 64);
  assert.equal(grid.vertical.length, 9);
  assert.equal(grid.horizontal.length, 9);
  assert.deepEqual(grid.vertical.slice(0, 3), [0, 64, 128]);
  assert.equal(grid.vertical[8], 512);
  assert.equal(grid.horizontal[8], 512);
  const defaultGrid = mapGridLines(64, 64, { x: 0, y: 0 }, 0.125);
  assert.equal(defaultGrid.vertical.length, 65);
  assert.equal(defaultGrid.horizontal.length, 65);
  assert.equal(defaultGrid.vertical[64], 512);
  assert.equal(defaultGrid.horizontal[64], 512);
  const source = fs.readFileSync(path.join(__dirname, "../src/map_editor.ts"), "utf8");
  assert.match(source, /context\.imageSmoothingEnabled = false/);
  assert.match(source, /context\.drawImage\(image, entry\.cell\.x \* 8, entry\.cell\.y \* 8, 8, 8, px, py, tileSize, tileSize\)/);
  assert.match(source, /imageSmoothingEnabled = false/);
  assert.match(source, /function panViewport\(deltaX, deltaY\)/);
  assert.match(source, /function zoomViewport\(requestedZoom, anchor\)/);
  assert.match(source, /const grid = mapGridGeometry\(model\.width, model\.height, pan, tileSize\)/);
  assert.doesNotMatch(source, /context\.strokeRect\(px \+ \.5, py \+ \.5, 7, 7\)/);
  assert.match(source, /overflow:hidden/);
  assert.match(source, /mapPicker.addEventListener\('change'/);
  assert.match(source, /selectedPicker.addEventListener\('change'/);
});

test("Map Workspace uses a fixed 64 CSS px tile scale and centers only the initial view", () => {
  const source = fs.readFileSync(path.join(__dirname, "../src/map_editor.ts"), "utf8");
  assert.match(source, /const logicalTileCssSize = \$\{MAP_TILE_CSS_SIZE\}/);
  assert.match(source, /const defaultMapViewTileSpan = \$\{DEFAULT_MAP_VIEW_TILE_SPAN\}/);
  assert.match(source, /function renderedTileSize\(\) \{ return logicalTileCssSize \* zoom; \}/);
  assert.doesNotMatch(source, /fitInitialMapView|initialMapOccupancy|baseTileSize/);
  assert.match(source, /zoom = Math\.max\(minimumMapZoom, Math\.min\(1, defaultMapViewTileSpan \/ Math\.max\(model\.width, model\.height\)\)\)/);
  assert.match(source, /pan = \{ x: \(width - model\.width \* tileSize\) \/ 2, y: \(height - model\.height \* tileSize\) \/ 2 \}/);
  assert.match(source, /zoomLabel\.textContent = Math\.round\(zoom \* 1000\) \/ 10 \+ '%'/);
  assert.match(source, /const resizeObserver = new ResizeObserver\(\(\) => \{\s*initializeMapView\(\);/);
  assert.match(source, /context\.moveTo\(px, pan\.y\); context\.lineTo\(px, mapBottom\)/);
  assert.match(source, /context\.moveTo\(pan\.x, py\); context\.lineTo\(mapRight, py\)/);
});

test("official demo world map is 64x64 with no retired-group cells and retains its assigned node script", () => {
  const repository = path.resolve(__dirname, "..", "..");
  const source = fs.readFileSync(path.join(repository, "games/tilesheet_demo", "world.b8map"), "utf8");
  const map = parseMapDocument(source, assetIds);
  assert.deepEqual([map.width, map.height, map.cells.length * map.width], [64, 64, 4096], "world map logical dimensions");
  assert.deepEqual([map.width * 8, map.height * 8], [512, 512], "world map game-pixel dimensions");
  assert.deepEqual(
    new Set(map.cells.flat().filter((cell) => cell !== "--")),
    new Set(),
  );
  assert.deepEqual(
    map.nodes.map(({ id, type, name, script }) => [id, type, name, script]),
    [["N1", "Node", "Player", "player.b8"], ["N2", "Camera", "Camera", "camera.b8"]],
  );
});

test("generated 64x64 map preserves multi-cell registered IDs and assigned nodes", () => {
  const repository = path.resolve(__dirname, "..", "..");
  const source = fs.readFileSync(path.join(repository, "games/tilesheet_demo/world.b8map"), "utf8");
  const ids = ["B7", "B9", "B17", "B18", "B19", "B20", "B28"];
  let generated = source;
  for (const [x, id] of ids.entries()) generated = replaceMapCell(generated, x, 0, id, assetIds);
  const map = parseMapDocument(generated, assetIds);
  assert.deepEqual([map.width, map.height, map.cells.flat().length], [64, 64, 4096]);
  assert.deepEqual(map.cells[0].slice(0, ids.length), ids);
  assert.deepEqual(new Set(map.cells.flat().filter(cell => cell !== "--")), new Set(ids));
  assert.deepEqual(map.nodes, parseMapDocument(source, assetIds).nodes);
});

test("tilesheet selector groups are stable, palette cells filter by group, and selection stays valid", () => {
  const sheets = [
    { group: "C", file: "art/town (C).png", cells: [{ id: "C1" }, { id: "C2" }] },
    { group: "A", file: "tilesheet (A).png", cells: [{ id: "A1" }, { id: "A3" }] },
    { group: "B", file: "dungeon (B).png", cells: [{ id: "B1" }, { id: "B2" }] },
  ];
  assert.deepEqual(tilesheetGroupOptions(sheets), [
    { group: "A", file: "tilesheet" },
    { group: "B", file: "dungeon" },
    { group: "C", file: "town" },
  ]);
  const visibleCells = (group) => sheets.find((sheet) => sheet.group === group).cells.map((cell) => cell.id);
  assert.deepEqual(visibleCells("B"), ["B1", "B2"]);
  assert.equal(selectedTileForGroup("A3", sheets.find((sheet) => sheet.group === "B").cells), "B1");
  assert.equal(selectedTileForGroup("B2", sheets.find((sheet) => sheet.group === "B").cells), "B2");
  assert.equal(selectedTileForGroup(null, [{ id: "A1" }]), "A1", "a one-group project selects its first cell");

  const source = fs.readFileSync(path.join(__dirname, "../src/map_editor.ts"), "utf8");
  assert.match(source, /tilesheetPicker\.addEventListener\('change', \(\) => showTilesheetGroup/);
  assert.match(source, /tilesheetPicker\.disabled = groups\.length <= 1/);
  assert.match(source, /selectedTile = sheet\.cells\.some\(\(cell\) => cell\.id === selectedTile\) \? selectedTile : \(sheet\.cells\[0\]\?\.id \|\| null\)/);
  assert.match(source, /selectedPicker\.addEventListener\('change', \(\) => selectTile\(selectedPicker\.value\)\)/);
  assert.match(source, /button\.addEventListener\('click', \(\) => selectTile\(cell\.id\)\)/);
});

test("map picker filters to one project, lists only .b8map files, and sorts deterministically", () => {
  const files = [
    { project: "one", path: "south.b8map" },
    { project: "one", path: "src/main.b8" },
    { project: "two", path: "foreign.b8map" },
    { project: "one", path: "east.b8map" },
    { project: "one", path: "east.b8map" },
    { project: "one", path: "layout.b8world" },
  ];
  assert.deepEqual(mapPickerPaths(files, "one"), ["east.b8map", "south.b8map"]);
  assert.deepEqual(mapPickerPaths(files, "two"), ["foreign.b8map"]);
});

test("inverse pan/zoom hit testing keeps painting on logical cells and map edges", () => {
  assert.deepEqual(mapCellAtViewportPoint({ x: 10, y: 30 }, 8, 8, { x: 10, y: 30 }, 1, 1), { x: 0, y: 0 });
  const pan = { x: -54, y: 25 };
  const zoom = 0.75;
  const tileSize = 64;
  const point = { x: pan.x + 7 * tileSize * zoom + 4, y: pan.y + 7 * tileSize * zoom + 4 };
  assert.deepEqual(mapCellAtViewportPoint(point, 8, 8, pan, zoom, tileSize), { x: 7, y: 7 });
  assert.equal(mapCellAtViewportPoint({ x: pan.x - 1, y: pan.y }, 8, 8, pan, zoom, tileSize), undefined);

  const anchored = zoomMapAt(pan, zoom, 1.25, { x: 250, y: 180 }, tileSize);
  const before = { x: (250 - pan.x) / (tileSize * zoom), y: (180 - pan.y) / (tileSize * zoom) };
  assert.deepEqual(
    { x: anchored.pan.x + before.x * tileSize * anchored.zoom, y: anchored.pan.y + before.y * tileSize * anchored.zoom },
    { x: 250, y: 180 },
  );
  const afterPoint = { x: anchored.pan.x + 7.5 * tileSize * anchored.zoom, y: anchored.pan.y + 7.5 * tileSize * anchored.zoom };
  const afterCell = mapCellAtViewportPoint(afterPoint, 8, 8, anchored.pan, anchored.zoom, tileSize);
  assert.deepEqual(afterCell, { x: 7, y: 7 });
  const blankMap = `version = 1\nwidth = 8\nheight = 8\n\n${Array.from({ length: 8 }, () => "-- -- -- -- -- -- -- --").join("\n")}\n`;
  const painted = replaceMapCell(blankMap, afterCell.x, afterCell.y, "A4", assetIds);
  assert.equal(parseMapDocument(painted, assetIds).cells[7][7], "A4", "painting after pan+zoom writes the hit logical cell");
});

test("grid lines transform with the same pan and zoom as map cells", () => {
  const grid = mapGridLines(8, 8, { x: -12, y: 20 }, 0.5, 128);
  assert.equal(grid.vertical[0], -12);
  assert.equal(grid.vertical[1], 52);
  assert.equal(grid.horizontal[8], 532);
});

test("rejects malformed maps, unknown/out-of-range cells and invalid edits safely", () => {
  const unsupported = "version = 2\nwidth = 1\nheight = 1\nA1\n";
  const badDimensions = "version = 1\nwidth = 2\nheight = 1\nA1\n";
  const unknown = "version = 1\nwidth = 1\nheight = 1\nA99\n";
  assert.throws(() => parseMapDocument(unsupported, assetIds), /unsupported map version/);
  assert.throws(() => parseMapDocument(badDimensions, assetIds), /row 1 has 1 cells; expected 2/);
  assert.throws(() => parseMapDocument(unknown, assetIds), /A99/);
  assert.throws(() => replaceMapCell(validMap, 0, 0, "A99", assetIds), /unregistered Bit8 tile/);
  assert.throws(() => replaceMapCell(unsupported, 0, 0, "A1", assetIds), /unsupported map version/);
  assert.equal(unsupported, "version = 2\nwidth = 1\nheight = 1\nA1\n");
});

test("edited serialization is still accepted by Rust Map Core", (context) => {
  const repository = path.resolve(__dirname, "..", "..");
  const bit8 = path.join(repository, "target", "debug", process.platform === "win32" ? "bit8.exe" : "bit8");
  if (!fs.existsSync(bit8)) return context.skip("build the Bit8 CLI first to run the Rust Map Core check");

  const demo = path.join(repository, "games", "tilesheet_demo");
  const project = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-map-model-core-"));
  try {
    const registry = fs.readFileSync(path.join(demo, "bit8.assets.toml"), "utf8")
      .replace('retired_groups = ["B"]\n', "");
    fs.writeFileSync(path.join(project, "bit8.assets.toml"), registry +
      '\n[[tilesheets]]\ngroup = "B"\nfile = "generated (B).png"\nsolid = [6, 7, 9]\n');
    fs.copyFileSync(path.join(demo, "bit8.sprites.toml"), path.join(project, "bit8.sprites.toml"));
    fs.copyFileSync(path.join(demo, "tilesheet (A).png"), path.join(project, "tilesheet (A).png"));
    fs.writeFileSync(path.join(project, "generated (B).png"), generatedTilesheetPng());
    fs.copyFileSync(path.join(demo, "MrHamburger (C).png"), path.join(project, "MrHamburger (C).png"));
    const ids = assetIds;
    const source = fs.readFileSync(path.join(demo, "world.b8map"), "utf8");
    const withGeneratedTile = replaceMapCell(source, 3, 2, "B28", ids);
    const painted = replaceMapCell(withGeneratedTile, 2, 2, "A4", ids);
    const erased = replaceMapCell(painted, 1, 1, null, ids);
    const map = path.join(project, "world.b8map");
    fs.writeFileSync(map, erased);
    const report = execFileSync(bit8, ["inspect", "map", project, map], { encoding: "utf8" });
    assert.match(report, /Size: 64 × 64 tiles/);
    assert.match(report, /A4/);
    assert.match(report, /B28/);
    const assets = JSON.parse(execFileSync(bit8, ["inspect", "tilesheets", project, "--json"], { encoding: "utf8" }));
    const sheet = assets.tilesheets.find(sheet => sheet.group === "B");
    assert.deepEqual([sheet.width, sheet.height, sheet.cells.length], [32, 64, 32]);
    assert.deepEqual(sheet.cells.map(cell => cell.id), Array.from({ length: 32 }, (_, index) => `B${index + 1}`));
    assert.deepEqual(sheet.cells.filter(cell => cell.solid).map(cell => cell.id), ["B6", "B7", "B9"]);
  } finally {
    fs.rmSync(project, { recursive: true, force: true });
  }
});
