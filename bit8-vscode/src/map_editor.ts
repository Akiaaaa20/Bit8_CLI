import * as vscode from "vscode";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import { parseTilesheetInspection, parseSpriteInspection, type SpriteSummary, type RegisteredTilesheet } from "./project_model";
import { nodePresentation } from "./node_presentation";
import { nativeFilePath, runNativeBit8Cli } from "./native_cli";
import { onAssetRegistryChanged, prepareAssetSnapshot, type AssetSnapshot } from "./asset_refresh";
import { setTilesheetCellSolid } from "./tilesheet_commands";
import { staticNodeSprite } from "./node_script_visual";
import {
  CAMERA_VIEWPORT_HEIGHT,
  CAMERA_VIEWPORT_WIDTH,
  MAP_TILE_CSS_SIZE,
  DEFAULT_MAP_VIEW_TILE_SPAN,
  MIN_MAP_ZOOM,
  editMapNode,
  mapCellTokenRange,
  mapPickerPaths,
  normalizeNodeScriptPath,
  parseMapDocument,
  type MapNodeEdit,
  tilesheetGroupOptions,
  type ProjectMapFile,
} from "./map_editor_model";

type InspectionReport = { tilesheets: RegisteredTilesheet[]; sprites: SpriteSummary[] };

const mapEditorViewType = "bit8.mapEditor";
const registryFile = "bit8.assets.toml";

export class Bit8MapEditorProvider implements vscode.CustomTextEditorProvider {
  async resolveCustomTextEditor(
    document: vscode.TextDocument,
    panel: vscode.WebviewPanel,
    _token: vscode.CancellationToken,
  ): Promise<void> {
    const folder = vscode.workspace.getWorkspaceFolder(document.uri);
    if (!folder) {
      panel.webview.html = errorHtml(panel.webview, "Open this map inside a Bit8 project workspace folder.");
      return;
    }
    const projectUri = folder.uri;
    panel.webview.options = { enableScripts: true, localResourceRoots: [projectUri] };
    panel.webview.html = mapEditorHtml(panel.webview);

    let assetReport: InspectionReport | undefined;
    let assetIds = new Set<string>();
    let projectMaps: string[] = [];
    let editQueue = Promise.resolve();
    let assetRefreshQueue = Promise.resolve();
    let disposed = false;
    let lastError: string | undefined;
    const reportError = (error: unknown): void => {
      if (disposed) return;
      const message = error instanceof Error ? error.message : String(error);
      lastError = message;
      void panel.webview.postMessage({ type: "error", message });
    };
    let modelGeneration = 0;
    const postModel = async (snapshot: AssetSnapshot): Promise<void> => {
      const generation = ++modelGeneration;
      const nodes = await Promise.all(snapshot.map.nodes.map(async (node) => {
        const { id, type, name, x, y, enabled, script, collider, visual, sprite } = node;
        if (sprite !== undefined && !assetReport?.sprites.some((item) => item.name === sprite)) {
          throw new Error(`Node '${name}' (${id}) references unknown sprite '${sprite}'.`);
        }
        let scriptVisual: string | undefined, scriptVisualIssue: string | undefined;
        if (type === "Node" && script) {
          try {
            const relative = normalizeNodeScriptPath(script);
            const uri = vscode.Uri.joinPath(projectUri, ...relative.split("/"));
            const open = vscode.workspace.textDocuments.find((item) => item.uri.toString() === uri.toString());
            const source = open?.getText() ?? new TextDecoder().decode(await vscode.workspace.fs.readFile(uri));
            const candidate = staticNodeSprite(source);
            if (candidate && snapshot.assetIds.has(candidate)) scriptVisual = candidate;
            else if (candidate) scriptVisualIssue = `Script tile is not registered: ${candidate}`;
          } catch { scriptVisualIssue = `Script unavailable: ${script}`; }
        }
        return { id, type, name, x, y, enabled, ...(script === undefined ? {} : { script }),
          ...(collider === undefined ? {} : { collider }), ...(visual === undefined ? {} : { visual }),
          ...(sprite === undefined ? {} : { sprite }),
          ...(scriptVisual === undefined ? {} : { scriptVisual }), ...(scriptVisualIssue === undefined ? {} : { scriptVisualIssue }) };
      }));
      if (disposed || generation !== modelGeneration) return;
      if (disposed) return;
      void panel.webview.postMessage({
        type: "model",
        model: {
          ...snapshot.map,
          nodes,
        },
        tilesheets: snapshot.tilesheets,
        sprites: assetReport?.sprites ?? [],
        groups: tilesheetGroupOptions(snapshot.tilesheets),
        projectMaps,
        currentMap: relativeToFolder(document.uri, projectUri),
      });
    };
    const refresh = (): void => {
      if (!assetReport) {
        if (lastError) void panel.webview.postMessage({ type: "error", message: lastError });
        return;
      }
      try {
        lastError = undefined;
        void postModel(prepareAssetSnapshot(assetReport.tilesheets, document.getText())).catch(reportError);
      } catch (error) {
        reportError(error);
      }
    };
    let projectPath: string | undefined;
    const reloadAssets = (): Promise<void> => {
      assetRefreshQueue = assetRefreshQueue.then(async () => {
        if (!projectPath || disposed) return;
        const nextReport = await inspectTilesheets(projectPath);
        const nextSnapshot = prepareAssetSnapshot(nextReport.tilesheets, document.getText());
        if (disposed) return;
        // Commit only after inspection and validation both succeed, so a bad or
        // temporarily inconsistent registry cannot replace the last good data.
        assetReport = nextReport;
        assetIds = nextSnapshot.assetIds;
        lastError = undefined;
        await postModel(nextSnapshot);
      }).catch(reportError);
      return assetRefreshQueue;
    };
    const assetChangeSubscription = onAssetRegistryChanged(projectUri.toString(), () => { void reloadAssets(); });

    const queueNodeEdit = (action: MapNodeEdit): Promise<void> => {
      editQueue = editQueue.then(async () => {
        const source = document.getText();
        const model = parseMapDocument(source, assetIds);
        if (action.type === "add" && action.cell && (action.cell.x >= model.width || action.cell.y >= model.height)) {
          throw new Error("The selected cell is outside the current map.");
        }
        const nodeEdit = editMapNode(source, action);
        const workspaceEdit = new vscode.WorkspaceEdit();
        workspaceEdit.replace(
          document.uri,
          new vscode.Range(document.positionAt(nodeEdit.startOffset), document.positionAt(source.length)),
          nodeEdit.replacement,
        );
        if (!(await vscode.workspace.applyEdit(workspaceEdit))) throw new Error("VS Code could not apply the map-node edit.");
        if (nodeEdit.selectedNodeId) void panel.webview.postMessage({ type: "selectNode", id: nodeEdit.selectedNodeId });
      }).catch(reportError);
      return editQueue;
    };

    const onMessage = panel.webview.onDidReceiveMessage((message: unknown) => {
      if (!isRecord(message)) return;
      if (message.command === "ready") {
        refresh();
        return;
      }
      if (message.command === "openSpriteDefinition" && typeof message.id === "string") {
        void (async () => {
          // Resolve the current assignment, not an untrusted/stale name or path from the Webview.
          const node = parseMapDocument(document.getText(), assetIds).nodes.find((item) => item.id === message.id);
          if (!node || node.type === "Camera" || !node.sprite || !assetReport?.sprites.some((item) => item.name === node.sprite)) return;
          const target = vscode.Uri.joinPath(projectUri, "bit8.sprites.toml");
          try {
            const definition = await vscode.workspace.openTextDocument(target);
            await vscode.window.showTextDocument(definition, { preview: true });
          } catch {
            void vscode.window.showWarningMessage("Bit8: Sprite definition unavailable. Check this project's bit8.sprites.toml.");
          }
        })().catch(reportError);
        return;
      }
      if (message.command === "selectMap" && typeof message.path === "string") {
        if (!projectMaps.includes(message.path)) {
          reportError(new Error("The selected map is outside the current Bit8 project folder."));
          return;
        }
        const target = vscode.Uri.joinPath(projectUri, ...message.path.split("/"));
        void vscode.commands.executeCommand("vscode.openWith", target, mapEditorViewType).then(
          undefined,
          reportError,
        );
        return;
      }
      if (message.command === "chooseNodeScript" && typeof message.id === "string") {
        void chooseNodeScript(folder, projectUri).then(async (path) => {
          if (path !== undefined) await queueNodeEdit({ type: "setScript", id: message.id as string, path });
        }).catch(reportError);
        return;
      }
      if (message.command === "setTileSolid") {
        if (typeof message.id !== "string" || !assetIds.has(message.id) || typeof message.solid !== "boolean" || !projectPath) {
          reportError(new Error("The map editor received an invalid tile collision edit request."));
          return;
        }
        void setTilesheetCellSolid(projectUri, message.id, message.solid).catch(reportError);
        return;
      }
      if (["addNode", "renameNode", "moveNode", "setNodeEnabled", "setNodeCollider", "setNodeVisual", "setNodeSprite", "clearNodeScript", "deleteNode"].includes(String(message.command))) {
        const action = mapNodeAction(message);
        if (!action) {
          reportError(new Error("The map editor received an invalid node edit request."));
          return;
        }
        if (action.type === "setVisual" && action.visual !== null && !assetIds.has(action.visual)) {
          reportError(new Error(`The visual '${action.visual}' is not a registered Bit8 tile.`));
          return;
        }
        if (action.type === "setSprite" && action.sprite !== null && !assetReport?.sprites.some((sprite) => sprite.name === action.sprite)) {
          reportError(new Error(`Unknown Sprite '${action.sprite}'.`)); return;
        }
        void queueNodeEdit(action);
        return;
      }
      if (message.command !== "paint") return;
      const { x, y, tile } = message;
      if (
        !Number.isSafeInteger(x) || !Number.isSafeInteger(y) ||
        (tile !== null && (typeof tile !== "string" || !assetIds.has(tile)))
      ) {
        reportError(new Error("The map editor received an invalid paint request."));
        return;
      }
      editQueue = editQueue.then(async () => {
        const range = mapCellTokenRange(document.getText(), x as number, y as number, assetIds);
        const edit = new vscode.WorkspaceEdit();
        edit.replace(
          document.uri,
          new vscode.Range(range.line, range.startCharacter, range.line, range.endCharacter),
          (tile as string | null) ?? "--",
        );
        if (!(await vscode.workspace.applyEdit(edit))) throw new Error("VS Code could not apply the map-cell edit.");
      }).catch(reportError);
    });

    const onDocumentChange = vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.uri.toString() === document.uri.toString()) refresh();
      else refreshForScript(event.document.uri);
    });
    const refreshForScript = (uri: vscode.Uri): void => {
      if (!assetReport) return;
      try {
        const nodes = parseMapDocument(document.getText(), assetIds).nodes;
        if (nodes.some((node) => node.type === "Node" && node.script &&
          vscode.Uri.joinPath(projectUri, ...normalizeNodeScriptPath(node.script).split("/")).toString() === uri.toString())) refresh();
      } catch (error) { reportError(error); }
    };
    const onScriptSave = vscode.workspace.onDidSaveTextDocument((saved) => refreshForScript(saved.uri));
    panel.onDidDispose(() => {
      disposed = true;
      assetChangeSubscription.dispose();
      onMessage.dispose();
      onDocumentChange.dispose();
      onScriptSave.dispose();
    });

    try {
      if (document.uri.scheme !== "file" || projectUri.scheme !== "file") {
        throw new Error("Bit8 Map Workspace asset inspection currently requires a local project and native Bit8 CLI.");
      }
      projectPath = nativeFilePath(projectUri.toString());
      try {
        await vscode.workspace.fs.stat(vscode.Uri.joinPath(projectUri, registryFile));
      } catch {
        throw new Error(`No ${registryFile} found in this Bit8 project.`);
      }

      const foundMaps = await vscode.workspace.findFiles(new vscode.RelativePattern(folder, "**/*.b8map"));
      const mapFiles: ProjectMapFile[] = foundMaps.map((uri) => ({
        project: projectUri.toString(),
        path: relativeToFolder(uri, projectUri),
      }));
      const activeMap = relativeToFolder(document.uri, projectUri);
      if (activeMap.endsWith(".b8map") && !mapFiles.some((entry) => entry.path === activeMap)) {
        mapFiles.push({ project: projectUri.toString(), path: activeMap });
      }
      projectMaps = mapPickerPaths(mapFiles, projectUri.toString());

      if (!document.isDirty) await validateMapWithCli(projectPath, nativeFilePath(document.uri.toString()));
      await reloadAssets();
    } catch (error) {
      reportError(error);
    }
  }
}

function mapNodeAction(message: Record<string, unknown>): MapNodeEdit | undefined {
  if (message.command === "setNodeSprite" && typeof message.id === "string" && (message.sprite === null || typeof message.sprite === "string")) {
    return { type: "setSprite", id: message.id, sprite: message.sprite };
  }
  if (message.command === "addNode") {
    const nodeType = message.nodeType === undefined ? "Node" : message.nodeType;
    if (nodeType !== "Node" && nodeType !== "Camera") return undefined;
    if (message.cell === undefined) return { type: "add", nodeType };
    if (!isRecord(message.cell) || !Number.isSafeInteger(message.cell.x) || !Number.isSafeInteger(message.cell.y)) return undefined;
    const x = message.cell.x as number, y = message.cell.y as number;
    if (x < 0 || y < 0) return undefined;
    return { type: "add", nodeType, cell: { x, y } };
  }
  if (typeof message.id !== "string") return undefined;
  if (message.command === "renameNode" && typeof message.name === "string") return { type: "rename", id: message.id, name: message.name };
  if (message.command === "moveNode" && Number.isSafeInteger(message.x) && Number.isSafeInteger(message.y)) {
    return { type: "move", id: message.id, x: message.x as number, y: message.y as number };
  }
  if (message.command === "setNodeEnabled" && typeof message.enabled === "boolean") {
    return { type: "setEnabled", id: message.id, enabled: message.enabled };
  }
  if (message.command === "setNodeCollider") {
    if (message.collider === null) return { type: "setCollider", id: message.id, collider: null };
    if (!isRecord(message.collider) || typeof message.collider.enabled !== "boolean" ||
        !Number.isSafeInteger(message.collider.offset_x) || !Number.isSafeInteger(message.collider.offset_y) ||
        !Number.isSafeInteger(message.collider.width) || !Number.isSafeInteger(message.collider.height)) return undefined;
    return { type: "setCollider", id: message.id, collider: {
      enabled: message.collider.enabled,
      offset_x: message.collider.offset_x as number,
      offset_y: message.collider.offset_y as number,
      width: message.collider.width as number,
      height: message.collider.height as number,
    } };
  }
  if (message.command === "setNodeVisual" && (message.visual === null || typeof message.visual === "string")) {
    return { type: "setVisual", id: message.id, visual: message.visual };
  }
  if (message.command === "clearNodeScript") return { type: "setScript", id: message.id, path: null };
  if (message.command === "deleteNode") return { type: "delete", id: message.id };
  return undefined;
}

async function chooseNodeScript(folder: vscode.WorkspaceFolder, projectUri: vscode.Uri): Promise<string | undefined> {
  const files = await vscode.workspace.findFiles(new vscode.RelativePattern(folder, "**/*.b8"));
  const choices = (await Promise.all(files.map(async (uri) => ({ path: await projectRelativeScriptPath(uri, projectUri) }))))
    .filter((choice): choice is { path: string } => choice.path !== undefined)
    .sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0)
    .map((choice) => ({ label: choice.path, path: choice.path }));
  if (!choices.length) {
    void vscode.window.showInformationMessage("No .b8 scripts were found in this Bit8 project.");
    return undefined;
  }
  return (await vscode.window.showQuickPick(choices, { placeHolder: "Choose a Bit8 script in this project" }))?.path;
}

async function projectRelativeScriptPath(uri: vscode.Uri, projectUri: vscode.Uri): Promise<string | undefined> {
  if (uri.scheme !== "file" || projectUri.scheme !== "file") return undefined;
  try {
    const [rootPath, filePath] = await Promise.all([fs.realpath(projectUri.fsPath), fs.realpath(uri.fsPath)]);
    const relative = path.relative(rootPath, filePath);
    if (!relative || path.isAbsolute(relative) || relative === ".." || relative.startsWith(`..${path.sep}`)) return undefined;
    return normalizeNodeScriptPath(relative.split(path.sep).join("/"));
  } catch {
    return undefined;
  }
}

export function registerBit8MapEditor(context: vscode.ExtensionContext): vscode.Disposable {
  return vscode.window.registerCustomEditorProvider(
    mapEditorViewType,
    new Bit8MapEditorProvider(),
    { supportsMultipleEditorsPerDocument: false, webviewOptions: { retainContextWhenHidden: true } },
  );
}

function relativeToFolder(uri: vscode.Uri, folder: vscode.Uri): string {
  const relative = uri.path.slice(folder.path.replace(/\/$/, "").length).replace(/^\//, "");
  return relative;
}

function inspectTilesheets(projectPath: string): Promise<InspectionReport> {
  return runNativeBit8Cli(projectPath, ["inspect", "tilesheets", projectPath, "--json"]).then((stdout) => {
    let report: RegisteredTilesheet[];
    let sprites: SpriteSummary[];
    try {
      const value: unknown = JSON.parse(stdout);
      report = parseTilesheetInspection(value);
      sprites = parseSpriteInspection(value);
    } catch {
      throw new Error("The Bit8 CLI returned invalid tilesheet JSON.");
    }
    report.sort((left, right) => left.group < right.group ? -1 : left.group > right.group ? 1 : 0);
    return { tilesheets: report, sprites };
  });
}

function validateMapWithCli(projectPath: string, mapPath: string): Promise<void> {
  return runNativeBit8Cli(projectPath, ["inspect", "map", projectPath, mapPath]).then(() => undefined);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object";
}

function errorHtml(webview: vscode.Webview, message: string): string {
  const nonce = createNonce();
  const escaped = message.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;",
  })[character] ?? character);
  return `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}';"><style nonce="${nonce}">body{color:var(--vscode-errorForeground);font-family:var(--vscode-font-family);padding:16px}</style>${escaped}`;
}

export function mapEditorHtml(webview: Pick<vscode.Webview, "cspSource">): string {
  const nonce = createNonce();
  return `<!doctype html>
<html lang="en"><head>
  <meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${webview.cspSource}; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
  <style nonce="${nonce}">
    * { box-sizing: border-box; }
    html, body { width:100%; height:100%; margin:0; padding:0; overflow:hidden; color:var(--vscode-foreground); background:var(--vscode-editor-background); font-family:var(--vscode-font-family); }
    #layout { display:flex; width:100%; height:100%; min-width:0; }
    #sidebar { width:220px; min-width:160px; max-width:42%; flex:0 0 220px; overflow:auto; padding:10px; border-right:1px solid var(--vscode-panel-border); }
    #mapLabel, #tilesLabel { display:block; color:var(--vscode-descriptionForeground); font-size:11px; letter-spacing:.04em; margin:2px 0 6px; }
    #mapPicker { width:100%; margin-bottom:12px; }
    #tilesheetPicker { width:100%; margin-bottom:7px; }
    #tools { display:flex; gap:4px; margin-bottom:14px; }
    button, select { color:var(--vscode-foreground); background:var(--vscode-dropdown-background); border:1px solid var(--vscode-dropdown-border); font:inherit; }
    button { padding:4px 7px; cursor:pointer; }
    button:hover, button.selected { outline:1px solid var(--vscode-focusBorder); background:var(--vscode-list-hoverBackground); }
    #nodeSection { margin:-5px 0 13px; }
    #nodeHeading, #nodePropertiesHeading { color:var(--vscode-descriptionForeground); font-size:11px; letter-spacing:.04em; margin:5px 0; }
    #addNode { width:100%; margin-bottom:5px; text-align:left; }
    #addNodeMenu { display:grid; grid-template-columns:1fr 1fr; gap:4px; margin-bottom:5px; }
    #addNodeMenu button { min-width:0; }
    #nodeList { display:grid; gap:2px; max-height:170px; overflow:auto; }
    .nodeItem { width:100%; text-align:left; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
    #nodeProperties { border-top:1px solid var(--vscode-panel-border); margin-top:7px; padding-top:5px; display:grid; gap:5px; }
    .nodeProperty { display:grid; grid-template-columns:48px minmax(0,1fr); align-items:center; gap:5px; font-size:11px; }
    .nodeProperty input:not([type=checkbox]) { min-width:0; width:100%; color:var(--vscode-input-foreground); background:var(--vscode-input-background); border:1px solid var(--vscode-input-border); padding:3px 4px; font:inherit; }
    #nodeEnabledRow { display:flex; align-items:center; gap:6px; }
    #nodeScriptRow { display:grid; grid-template-columns:48px minmax(0,1fr); align-items:start; gap:5px; font-size:11px; }
    #nodeScriptValue { min-width:0; overflow-wrap:anywhere; color:var(--vscode-descriptionForeground); padding:3px 0; }
    #nodeScriptActions { display:flex; gap:3px; margin-left:53px; }
    #nodeScriptActions button { flex:1; }
    #nodeVisualActions { display:flex; gap:3px; min-width:0; }
    #nodeVisualPicker { min-width:0; width:100%; }
    #clearNodeVisual { flex:none; }
    #nodeVisualStatus { color:var(--vscode-errorForeground); font-size:10px; overflow-wrap:anywhere; }
    .presentationHeading { margin:8px 0 4px; font-size:10px; color:var(--vscode-descriptionForeground); }
    #nodeSpriteInfo { font-size:11px; overflow-wrap:anywhere; }
    #nodeSpriteInfo p { margin:4px 0; white-space:pre-line; }
    #nodeVisualRow { color:var(--vscode-descriptionForeground); font-size:11px; }
    #deleteNode { width:100%; margin-top:3px; }
    #tiles { display:grid; grid-template-columns:repeat(3,minmax(0,1fr)); gap:4px; }
    #tileProperties { border-top:1px solid var(--vscode-panel-border); margin-top:9px; padding-top:7px; display:grid; gap:5px; font-size:11px; }
    .propertyHeading { color:var(--vscode-descriptionForeground); font-size:11px; letter-spacing:.04em; }
    #tileSolidRow, #nodeColliderRow { display:flex; align-items:center; gap:6px; }
    #tileSolidHint { color:var(--vscode-descriptionForeground); }
    #colliderFields { display:grid; gap:5px; }
    .group { grid-column:1/-1; color:var(--vscode-descriptionForeground); font-size:11px; margin-top:7px; }
    .tile { min-width:0; padding:3px 2px; display:grid; justify-items:center; gap:2px; }
    .tile canvas, #selectedPreview { width:42px; height:42px; image-rendering:pixelated; background:repeating-conic-gradient(#666 0% 25%,#aaa 0% 50%) 50%/8px 8px; }
    .tile span { font-size:10px; }
    #workspace { min-width:0; min-height:0; flex:1; display:flex; flex-direction:column; overflow:hidden; }
    #toolbar { min-height:40px; flex:none; display:flex; align-items:center; gap:5px; padding:4px 8px; border-bottom:1px solid var(--vscode-panel-border); }
    #toolbar .spacer { flex:1; }
    #selectedPreview { width:24px; height:24px; }
    #selectedTilePicker { max-width:180px; }
    #cellCoordinate { min-width:88px; color:var(--vscode-descriptionForeground); font-size:11px; white-space:nowrap; }
    #zoomLabel { min-width:38px; text-align:center; color:var(--vscode-descriptionForeground); font-size:11px; }
    #viewport { position:relative; min-width:0; min-height:0; flex:1; overflow:hidden; touch-action:none; outline:none; }
    #mapCanvas { position:absolute; inset:0; width:100%; height:100%; display:block; image-rendering:pixelated; background:var(--vscode-editor-background); touch-action:none; }
    #status { color:var(--vscode-descriptionForeground); margin:auto; padding:16px; white-space:pre-wrap; }
    #error { position:absolute; inset:12px auto auto 12px; z-index:2; max-width:min(560px,calc(100% - 24px)); color:var(--vscode-errorForeground); background:var(--vscode-editor-background); border-left:3px solid var(--vscode-errorForeground); padding:8px 10px; white-space:pre-wrap; }
    [hidden] { display:none !important; }
    @media (max-width:520px) { #sidebar { width:172px; min-width:142px; flex-basis:172px; padding:7px; } #tiles { grid-template-columns:repeat(2,minmax(0,1fr)); } #selectedTilePicker { max-width:90px; } }
  </style>
</head><body>
  <main id="layout">
    <aside id="sidebar">
      <label id="mapLabel" for="mapPicker">MAP</label><select id="mapPicker" aria-label="Project maps"></select>
      <div id="tools" aria-label="Map tools">
        <button id="pencil" class="selected" type="button" title="Pencil">✎</button>
        <button id="eraser" type="button" title="Eraser">⌫</button>
        <button id="panTool" type="button" title="Pan">✋</button>
        <button id="nodeTool" type="button" title="Node">◇</button>
      </div>
      <section id="nodeSection" hidden>
        <div id="nodeHeading">NODES</div>
        <button id="addNode" type="button">+ Add Node</button>
        <div id="addNodeMenu" hidden aria-label="Choose Node type"><button id="addGenericNode" type="button">Node</button><button id="addCameraNode" type="button">Camera</button></div>
        <div id="nodeList" aria-label="Nodes in this map"></div>
        <div id="nodeProperties" hidden>
          <div id="nodePropertiesHeading">NODE</div>
          <div class="nodeProperty"><span>Type</span><span id="nodeTypeValue">Node</span></div>
          <label class="nodeProperty">Name<input id="nodeName" type="text" autocomplete="off"></label>
          <label class="nodeProperty">X<input id="nodeX" type="number" step="8"></label>
          <label class="nodeProperty">Y<input id="nodeY" type="number" step="8"></label>
          <label id="nodeEnabledRow">Enabled<input id="nodeEnabled" type="checkbox"></label>
          <div id="nodeSpriteRow">
            <div class="presentationHeading">PRESENTATION</div>
            <div class="nodeProperty"><span>Sprite</span><select id="nodeSpritePicker" aria-label="Node Sprite"></select></div>
            <div class="nodeProperty"><span>Preview</span><canvas id="nodeSpritePreview" width="8" height="8" style="width:32px;height:32px;image-rendering:pixelated"></canvas></div>
            <div id="nodeSpritePreviewLabel"></div>
            <div id="nodeSpriteInfo" hidden></div>
            <button id="openSpriteDefinition" type="button" title="Open this project's bit8.sprites.toml (definition ranges are not available)" hidden>Open Definition</button>
          </div>
          <div id="nodeVisualRow">
            <div class="presentationHeading">LEGACY VISUAL · compatibility / fallback</div>
            <div class="nodeProperty"><span>Visual</span><span id="nodeVisualActions"><select id="nodeVisualPicker" aria-label="Node visual"></select><button id="clearNodeVisual" type="button">Clear</button></span></div>
            <div id="nodeVisualStatus" hidden></div>
            <div id="nodeVisualSource" hidden></div>
          </div>
          <div class="propertyHeading">COLLISION</div>
          <label id="nodeColliderRow">Collider<input id="nodeColliderEnabled" type="checkbox"></label>
          <div id="colliderFields" hidden>
            <div class="nodeProperty"><span>Shape</span><span>Box</span></div>
            <label class="nodeProperty">Offset X<input id="colliderOffsetX" type="number" step="1"></label>
            <label class="nodeProperty">Offset Y<input id="colliderOffsetY" type="number" step="1"></label>
            <label class="nodeProperty">Width<input id="colliderWidth" type="number" min="1" step="1"></label>
            <label class="nodeProperty">Height<input id="colliderHeight" type="number" min="1" step="1"></label>
          </div>
          <div id="nodeViewportRow" class="nodeProperty" hidden><span>Viewport</span><span id="nodeViewportValue">${CAMERA_VIEWPORT_WIDTH} × ${CAMERA_VIEWPORT_HEIGHT}</span></div>
          <div id="nodeScriptRow"><span>Script</span><span id="nodeScriptValue">None</span></div>
          <div id="nodeScriptActions"><button id="chooseNodeScript" type="button">Choose…</button><button id="clearNodeScript" type="button" hidden>Clear</button></div>
          <button id="deleteNode" type="button">Delete Node</button>
        </div>
      </section>
      <label id="tilesLabel" for="tilesheetPicker">TILES</label>
      <select id="tilesheetPicker" aria-label="Registered tilesheet group"></select>
      <div id="tiles" aria-label="Registered Bit8 tiles"></div>
      <section id="tileProperties" aria-label="Selected tile properties">
        <div class="propertyHeading">TILE</div><div>ID: <span id="selectedTileId">--</span></div>
        <div class="propertyHeading">COLLISION</div>
        <label id="tileSolidRow"><input id="tileSolid" type="checkbox"> Solid</label>
        <div id="tileSolidHint">Applies wherever this tile is used.</div>
      </section>
    </aside>
    <section id="workspace">
      <div id="toolbar">
        <span>Selected</span><canvas id="selectedPreview" width="8" height="8" aria-hidden="true"></canvas>
        <select id="selectedTilePicker" aria-label="Selected tile"></select>
        <span id="cellCoordinate" aria-label="Map cell coordinate">Cell: --, --</span>
        <span class="spacer"></span><button id="zoomOut" type="button" title="Zoom out">−</button><span id="zoomLabel">100%</span><button id="zoomIn" type="button" title="Zoom in">+</button>
      </div>
      <div id="viewport" tabindex="0" aria-label="Map canvas. Space-drag to pan."><canvas id="mapCanvas" aria-label="Bit8 map viewport"></canvas><div id="error" hidden></div><div id="status">Loading map…</div></div>
    </section>
  </main>
  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    const viewport = document.getElementById('viewport');
    const canvas = document.getElementById('mapCanvas');
    const context = canvas.getContext('2d');
    const tiles = document.getElementById('tiles');
    const mapPicker = document.getElementById('mapPicker');
    const tilesheetPicker = document.getElementById('tilesheetPicker');
    const selectedPicker = document.getElementById('selectedTilePicker');
    const selectedPreview = document.getElementById('selectedPreview');
    const nodeSection = document.getElementById('nodeSection');
    const nodeList = document.getElementById('nodeList');
    const nodeProperties = document.getElementById('nodeProperties');
    const nodeTypeValue = document.getElementById('nodeTypeValue');
    const nodeViewportRow = document.getElementById('nodeViewportRow');
    const addNodeMenu = document.getElementById('addNodeMenu');
    const nodeName = document.getElementById('nodeName');
    const nodeX = document.getElementById('nodeX');
    const nodeY = document.getElementById('nodeY');
    const nodeEnabled = document.getElementById('nodeEnabled');
    const nodeSpriteRow = document.getElementById('nodeSpriteRow');
    const nodeSpritePicker = document.getElementById('nodeSpritePicker');
    const nodeSpritePreview = document.getElementById('nodeSpritePreview');
    const nodeSpritePreviewLabel = document.getElementById('nodeSpritePreviewLabel');
    const nodeSpriteInfo = document.getElementById('nodeSpriteInfo');
    const openSpriteDefinition = document.getElementById('openSpriteDefinition');
    let sprites = [];
    const nodePresentation = ${nodePresentation.toString()};
    const nodeVisualRow = document.getElementById('nodeVisualRow');
    const nodeVisualPicker = document.getElementById('nodeVisualPicker');
    const nodeVisualStatus = document.getElementById('nodeVisualStatus');
    const nodeVisualSource = document.getElementById('nodeVisualSource');
    const clearNodeVisualButton = document.getElementById('clearNodeVisual');
    const nodeColliderEnabled = document.getElementById('nodeColliderEnabled');
    const colliderFields = document.getElementById('colliderFields');
    const colliderOffsetX = document.getElementById('colliderOffsetX');
    const colliderOffsetY = document.getElementById('colliderOffsetY');
    const colliderWidth = document.getElementById('colliderWidth');
    const colliderHeight = document.getElementById('colliderHeight');
    const selectedTileId = document.getElementById('selectedTileId');
    const tileSolid = document.getElementById('tileSolid');
    const nodeScriptValue = document.getElementById('nodeScriptValue');
    const chooseNodeScriptButton = document.getElementById('chooseNodeScript');
    const clearNodeScriptButton = document.getElementById('clearNodeScript');
    const errorBox = document.getElementById('error');
    const status = document.getElementById('status');
    const zoomLabel = document.getElementById('zoomLabel');
    const cellCoordinate = document.getElementById('cellCoordinate');
    let model = null, paletteMappedSheets = new Map(), tileLookup = new Map(), sheetLookup = new Map(), selectedTile = null, selectedGroup = null;
    let mode = 'pencil', gesture = null, gesturePointerId = null, lastCell = '', dragPoint = null, spaceHeld = false;
    let selectedNodeId = null, currentMapPath = null, nodeDrag = null, pendingNodeSelectionId = null;
    let pan = { x: 0, y: 0 }, zoom = 1, initialViewSet = false, hoveredCell = null, lastInteractedCell = null;
    const logicalTileCssSize = ${MAP_TILE_CSS_SIZE};
    const defaultMapViewTileSpan = ${DEFAULT_MAP_VIEW_TILE_SPAN};
    const minimumMapZoom = ${MIN_MAP_ZOOM};
    const cameraViewportWidth = ${CAMERA_VIEWPORT_WIDTH};
    const cameraViewportHeight = ${CAMERA_VIEWPORT_HEIGHT};

    function showError(message) { errorBox.hidden = false; errorBox.textContent = String(message) + '\\nThe map document was not rewritten.'; }
    function renderedTileSize() { return logicalTileCssSize * zoom; }
    function mapPoint(point) { const size = renderedTileSize(); return { x: pan.x + point.x * size, y: pan.y + point.y * size }; }
    function mapGamePixelPoint(point) { const size = renderedTileSize() / 8; return { x: pan.x + point.x * size, y: pan.y + point.y * size }; }
    function cellAt(point) {
      const tileSize = renderedTileSize();
      const x = Math.floor((point.x - pan.x) / tileSize), y = Math.floor((point.y - pan.y) / tileSize);
      return model && x >= 0 && y >= 0 && x < model.width && y < model.height ? { x, y } : undefined;
    }
    function cellAtPointer(event) {
      const rect = viewport.getBoundingClientRect();
      return cellAt({ x: event.clientX - rect.left, y: event.clientY - rect.top });
    }
    function renderCellCoordinate() {
      const cell = hoveredCell || lastInteractedCell;
      cellCoordinate.textContent = cell ? 'Cell: ' + cell.x + ', ' + cell.y : 'Cell: --, --';
    }
    function updateHoveredCell(event) {
      hoveredCell = cellAtPointer(event) || null;
      renderCellCoordinate();
      return hoveredCell;
    }
    function drawMap() {
      if (!model) return;
      const width = viewport.clientWidth, height = viewport.clientHeight;
      if (!width || !height) return;
      const ratio = Math.max(1, window.devicePixelRatio || 1);
      if (canvas.width !== Math.round(width * ratio) || canvas.height !== Math.round(height * ratio)) {
        canvas.width = Math.round(width * ratio); canvas.height = Math.round(height * ratio);
      }
      canvas.style.width = width + 'px'; canvas.style.height = height + 'px';
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, width, height);
      context.imageSmoothingEnabled = false;
      const tileSize = renderedTileSize();
      const startX = Math.max(0, Math.floor(-pan.x / tileSize));
      const startY = Math.max(0, Math.floor(-pan.y / tileSize));
      const endX = Math.min(model.width, Math.ceil((width - pan.x) / tileSize));
      const endY = Math.min(model.height, Math.ceil((height - pan.y) / tileSize));
      for (let y = startY; y < endY; y++) for (let x = startX; x < endX; x++) {
        const px = pan.x + x * tileSize, py = pan.y + y * tileSize;
        context.fillStyle = (x + y) % 2 ? '#464b52' : '#353a40';
        context.fillRect(px, py, tileSize, tileSize);
        const entry = tileLookup.get(model.cells[y][x]);
        const image = entry && paletteMappedSheets.get(entry.sheet.group);
        if (entry && image) context.drawImage(image, entry.cell.x * 8, entry.cell.y * 8, 8, 8, px, py, tileSize, tileSize);
        if (entry && entry.cell.solid) {
          context.strokeStyle = 'rgba(255, 196, 0, .9)'; context.lineWidth = 2 / ratio;
          context.strokeRect(px + 2 / ratio, py + 2 / ratio, tileSize - 4 / ratio, tileSize - 4 / ratio);
        }
      }
      const grid = mapGridGeometry(model.width, model.height, pan, tileSize);
      context.beginPath(); context.strokeStyle = 'rgba(255,255,255,.38)'; context.lineWidth = 1 / ratio;
      const mapRight = pan.x + model.width * tileSize, mapBottom = pan.y + model.height * tileSize;
      for (const x of grid.vertical) { const px = Math.round(x * ratio) / ratio + .5 / ratio; context.moveTo(px, pan.y); context.lineTo(px, mapBottom); }
      for (const y of grid.horizontal) { const py = Math.round(y * ratio) / ratio + .5 / ratio; context.moveTo(pan.x, py); context.lineTo(mapRight, py); }
      context.stroke();
      drawCameraViewports(ratio);
      drawNodes();
    }
    function mapNodePosition(node) { return mapPoint({ x: node.x / 8, y: node.y / 8 }); }
    function drawCameraViewports(ratio) {
      if (!model || !Array.isArray(model.nodes)) return;
      for (const node of model.nodes) {
        if (node.type !== 'Camera') continue;
        const center = nodeDrag && nodeDrag.id === node.id ? nodeDrag : node;
        const halfWidth = cameraViewportWidth / 2, halfHeight = cameraViewportHeight / 2;
        const topLeft = mapPoint({ x: (center.x - halfWidth) / 8, y: (center.y - halfHeight) / 8 });
        const bottomRight = mapPoint({ x: (center.x + halfWidth) / 8, y: (center.y + halfHeight) / 8 });
        context.strokeStyle = node.enabled ? 'rgba(75, 170, 255, .95)' : 'rgba(75, 170, 255, .38)';
        context.lineWidth = (node.id === selectedNodeId ? 2 : 1.5) / ratio;
        context.strokeRect(topLeft.x, topLeft.y, bottomRight.x - topLeft.x, bottomRight.y - topLeft.y);
      }
    }
    function drawNodes() {
      if (!model || !Array.isArray(model.nodes)) return;
      for (const node of model.nodes) {
        const nodeX = nodeDrag && nodeDrag.id === node.id ? nodeDrag.x : node.x;
        const nodeY = nodeDrag && nodeDrag.id === node.id ? nodeDrag.y : node.y;
        const position = mapGamePixelPoint({ x: nodeX, y: nodeY });
        const selected = node.id === selectedNodeId;
        const visual = resolvedNodeVisual(node);
        const image = visual && paletteMappedSheets.get(visual.sheet.group);
        const size = renderedTileSize();
        if (visual && image) {
          context.imageSmoothingEnabled = false;
          context.drawImage(image, visual.cell.x * 8, visual.cell.y * 8, 8, 8, position.x, position.y, size, size);
          if (selected) {
            context.strokeStyle = '#4fc1ff'; context.lineWidth = 2;
            context.strokeRect(position.x + 1, position.y + 1, size - 2, size - 2);
          }
        } else {
          // Camera marker is a centered editor handle, not an 8x8 game body.
          const markerSize = node.type === 'Camera' ? 20 : Math.max(6, 8 * zoom);
          context.fillStyle = selected ? '#4fc1ff' : (node.type === 'Camera' ? '#4baaff' : '#b8b8b8');
          context.fillRect(position.x - markerSize / 2, position.y - markerSize / 2, markerSize, markerSize);
          context.fillStyle = '#1e1e1e';
          context.fillRect(position.x - markerSize / 4, position.y - markerSize / 4, markerSize / 2, markerSize / 2);
          if (node.type === 'Camera') {
            context.fillStyle = selected ? '#4fc1ff' : '#4baaff';
            context.fillRect(position.x - markerSize, position.y - .5, markerSize * 2, 1);
            context.fillRect(position.x - .5, position.y - markerSize, 1, markerSize * 2);
          }
        }
        if (node.type !== 'Camera' && node.collider && node.collider.enabled) {
          const left = nodeX + node.collider.offset_x, top = nodeY + node.collider.offset_y;
          const leftTop = mapGamePixelPoint({ x: left, y: top });
          const rightBottom = mapGamePixelPoint({ x: left + node.collider.width, y: top + node.collider.height });
          context.strokeStyle = 'rgba(255, 196, 0, .95)'; context.lineWidth = 2;
          context.strokeRect(leftTop.x, leftTop.y, rightBottom.x - leftTop.x, rightBottom.y - leftTop.y);
        }
        context.fillStyle = '#f0f0f0';
        context.font = '12px sans-serif';
        context.fillText(node.name, position.x + (visual && image ? size + 4 : Math.max(6, 8 * zoom)), position.y + 4);
      }
    }
    function nodeAtPointer(event) {
      if (!model || !Array.isArray(model.nodes)) return null;
      const rect = viewport.getBoundingClientRect();
      const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
      for (const node of [...model.nodes].reverse()) {
        const nodeX = nodeDrag && nodeDrag.id === node.id ? nodeDrag.x : node.x;
        const nodeY = nodeDrag && nodeDrag.id === node.id ? nodeDrag.y : node.y;
        const position = mapGamePixelPoint({ x: nodeX, y: nodeY });
        const hasVisual = !!resolvedNodeVisual(node);
        if (hasVisual && point.x >= position.x && point.x <= position.x + renderedTileSize() &&
            point.y >= position.y && point.y <= position.y + renderedTileSize()) return node;
        const hitRadius = node.type === 'Camera' ? 12 : Math.max(6, 8 * zoom) / 2 + 2;
        if (Math.abs(point.x - position.x) <= hitRadius && Math.abs(point.y - position.y) <= hitRadius) return node;
      }
      return null;
    }
    function mapGridGeometry(width, height, offset, size) {
      return { vertical: Array.from({ length: width + 1 }, (_, i) => offset.x + i * size), horizontal: Array.from({ length: height + 1 }, (_, i) => offset.y + i * size) };
    }
    function resolvedNodeVisual(node) {
      if (node.type !== 'Node') return undefined;
      return tileLookup.get(nodePresentation(node, sprites, new Set(tileLookup.keys())).tile);
    }
    function updateSelected() {
      for (const button of tiles.querySelectorAll('.tile')) button.classList.toggle('selected', button.dataset.id === selectedTile);
      selectedPicker.value = selectedTile || '';
      const entry = tileLookup.get(selectedTile);
      selectedTileId.textContent = entry ? selectedTile : '--';
      tileSolid.checked = !!(entry && entry.cell.solid);
      tileSolid.disabled = !entry;
      const image = entry && paletteMappedSheets.get(entry.sheet.group);
      const preview = selectedPreview.getContext('2d'); preview.clearRect(0, 0, 8, 8); preview.imageSmoothingEnabled = false;
      if (entry && image) preview.drawImage(image, entry.cell.x * 8, entry.cell.y * 8, 8, 8, 0, 0, 8, 8);
    }
    function selectTile(id) { if (!tileLookup.has(id)) return; selectedTile = id; updateSelected(); }
    function setMode(value) {
      mode = value;
      for (const [id, tool] of [['pencil', 'pencil'], ['eraser', 'eraser'], ['panTool', 'pan'], ['nodeTool', 'node']]) document.getElementById(id).classList.toggle('selected', mode === tool);
      nodeSection.hidden = mode !== 'node';
      viewport.style.cursor = mode === 'pan' ? 'grab' : (mode === 'eraser' ? 'cell' : (mode === 'node' ? 'pointer' : 'crosshair'));
      renderNodeSidebar();
    }
    function selectNode(id) {
      selectedNodeId = model && Array.isArray(model.nodes) && model.nodes.some((node) => node.id === id) ? id : null;
      renderNodeSidebar(); drawMap();
    }
    function renderNodeSidebar() {
      if (!model || !Array.isArray(model.nodes)) return;
      nodeList.replaceChildren();
      for (const node of model.nodes) {
        const item = document.createElement('button');
        item.type = 'button'; item.className = 'nodeItem'; item.textContent = node.name; item.title = node.id + ' · ' + node.name;
        item.classList.toggle('selected', node.id === selectedNodeId);
        item.addEventListener('click', () => selectNode(node.id));
        nodeList.append(item);
      }
      const selected = model.nodes.find((node) => node.id === selectedNodeId);
      nodeProperties.hidden = mode !== 'node' || !selected;
      if (selected) {
        nodeTypeValue.textContent = selected.type || 'Node';
        nodeViewportRow.hidden = selected.type !== 'Camera';
        nodeVisualRow.hidden = selected.type === 'Camera';
        nodeSpriteRow.hidden = selected.type === 'Camera';
        openSpriteDefinition.hidden = selected.type === 'Camera' || !selected.sprite || !sprites.some(sprite => sprite.name === selected.sprite);
        if (selected.type !== 'Camera') {
          nodeSpritePicker.replaceChildren();
          const noSprite = document.createElement('option'); noSprite.value = ''; noSprite.textContent = 'None'; nodeSpritePicker.append(noSprite);
          for (const sprite of sprites) {
            const option = document.createElement('option'); option.value = sprite.name; option.textContent = sprite.name; nodeSpritePicker.append(option);
          }
          nodeSpritePicker.value = selected.sprite || '';
          const preview = selected.sprite ? resolvedNodeVisual(selected) : undefined;
          const previewImage = preview && paletteMappedSheets.get(preview.sheet.group);
          const spriteInfo = sprites.find(sprite => sprite.name === selected.sprite);
          nodeSpritePreviewLabel.textContent = selected.sprite ? (spriteInfo?.preview || 'Preview unavailable') : 'No Sprite selected';
          nodeSpriteInfo.hidden = !selected.sprite;
          nodeSpriteInfo.replaceChildren();
          if (spriteInfo) {
            const heading = document.createElement('div'); heading.className = 'presentationHeading'; heading.textContent = 'SPRITE INFO'; nodeSpriteInfo.append(heading);
            const name = document.createElement('strong'); name.textContent = spriteInfo.name; nodeSpriteInfo.append(name);
            const animations = document.createElement('p'); animations.textContent = spriteInfo.animations ? (spriteInfo.animations.length ? 'Animations' : 'No animations') : 'Animation information unavailable. Update the Bit8 CLI.'; nodeSpriteInfo.append(animations);
            for (const animation of spriteInfo.animations || []) {
              const detail = document.createElement('p');
              detail.textContent = animation.name + '\\nFrames: ' + animation.frames.join(' ') + '\\nFPS: ' + animation.fps + '\\nLoop: ' + (animation.loop ? 'Yes' : 'No');
              nodeSpriteInfo.append(detail);
            }
          }
          nodeSpritePreview.hidden = !previewImage;
          const previewContext = nodeSpritePreview.getContext('2d');
          if (previewContext) {
            previewContext.clearRect(0,0,8,8);
            previewContext.imageSmoothingEnabled = false;
            if (preview && previewImage) previewContext.drawImage(previewImage,preview.cell.x*8,preview.cell.y*8,8,8,0,0,8,8);
          }
          nodeVisualPicker.replaceChildren();
          const scriptEntry = tileLookup.get(selected.scriptVisual);
          const none = document.createElement('option'); none.value = ''; none.textContent = scriptEntry ? 'Script: ' + selected.scriptVisual : 'None'; nodeVisualPicker.append(none);
          for (const id of [...tileLookup.keys()].sort(compareTileIds)) {
            const option = document.createElement('option'); option.value = id; option.textContent = id; nodeVisualPicker.append(option);
          }
          const hasVisual = typeof selected.visual === 'string' && tileLookup.has(selected.visual);
          if (typeof selected.visual === 'string' && !hasVisual) {
            const missing = document.createElement('option'); missing.value = selected.visual; missing.textContent = '(Missing: ' + selected.visual + ')'; nodeVisualPicker.append(missing);
          }
          nodeVisualPicker.value = selected.visual || '';
          const issue = selected.visual && !hasVisual ? 'Missing registered tile: ' + selected.visual : (!hasVisual ? selected.scriptVisualIssue : '');
          nodeVisualStatus.hidden = !issue;
          nodeVisualStatus.textContent = issue || '';
          nodeVisualSource.hidden = !hasVisual && !scriptEntry;
          nodeVisualSource.textContent = hasVisual ? 'Source: Manual' : (scriptEntry ? 'Visual: ' + selected.scriptVisual + ' · Source: Script' : '');
          clearNodeVisualButton.hidden = !selected.visual;
        }
        nodeName.value = selected.name; nodeX.value = String(selected.x); nodeY.value = String(selected.y); nodeEnabled.checked = selected.enabled;
        const collider = selected.collider;
        nodeColliderEnabled.checked = !!(collider && collider.enabled);
        colliderFields.hidden = !nodeColliderEnabled.checked;
        colliderOffsetX.value = String(collider?.offset_x ?? 0); colliderOffsetY.value = String(collider?.offset_y ?? 0);
        colliderWidth.value = String(collider?.width ?? 8); colliderHeight.value = String(collider?.height ?? 8);
        nodeScriptValue.textContent = selected.script ?? 'None';
        nodeScriptValue.title = selected.script || '';
        clearNodeScriptButton.hidden = !selected.script;
      } else {
        nodeTypeValue.textContent = 'Node';
        nodeViewportRow.hidden = true;
        nodeSpriteRow.hidden = true; nodeSpritePicker.replaceChildren(); nodeSpritePreview.hidden = true;
        nodeSpriteInfo.hidden = true; nodeSpriteInfo.replaceChildren(); nodeSpritePreviewLabel.textContent = '';
        openSpriteDefinition.hidden = true;
        nodeVisualRow.hidden = true; nodeVisualPicker.replaceChildren(); nodeVisualStatus.hidden = true; nodeVisualStatus.textContent = ''; nodeVisualSource.hidden = true; clearNodeVisualButton.hidden = true;
        nodeColliderEnabled.checked = false; colliderFields.hidden = true;
        nodeScriptValue.textContent = 'None'; nodeScriptValue.title = ''; clearNodeScriptButton.hidden = true;
      }
    }
    function compareTileIds(left, right) {
      const leftMatch = /^([A-Z]+)(\d+)$/.exec(left), rightMatch = /^([A-Z]+)(\d+)$/.exec(right);
      if (!leftMatch || !rightMatch) return left.localeCompare(right);
      return leftMatch[1] === rightMatch[1]
        ? Number(leftMatch[2]) - Number(rightMatch[2])
        : leftMatch[1].localeCompare(rightMatch[1]);
    }
    function commitNodePosition() {
      const x = Number(nodeX.value), y = Number(nodeY.value);
      if (!Number.isSafeInteger(x) || !Number.isSafeInteger(y) || !selectedNodeId) { showError('Node X and Y must be integers.'); return; }
      vscode.postMessage({ command: 'moveNode', id: selectedNodeId, x, y });
    }
    function commitNodeCollider() {
      if (!selectedNodeId) return;
      const collider = {
        enabled: nodeColliderEnabled.checked,
        offset_x: Number(colliderOffsetX.value), offset_y: Number(colliderOffsetY.value),
        width: Number(colliderWidth.value), height: Number(colliderHeight.value),
      };
      if (![collider.offset_x, collider.offset_y, collider.width, collider.height].every(Number.isSafeInteger) || collider.width <= 0 || collider.height <= 0) {
        showError('Collider offsets must be integers and width/height must be positive integers.');
        return;
      }
      vscode.postMessage({ command: 'setNodeCollider', id: selectedNodeId, collider });
    }
    function paintAt(event) {
      if (gesture !== 'paint' || !model) return;
      const cell = cellAtPointer(event);
      if (!cell || (mode === 'pencil' && !selectedTile)) return;
      lastInteractedCell = cell;
      renderCellCoordinate();
      const key = cell.x + ',' + cell.y; if (key === lastCell) return; lastCell = key;
      vscode.postMessage({ command: 'paint', x: cell.x, y: cell.y, tile: mode === 'eraser' ? null : selectedTile });
    }
    function moveNodeAt(event) {
      if (gesture !== 'moveNode' || !nodeDrag || !model) return;
      const rect = viewport.getBoundingClientRect();
      const tileSize = renderedTileSize();
      const draggedNode = model.nodes.find((node) => node.id === nodeDrag.id);
      const camera = draggedNode && draggedNode.type === 'Camera';
      const offset = camera ? .5 : 0;
      const cellX = Math.round((event.clientX - rect.left - pan.x) / tileSize - offset);
      const cellY = Math.round((event.clientY - rect.top - pan.y) / tileSize - offset);
      const x = camera ? cellX * 8 + 4 : Math.max(0, Math.min((model.width - 1) * 8, cellX * 8));
      const y = camera ? cellY * 8 + 4 : Math.max(0, Math.min((model.height - 1) * 8, cellY * 8));
      if (x !== nodeDrag.x || y !== nodeDrag.y) {
        nodeDrag.x = x; nodeDrag.y = y; nodeDrag.changed = x !== nodeDrag.startX || y !== nodeDrag.startY;
        drawMap();
      }
    }
    function panViewport(deltaX, deltaY) { pan = { x: pan.x + deltaX, y: pan.y + deltaY }; drawMap(); }
    function zoomViewport(requestedZoom, anchor) {
      const nextZoom = Math.max(.125, Math.min(4, requestedZoom));
      const oldTileSize = renderedTileSize();
      const logical = { x: (anchor.x - pan.x) / oldTileSize, y: (anchor.y - pan.y) / oldTileSize };
      const nextTileSize = logicalTileCssSize * nextZoom;
      pan = { x: anchor.x - logical.x * nextTileSize, y: anchor.y - logical.y * nextTileSize };
      zoom = nextZoom; zoomLabel.textContent = Math.round(zoom * 100) + '%'; drawMap();
    }
    function initializeMapView() {
      if (!model || initialViewSet) return;
      const width = viewport.clientWidth, height = viewport.clientHeight;
      if (!(width > 0 && height > 0)) return;
      zoom = Math.max(minimumMapZoom, Math.min(1, defaultMapViewTileSpan / Math.max(model.width, model.height)));
      const tileSize = renderedTileSize();
      pan = { x: (width - model.width * tileSize) / 2, y: (height - model.height * tileSize) / 2 };
      zoomLabel.textContent = Math.round(zoom * 1000) / 10 + '%';
      initialViewSet = true;
    }
    function paletteMappedCanvas(sheet) {
      const canvas = document.createElement('canvas');
      canvas.width = sheet.width; canvas.height = sheet.height;
      const context = canvas.getContext('2d');
      const imageData = context.createImageData(sheet.width, sheet.height);
      for (const cell of sheet.cells) {
        for (let index = 0; index < 64; index++) {
          const paletteIndex = cell.pixels[index];
          const color = sheet.palette[paletteIndex];
          const target = ((cell.y * 8 + Math.floor(index / 8)) * sheet.width) + cell.x * 8 + index % 8;
          const offset = target * 4;
          imageData.data[offset] = (color >> 16) & 255;
          imageData.data[offset + 1] = (color >> 8) & 255;
          imageData.data[offset + 2] = color & 255;
          imageData.data[offset + 3] = paletteIndex === 0 ? 0 : 255;
        }
      }
      context.putImageData(imageData, 0, 0);
      return canvas;
    }
    function loadTilesheets(sheets, groups) {
      tileLookup = new Map(); sheetLookup = new Map(); paletteMappedSheets = new Map(); tilesheetPicker.replaceChildren();
      for (const sheet of sheets) {
        sheetLookup.set(sheet.group, sheet);
        paletteMappedSheets.set(sheet.group, paletteMappedCanvas(sheet));
        for (const cell of sheet.cells) {
          tileLookup.set(cell.id, { sheet, cell });
        }
      }
      for (const group of groups) {
        const option = document.createElement('option'); option.value = group.group; option.textContent = group.group + ' · ' + group.file;
        tilesheetPicker.append(option);
      }
      tilesheetPicker.disabled = groups.length <= 1;
      return Promise.resolve();
    }
    function showTilesheetGroup(group) {
      const sheet = sheetLookup.get(group);
      if (!sheet) return;
      selectedGroup = group;
      tilesheetPicker.value = group;
      tiles.replaceChildren(); selectedPicker.replaceChildren();
      for (const cell of sheet.cells) {
        const button = document.createElement('button'); button.type = 'button'; button.className = 'tile'; button.dataset.id = cell.id; button.title = cell.id;
        const preview = document.createElement('canvas'); preview.width = 8; preview.height = 8;
        const ctx = preview.getContext('2d'); ctx.imageSmoothingEnabled = false;
        const image = paletteMappedSheets.get(sheet.group);
        if (image) ctx.drawImage(image, cell.x * 8, cell.y * 8, 8, 8, 0, 0, 8, 8);
        const label = document.createElement('span'); label.textContent = cell.solid ? cell.id + ' · Solid' : cell.id; button.append(preview, label);
        button.addEventListener('click', () => selectTile(cell.id)); tiles.append(button);
        const option = document.createElement('option'); option.value = cell.id; option.textContent = cell.id; selectedPicker.append(option);
      }
      selectedTile = sheet.cells.some((cell) => cell.id === selectedTile) ? selectedTile : (sheet.cells[0]?.id || null);
      updateSelected();
    }
    function loadProjectMaps(paths, current) {
      mapPicker.replaceChildren();
      for (const path of paths) { const option = document.createElement('option'); option.value = path; option.textContent = path; mapPicker.append(option); }
      mapPicker.value = current;
      mapPicker.disabled = paths.length < 2;
    }
    function isFormControl(target) {
      return Boolean(target && target.closest && target.closest('button, select, input, textarea, [contenteditable="true"], [role="button"], [role="combobox"]'));
    }
    function isViewportTarget(target) { return target === viewport || target === canvas || Boolean(viewport.contains && viewport.contains(target)); }
    function onSpaceKeyDown(event) {
      if (event.code !== 'Space' || isFormControl(event.target) || !isViewportTarget(event.target)) return;
      spaceHeld = true;
      event.preventDefault();
    }
    function onSpaceKeyUp(event) {
      if (event.code !== 'Space') return;
      if (spaceHeld) event.preventDefault();
      spaceHeld = false;
    }
    document.addEventListener('keydown', onSpaceKeyDown, true);
    document.addEventListener('keyup', onSpaceKeyUp, true);
    mapPicker.addEventListener('change', () => vscode.postMessage({ command: 'selectMap', path: mapPicker.value }));
    tilesheetPicker.addEventListener('change', () => showTilesheetGroup(tilesheetPicker.value));
    selectedPicker.addEventListener('change', () => selectTile(selectedPicker.value));
    document.getElementById('pencil').addEventListener('click', () => setMode('pencil'));
    document.getElementById('eraser').addEventListener('click', () => setMode('eraser'));
    document.getElementById('panTool').addEventListener('click', () => setMode('pan'));
    document.getElementById('nodeTool').addEventListener('click', () => setMode('node'));
    document.getElementById('addNode').addEventListener('click', () => { addNodeMenu.hidden = !addNodeMenu.hidden; });
    function createNode(nodeType) {
      const cell = hoveredCell || lastInteractedCell;
      addNodeMenu.hidden = true;
      vscode.postMessage({ command: 'addNode', nodeType, ...(cell ? { cell: { x: cell.x, y: cell.y } } : {}) });
    }
    document.getElementById('addGenericNode').addEventListener('click', () => createNode('Node'));
    document.getElementById('addCameraNode').addEventListener('click', () => createNode('Camera'));
    nodeName.addEventListener('change', () => {
      if (selectedNodeId) vscode.postMessage({ command: 'renameNode', id: selectedNodeId, name: nodeName.value });
    });
    nodeX.addEventListener('change', commitNodePosition);
    nodeY.addEventListener('change', commitNodePosition);
    nodeEnabled.addEventListener('change', () => {
      if (selectedNodeId) vscode.postMessage({ command: 'setNodeEnabled', id: selectedNodeId, enabled: nodeEnabled.checked });
    });
    nodeSpritePicker.addEventListener('change', () => {
      if (selectedNodeId) vscode.postMessage({ command: 'setNodeSprite', id: selectedNodeId, sprite: nodeSpritePicker.value || null });
    });
    openSpriteDefinition.addEventListener('click', () => {
      if (selectedNodeId && !openSpriteDefinition.hidden) vscode.postMessage({ command: 'openSpriteDefinition', id: selectedNodeId });
    });
    nodeVisualPicker.addEventListener('change', () => {
      if (selectedNodeId) vscode.postMessage({ command: 'setNodeVisual', id: selectedNodeId, visual: nodeVisualPicker.value || null });
    });
    clearNodeVisualButton.addEventListener('click', () => {
      if (selectedNodeId) vscode.postMessage({ command: 'setNodeVisual', id: selectedNodeId, visual: null });
    });
    nodeColliderEnabled.addEventListener('change', () => {
      if (!selectedNodeId) return;
      colliderFields.hidden = !nodeColliderEnabled.checked;
      commitNodeCollider();
    });
    for (const input of [colliderOffsetX, colliderOffsetY, colliderWidth, colliderHeight]) input.addEventListener('change', commitNodeCollider);
    tileSolid.addEventListener('change', () => {
      const entry = tileLookup.get(selectedTile);
      if (entry) vscode.postMessage({ command: 'setTileSolid', id: selectedTile, solid: tileSolid.checked });
    });
    chooseNodeScriptButton.addEventListener('click', () => {
      if (selectedNodeId) vscode.postMessage({ command: 'chooseNodeScript', id: selectedNodeId });
    });
    clearNodeScriptButton.addEventListener('click', () => {
      if (selectedNodeId) vscode.postMessage({ command: 'clearNodeScript', id: selectedNodeId });
    });
    document.getElementById('deleteNode').addEventListener('click', () => {
      if (!selectedNodeId) return;
      const id = selectedNodeId; selectedNodeId = null; renderNodeSidebar(); drawMap();
      vscode.postMessage({ command: 'deleteNode', id });
    });
    document.getElementById('zoomIn').addEventListener('click', () => zoomViewport(zoom * 1.25, { x: viewport.clientWidth / 2, y: viewport.clientHeight / 2 }));
    document.getElementById('zoomOut').addEventListener('click', () => zoomViewport(zoom / 1.25, { x: viewport.clientWidth / 2, y: viewport.clientHeight / 2 }));
    viewport.addEventListener('wheel', (event) => {
      const scale = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? viewport.clientHeight : 1;
      const deltaX = event.deltaX * scale, deltaY = event.deltaY * scale;
      if (event.metaKey || event.ctrlKey) {
        if (!deltaY) return;
        const rect = viewport.getBoundingClientRect();
        zoomViewport(zoom * Math.exp(-deltaY * .002), { x: event.clientX - rect.left, y: event.clientY - rect.top });
        updateHoveredCell(event);
      } else {
        if (!deltaX && !deltaY) return;
        panViewport(-deltaX, -deltaY);
        updateHoveredCell(event);
      }
      event.preventDefault();
      if (event.stopPropagation) event.stopPropagation();
    }, { passive: false });
    viewport.addEventListener('pointerdown', (event) => {
      if (gesture || (event.button !== 0 && event.button !== 1)) return;
      const shouldPan = event.button === 1 || (event.button === 0 && (mode === 'pan' || spaceHeld));
      updateHoveredCell(event);
      if (shouldPan) gesture = 'pan';
      else if (mode === 'node') {
        const hit = nodeAtPointer(event);
        if (hit) {
          selectNode(hit.id);
          gesture = 'moveNode';
          nodeDrag = { id: hit.id, startX: hit.x, startY: hit.y, x: hit.x, y: hit.y, changed: false };
        } else {
          selectNode(null);
          if (hoveredCell) { lastInteractedCell = hoveredCell; renderCellCoordinate(); }
          gesture = 'node';
        }
      } else gesture = 'paint';
      gesturePointerId = event.pointerId; lastCell = '';
      if (viewport.focus) viewport.focus({ preventScroll: true });
      viewport.setPointerCapture(event.pointerId);
      event.preventDefault();
      if (gesture === 'pan') { dragPoint = { x: event.clientX, y: event.clientY }; viewport.style.cursor = 'grabbing'; }
      else if (gesture === 'paint') paintAt(event);
    });
    viewport.addEventListener('pointermove', (event) => {
      if (gesturePointerId !== null && event.pointerId !== undefined && event.pointerId !== gesturePointerId) return;
      updateHoveredCell(event);
      if (gesture === 'pan' && dragPoint) { panViewport(event.clientX - dragPoint.x, event.clientY - dragPoint.y); dragPoint = { x: event.clientX, y: event.clientY }; updateHoveredCell(event); event.preventDefault(); }
      else if (gesture === 'moveNode') { moveNodeAt(event); event.preventDefault(); }
      else if (gesture === 'paint') { paintAt(event); event.preventDefault(); }
    });
    viewport.addEventListener('pointerleave', () => { hoveredCell = null; renderCellCoordinate(); });
    function endPointer(event) {
      if (event && gesturePointerId !== null && event.pointerId !== undefined && event.pointerId !== gesturePointerId) return;
      const pointerId = gesturePointerId;
      if (gesture === 'moveNode' && nodeDrag && nodeDrag.changed) {
        vscode.postMessage({ command: 'moveNode', id: nodeDrag.id, x: nodeDrag.x, y: nodeDrag.y });
      }
      gesture = null; gesturePointerId = null; dragPoint = null; lastCell = '';
      nodeDrag = null; drawMap();
      if (pointerId !== null && viewport.hasPointerCapture && viewport.hasPointerCapture(pointerId) && viewport.releasePointerCapture) {
        try { viewport.releasePointerCapture(pointerId); } catch (_) { /* capture may already have ended */ }
      }
      viewport.style.cursor = mode === 'pan' ? 'grab' : (mode === 'eraser' ? 'cell' : 'crosshair');
    }
    function clearNavigationInput() { spaceHeld = false; endPointer(); }
    viewport.addEventListener('pointerup', endPointer); viewport.addEventListener('pointercancel', endPointer);
    viewport.addEventListener('lostpointercapture', endPointer);
    window.addEventListener('blur', clearNavigationInput);
    const resizeObserver = new ResizeObserver(() => {
      initializeMapView();
      drawMap();
    });
    resizeObserver.observe(viewport);
    window.addEventListener('message', async (event) => {
      const message = event.data;
      if (message.type === 'error') { showError(message.message); updateSelected(); renderNodeSidebar(); return; }
      if (message.type === 'selectNode') {
        const id = typeof message.id === 'string' ? message.id : null;
        if (id && model && Array.isArray(model.nodes) && model.nodes.some((node) => node.id === id)) selectNode(id);
        else pendingNodeSelectionId = id;
        return;
      }
      if (message.type !== 'model') return;
      try {
        if (currentMapPath !== message.currentMap) {
          selectedNodeId = null; pendingNodeSelectionId = null; currentMapPath = message.currentMap;
        }
        hoveredCell = null; lastInteractedCell = null; renderCellCoordinate();
        await loadTilesheets(message.tilesheets, message.groups); sprites = message.sprites || []; model = message.model; loadProjectMaps(message.projectMaps, message.currentMap);
        model.nodes = Array.isArray(model.nodes)
          ? [...model.nodes].sort((left, right) => BigInt(left.id.slice(1)) < BigInt(right.id.slice(1)) ? -1 : BigInt(left.id.slice(1)) > BigInt(right.id.slice(1)) ? 1 : 0)
          : [];
        if (pendingNodeSelectionId && model.nodes.some((node) => node.id === pendingNodeSelectionId)) {
          selectedNodeId = pendingNodeSelectionId; pendingNodeSelectionId = null;
        }
        if (!model.nodes.some((node) => node.id === selectedNodeId)) selectedNodeId = null;
        renderNodeSidebar();
        const firstGroup = message.groups[0]?.group || null;
        const activeGroup = selectedGroup && sheetLookup.has(selectedGroup) ? selectedGroup : firstGroup;
        errorBox.hidden = true; status.hidden = true; setMode(mode);
        if (activeGroup) showTilesheetGroup(activeGroup);
        else { tiles.replaceChildren(); selectedPicker.replaceChildren(); selectedTile = null; updateSelected(); }
        initializeMapView();
        drawMap();
      } catch (error) { showError(error instanceof Error ? error.message : String(error)); }
    });
    vscode.postMessage({ command: 'ready' });
  </script>
</body></html>`;
}

function createNonce(): string {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  return Array.from({ length: 32 }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join("");
}
