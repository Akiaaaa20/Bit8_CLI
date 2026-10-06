/** Frontend-neutral helpers for parsing and navigating Bit8 map documents. */
export interface MapEditorModel {
  width: number;
  height: number;
  cells: string[][];
  rowLines: number[];
  nodes: MapNode[];
}

export interface MapNode {
  id: string;
  type: "Node" | "Camera";
  name: string;
  x: number;
  y: number;
  enabled: boolean;
  script?: string;
  collider?: BoxCollider;
  visual?: string;
  sprite?: string;
}

export interface BoxCollider {
  enabled: boolean;
  offset_x: number;
  offset_y: number;
  width: number;
  height: number;
}

export type MapNodeEdit =
  | { type: "add"; nodeType?: "Node" | "Camera"; cell?: { x: number; y: number } }
  | { type: "rename"; id: string; name: string }
  | { type: "move"; id: string; x: number; y: number }
  | { type: "setEnabled"; id: string; enabled: boolean }
  | { type: "setScript"; id: string; path: string | null }
  | { type: "setCollider"; id: string; collider: BoxCollider | null }
  | { type: "setVisual"; id: string; visual: string | null }
  | { type: "setSprite"; id: string; sprite: string | null }
  | { type: "delete"; id: string };

export interface MapNodeTextEdit {
  startOffset: number;
  replacement: string;
  selectedNodeId?: string;
}

interface RawMapNode {
  value: MapNode;
  lines: string[];
  start: number;
  end: number;
}

interface ParsedNodeSection {
  startOffset: number;
  lines: string[];
  lineEnding: string;
  hadTrailingLineEnding: boolean;
  records: RawMapNode[];
  nodes: MapNode[];
  nextId: bigint;
  nextIdLine: number;
  hasSection: boolean;
}

export interface MapTokenRange {
  line: number;
  startCharacter: number;
  endCharacter: number;
}

export interface MapPoint {
  x: number;
  y: number;
}

export interface CameraViewportBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export interface CameraViewportRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ProjectMapFile {
  project: string;
  path: string;
}

export const MIN_MAP_ZOOM = 0.125;
export const MAX_MAP_ZOOM = 4;
export const MAP_TILE_CSS_SIZE = 64;
export const DEFAULT_MAP_VIEW_TILE_SPAN = 8;
export const CAMERA_VIEWPORT_WIDTH = 64;
export const CAMERA_VIEWPORT_HEIGHT = 64;
export const CAMERA_VIEWPORT_HALF_WIDTH = CAMERA_VIEWPORT_WIDTH / 2;
export const CAMERA_VIEWPORT_HALF_HEIGHT = CAMERA_VIEWPORT_HEIGHT / 2;

/** Use a predictable map-size baseline; this deliberately does not fit to the viewport. */
export function initialMapZoom(width: number, height: number): number {
  return Math.max(MIN_MAP_ZOOM, Math.min(1, DEFAULT_MAP_VIEW_TILE_SPAN / Math.max(width, height)));
}

export interface TilesheetGroupOption {
  group: string;
  file: string;
}

/** Stable, compact names for the registered sheet/group selector. */
export function tilesheetGroupOptions(sheets: readonly TilesheetGroupOption[]): TilesheetGroupOption[] {
  return [...sheets]
    .sort((left, right) => left.group < right.group ? -1 : left.group > right.group ? 1 : 0)
    .map((sheet) => {
      const filename = sheet.file.split("/").pop() ?? sheet.file;
      const stem = filename.replace(/\.png$/i, "").replace(/\s\([A-Z]+\)$/, "");
      return { group: sheet.group, file: stem };
    });
}

export function selectedTileForGroup(current: string | null, cells: readonly { id: string }[]): string | null {
  if (current && cells.some((cell) => cell.id === current)) return current;
  return cells[0]?.id ?? null;
}

export function mapDisplaySize(width: number, height: number, tileSize: number, zoom = 1): MapPoint {
  return { x: width * tileSize * zoom, y: height * tileSize * zoom };
}

/** Selects only maps from one workspace folder and gives a locale-independent order. */
export function mapPickerPaths(files: readonly ProjectMapFile[], project: string): string[] {
  return [...new Set(files
    .filter((file) => file.project === project && file.path.endsWith(".b8map"))
    .map((file) => file.path))]
    .sort((left, right) => left < right ? -1 : left > right ? 1 : 0);
}

export function mapCellAtViewportPoint(
  point: MapPoint,
  width: number,
  height: number,
  pan: MapPoint,
  zoom: number,
  tileSize: number,
): { x: number; y: number } | undefined {
  const renderedTileSize = tileSize * zoom;
  if (renderedTileSize <= 0) return undefined;
  const x = Math.floor((point.x - pan.x) / renderedTileSize);
  const y = Math.floor((point.y - pan.y) / renderedTileSize);
  return x >= 0 && y >= 0 && x < width && y < height ? { x, y } : undefined;
}

export function mapPointToViewport(point: MapPoint, pan: MapPoint, zoom: number, tileSize: number): MapPoint {
  const scale = tileSize * zoom;
  return { x: pan.x + point.x * scale, y: pan.y + point.y * scale };
}

/** Camera coordinates are the center of its fixed viewport, in game pixels. */
export function cameraViewportBounds(centerX: number, centerY: number): CameraViewportBounds {
  return {
    left: centerX - CAMERA_VIEWPORT_HALF_WIDTH,
    top: centerY - CAMERA_VIEWPORT_HALF_HEIGHT,
    right: centerX + CAMERA_VIEWPORT_HALF_WIDTH,
    bottom: centerY + CAMERA_VIEWPORT_HALF_HEIGHT,
  };
}

/** Convert game-pixel coordinates using the same map pan/zoom transform as Node markers. */
export function gamePixelToMapPoint(point: MapPoint, pan: MapPoint, zoom: number, tileCssSize = MAP_TILE_CSS_SIZE): MapPoint {
  return mapPointToViewport({ x: point.x / 8, y: point.y / 8 }, pan, zoom, tileCssSize);
}

/** Editor-only viewport rectangle; it is intentionally not clipped to map bounds. */
export function cameraViewportRect(
  centerX: number,
  centerY: number,
  pan: MapPoint,
  zoom: number,
  tileCssSize = MAP_TILE_CSS_SIZE,
): CameraViewportRect {
  const bounds = cameraViewportBounds(centerX, centerY);
  const topLeft = gamePixelToMapPoint({ x: bounds.left, y: bounds.top }, pan, zoom, tileCssSize);
  const bottomRight = gamePixelToMapPoint({ x: bounds.right, y: bounds.bottom }, pan, zoom, tileCssSize);
  return { x: topLeft.x, y: topLeft.y, width: bottomRight.x - topLeft.x, height: bottomRight.y - topLeft.y };
}

/** Adjusts pan so the logical point under the viewport anchor stays fixed. */
export function zoomMapAt(
  pan: MapPoint,
  currentZoom: number,
  requestedZoom: number,
  anchor: MapPoint,
  tileSize: number,
): { pan: MapPoint; zoom: number } {
  const zoom = Math.max(MIN_MAP_ZOOM, Math.min(MAX_MAP_ZOOM, requestedZoom));
  const logicalX = (anchor.x - pan.x) / (tileSize * currentZoom);
  const logicalY = (anchor.y - pan.y) / (tileSize * currentZoom);
  return {
    zoom,
    pan: {
      x: anchor.x - logicalX * tileSize * zoom,
      y: anchor.y - logicalY * tileSize * zoom,
    },
  };
}

export function mapGridLines(
  width: number,
  height: number,
  pan: MapPoint = { x: 0, y: 0 },
  zoom = 1,
  tileSize = MAP_TILE_CSS_SIZE,
): { vertical: number[]; horizontal: number[] } {
  const size = tileSize * zoom;
  return {
    vertical: Array.from({ length: width + 1 }, (_, index) => pan.x + index * size),
    horizontal: Array.from({ length: height + 1 }, (_, index) => pan.y + index * size),
  };
}

export function parseMapDocument(source: string, assetIds: ReadonlySet<string>): MapEditorModel {
  const lines = source.split(/\r\n|\n|\r/);
  const headers = new Map<string, number>();
  const rows: Array<{ line: number; cells: string[] }> = [];
  let readingRows = false;

  for (let line = 0; line < lines.length; line++) {
    const text = lines[line].trim();
    if (!text) continue;

    // Node persistence is Rust Map Core data. The frozen Map Workspace only
    // parses tile rows and leaves this trailing section untouched in source.
    if (readingRows && text === "[node_state]") break;

    if (!readingRows) {
      const separator = text.indexOf("=");
      if (separator >= 0) {
        const key = text.slice(0, separator).trim();
        const value = text.slice(separator + 1).trim();
        if (key !== "version" && key !== "width" && key !== "height") {
          throw new Error(`line ${line + 1}: unknown map header '${key}'`);
        }
        if (headers.has(key)) throw new Error(`line ${line + 1}: duplicate '${key}' header`);
        if (!/^\d+$/.test(value)) throw new Error(`line ${line + 1}: invalid '${key}' value`);
        headers.set(key, Number(value));
        continue;
      }
      if (/^(version|width|height)\b/.test(text)) {
        throw new Error(`line ${line + 1}: malformed map header; expected 'key = value'`);
      }
      readingRows = true;
    }

    rows.push({ line, cells: text.split(/[ \t\v\f\r]+/) });
  }

  const version = headers.get("version");
  const width = headers.get("width");
  const height = headers.get("height");
  if (version === undefined) throw new Error("missing version header (expected version = 1)");
  if (version !== 1) throw new Error(`unsupported map version ${version}; expected 1`);
  if (width === undefined) throw new Error("missing width header");
  if (height === undefined) throw new Error("missing height header");
  if (width < 1 || width > 256 || height < 1 || height > 256) {
    throw new Error("map dimensions must be between 1 and 256 tiles");
  }
  if (rows.length !== height) throw new Error(`expected ${height} map rows, found ${rows.length}`);

  for (let index = 0; index < rows.length; index++) {
    const row = rows[index];
    if (row.cells.length !== width) {
      throw new Error(`line ${row.line + 1}: row ${index + 1} has ${row.cells.length} cells; expected ${width}`);
    }
    for (let column = 0; column < row.cells.length; column++) {
      const cell = row.cells[column];
      if (cell === "--") continue;
      if (!/^[A-Z]+[0-9]+$/.test(cell)) {
        throw new Error(`line ${row.line + 1}: malformed cell '${cell}'`);
      }
      if (!assetIds.has(cell)) {
        throw new Error(`line ${row.line + 1}, column ${column + 1}: unknown or out-of-range asset '${cell}'`);
      }
    }
  }

  const nodeSection = parseNodeSection(source);
  return {
    width,
    height,
    cells: rows.map((row) => row.cells),
    rowLines: rows.map((row) => row.line),
    nodes: nodeSection.nodes,
  };
}

/** Apply a node action to only the persisted [node_state] section. */
export function editMapNode(source: string, action: MapNodeEdit): MapNodeTextEdit {
  const section = parseNodeSection(source);
  let selectedNodeId: string | undefined;
  let deletedNodeId: string | undefined;
  let nextId = section.nextId;
  const records = section.records.map((record) => ({ ...record, lines: [...record.lines] }));

  if (action.type === "add") {
    if (nextId >= 18446744073709551615n) throw new Error("Map node ID space is exhausted.");
    const id = `N${nextId}`;
    const names = new Set(section.nodes.map((node) => node.name));
    const nodeType = action.nodeType ?? "Node";
    if (nodeType !== "Node" && nodeType !== "Camera") throw new Error(`Unsupported Node type '${String(nodeType)}'.`);
    const baseName = nodeType;
    let name: string = baseName;
    let suffix = 2;
    while (names.has(name)) name = `${baseName}${suffix++}`;
    const offset = nodeType === "Camera" ? 4 : 0;
    const x = (action.cell ? action.cell.x * 8 : 0) + offset;
    const y = (action.cell ? action.cell.y * 8 : 0) + offset;
    if (![x, y].every(Number.isSafeInteger)) throw new Error("The selected map cell is outside the supported node coordinate range.");
    nextId += 1n;
    records.push({
      value: { id, type: nodeType, name, x, y, enabled: true },
      start: 0,
      end: 0,
      lines: [
      "[[node_state.nodes]]",
      `id = ${JSON.stringify(id)}`,
      `type = ${JSON.stringify(nodeType)}`,
      `name = ${JSON.stringify(name)}`,
      `x = ${x}`,
      `y = ${y}`,
      "enabled = true",
      ],
    });
    selectedNodeId = id;
  } else {
    const record = records.find((candidate) => candidate.value.id === action.id);
    if (!record) throw new Error(`Node '${action.id}' no longer exists in this map.`);
    switch (action.type) {
      case "rename": {
        if (!action.name.trim()) throw new Error("Node name must not be empty.");
        if (section.nodes.some((node) => node.id !== action.id && node.name === action.name)) {
          throw new Error(`A node named '${action.name}' already exists in this map.`);
        }
        replaceNodeField(record, "name", JSON.stringify(action.name));
        record.value.name = action.name;
        selectedNodeId = action.id;
        break;
      }
      case "move": {
        if (!Number.isSafeInteger(action.x) || !Number.isSafeInteger(action.y)) {
          throw new Error("Node X and Y must be integers.");
        }
        const snap = record.value.type === "Camera" ? snapCameraCoordinate : snapNodeCoordinate;
        record.value.x = snap(action.x);
        record.value.y = snap(action.y);
        replaceNodeField(record, "x", String(record.value.x));
        replaceNodeField(record, "y", String(record.value.y));
        selectedNodeId = action.id;
        break;
      }
      case "setEnabled":
        record.value.enabled = action.enabled;
        replaceNodeField(record, "enabled", String(action.enabled));
        selectedNodeId = action.id;
        break;
      case "setScript": {
        const script = action.path === null ? undefined : normalizeNodeScriptPath(action.path);
        setNodeScript(record, script);
        selectedNodeId = action.id;
        break;
      }
      case "setCollider": {
        if (action.collider) validateBoxCollider(action.collider, action.id);
        setNodeCollider(record, action.collider ?? undefined);
        selectedNodeId = action.id;
        break;
      }
      case "setVisual": {
        setNodeVisual(record, action.visual ?? undefined);
        selectedNodeId = action.id;
        break;
      }
      case "setSprite": {
        if (record.value.type !== "Node") throw new Error("Camera does not use ordinary Node Sprite presentation.");
        setNodePresentationField(record, "sprite", action.sprite ?? undefined);
        selectedNodeId = action.id;
        break;
      }
      case "delete":
        deletedNodeId = action.id;
        break;
    }
  }

  const recordsStart = section.records[0]?.start ?? section.lines.length;
  const preamble = section.lines.slice(0, recordsStart);
  setRootNextId(preamble, section.nextIdLine, nextId);
  const outputLines = [...preamble];
  for (const record of records) {
    if (record.value.id === deletedNodeId) continue;
    if (outputLines.at(-1) !== "" && outputLines.length) outputLines.push("");
    outputLines.push(...record.lines);
  }
  const suffix = outputLines.join(section.lineEnding) + (section.hadTrailingLineEnding ? section.lineEnding : "");
  const needsLeadingLineEnding = !section.hasSection && source.length > 0 && !/(\r\n|\r|\n)$/.test(source);
  const replacement = (needsLeadingLineEnding ? section.lineEnding : "") + suffix;
  return { startOffset: section.startOffset, replacement, selectedNodeId };
}

/** Validate and normalize a project-relative Bit8 script path without filesystem or editor dependencies. */
export function normalizeNodeScriptPath(path: string): string {
  const normalizedSeparators = path.replace(/\\/g, "/");
  if (!normalizedSeparators || normalizedSeparators.startsWith("/") || /^[A-Za-z]:/.test(normalizedSeparators) || /^[A-Za-z][A-Za-z0-9+.-]*:/.test(normalizedSeparators)) {
    throw new Error("Node scripts must use a project-relative .b8 path.");
  }
  const segments: string[] = [];
  for (const segment of normalizedSeparators.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      if (!segments.length) throw new Error("Node script path must stay inside the Bit8 project.");
      segments.pop();
      continue;
    }
    segments.push(segment);
  }
  const result = segments.join("/");
  if (!result || !result.endsWith(".b8")) throw new Error("Only existing .b8 project scripts can be assigned to a Node.");
  return result;
}

export function snapNodeCoordinate(value: number): number {
  return Math.round(value / 8) * 8;
}

/** Editor-only nearest tile center; ties select the higher coordinate. */
export function snapCameraCoordinate(value: number): number {
  return Math.round((value - 4) / 8) * 8 + 4;
}

function parseNodeSection(source: string): ParsedNodeSection {
  const allLines = source.split(/\r\n|\n|\r/);
  const lineEnding = source.includes("\r\n") ? "\r\n" : source.includes("\r") ? "\r" : "\n";
  const hadTrailingLineEnding = /(\r\n|\r|\n)$/.test(source);
  if (hadTrailingLineEnding) allLines.pop();
  const markerLine = allLines.findIndex((line) => line.trim() === "[node_state]");
  if (markerLine < 0) {
    return {
      startOffset: source.length,
      lines: ["[node_state]", "next_id = 1"],
      lineEnding,
      hadTrailingLineEnding: true,
      records: [],
      nodes: [],
      nextId: 1n,
      nextIdLine: 1,
      hasSection: false,
    };
  }

  const lines = allLines.slice(markerLine);
  const startOffset = allLines.slice(0, markerLine).reduce((sum, line) => sum + line.length + lineEnding.length, 0);
  const recordStarts: number[] = [];
  for (let index = 1; index < lines.length; index++) {
    const text = lines[index].trim();
    if (text.startsWith("[[") && text.endsWith("]]")) {
      if (text !== "[[node_state.nodes]]") throw new Error(`Unsupported node table '${text}'.`);
      recordStarts.push(index);
    } else if (text.startsWith("[") && text.endsWith("]") &&
        text !== "[node_state]" && text !== "[node_state.nodes.collider]") {
      throw new Error(`Unsupported node table '${text}'.`);
    }
  }

  let nextIdLine = -1;
  let serializedNextId: bigint | undefined;
  const firstRecord = recordStarts[0] ?? lines.length;
  for (let index = 1; index < firstRecord; index++) {
    const assignment = parseTomlAssignment(lines[index]);
    if (!assignment) continue;
    if (assignment.key !== "next_id") throw new Error(`Unsupported node-state field '${assignment.key}'.`);
    if (nextIdLine >= 0) throw new Error("Duplicate node next_id field.");
    if (!/^\d+$/.test(assignment.value)) throw new Error("Node next_id must be a positive integer.");
    serializedNextId = BigInt(assignment.value);
    nextIdLine = index;
  }

  const records: RawMapNode[] = [];
  for (let index = 0; index < recordStarts.length; index++) {
    const start = recordStarts[index];
    const end = recordStarts[index + 1] ?? lines.length;
    const rawLines = lines.slice(start, end);
    const values = new Map<string, string>();
    const colliderValues = new Map<string, string>();
    let inCollider = false;
    let hasColliderTable = false;
    for (const line of rawLines.slice(1)) {
      const table = line.trim();
      if (table === "[node_state.nodes.collider]") {
        if (hasColliderTable) throw new Error(`Node record has duplicate collider tables.`);
        hasColliderTable = true;
        inCollider = true;
        continue;
      }
      if (table.startsWith("[") && table.endsWith("]")) throw new Error(`Unsupported node table '${table}'.`);
      const assignment = parseTomlAssignment(line);
      if (!assignment) continue;
      const target = inCollider ? colliderValues : values;
      if (target.has(assignment.key)) throw new Error(`Duplicate '${assignment.key}' field in node record.`);
      target.set(assignment.key, assignment.value);
    }
    const id = parseTomlString(values.get("id"), "id");
    const name = parseTomlString(values.get("name"), "name");
    const type = values.has("type") ? parseTomlString(values.get("type"), "type") : "Node";
    if (type !== "Node" && type !== "Camera") {
      throw new Error(`Node '${id}' ('${name}') has unknown type '${type}'; expected Node or Camera.`);
    }
    if (!/^N[1-9]\d*$/.test(id)) throw new Error(`Invalid map node ID '${id}'.`);
    const x = parseNodeInteger(values.get("x"), "x");
    const y = parseNodeInteger(values.get("y"), "y");
    if (!Number.isSafeInteger(x) || !Number.isSafeInteger(y)) throw new Error(`Node '${id}' coordinates exceed the editor's safe integer range.`);
    const enabledRaw = values.get("enabled");
    if (enabledRaw !== "true" && enabledRaw !== "false") throw new Error(`Node '${id}' enabled must be true or false.`);
    const script = values.has("script") ? parseTomlString(values.get("script"), "script") : undefined;
    const visual = values.has("visual") ? parseOptionalTomlString(values.get("visual")) : undefined;
    const sprite = values.has("sprite") ? parseTomlString(values.get("sprite"), "sprite") : undefined;
    const collider = hasColliderTable ? parseNodeCollider(colliderValues, id, name) : undefined;
    records.push({ value: { id, type, name, x, y, enabled: enabledRaw === "true", ...(script === undefined ? {} : { script }), ...(collider === undefined ? {} : { collider }), ...(visual === undefined ? {} : { visual }), ...(sprite === undefined ? {} : { sprite }) }, lines: rawLines, start, end });
  }

  const ids = new Set<string>();
  const names = new Set<string>();
  let maxId = 0n;
  for (const record of records) {
    if (ids.has(record.value.id)) throw new Error(`Duplicate map node ID '${record.value.id}'.`);
    if (!record.value.name.trim()) throw new Error(`Node '${record.value.id}' name must not be empty.`);
    if (names.has(record.value.name)) throw new Error(`Duplicate map node name '${record.value.name}'.`);
    ids.add(record.value.id);
    names.add(record.value.name);
    maxId = maxId > BigInt(record.value.id.slice(1)) ? maxId : BigInt(record.value.id.slice(1));
  }
  const minimumNextId = maxId + 1n;
  const nextId = serializedNextId ?? minimumNextId;
  if (nextId < minimumNextId || nextId < 1n || nextId > 18446744073709551615n) {
    throw new Error(`Node next_id must be between ${minimumNextId} and 18446744073709551615.`);
  }
  if (nextIdLine < 0) nextIdLine = 1;
  return { startOffset, lines, lineEnding, hadTrailingLineEnding, records, nodes: records.map((record) => record.value).sort(compareNodeIds), nextId, nextIdLine, hasSection: true };
}

function parseNodeCollider(values: Map<string, string>, id: string, name: string): BoxCollider {
  for (const key of values.keys()) {
    if (!["enabled", "offset_x", "offset_y", "width", "height"].includes(key)) {
      throw new Error(`Node '${id}' ('${name}') collider has unknown field '${key}'.`);
    }
  }
  const enabledRaw = values.get("enabled");
  if (enabledRaw !== "true" && enabledRaw !== "false") throw new Error(`Node '${id}' ('${name}') collider enabled must be true or false.`);
  const integer = (field: string): number => {
    const raw = values.get(field);
    if (raw === undefined || !/^-?\d+$/.test(raw)) throw new Error(`Node '${id}' ('${name}') collider ${field} must be an integer.`);
    const parsed = Number(raw);
    if (!Number.isSafeInteger(parsed)) throw new Error(`Node '${id}' ('${name}') collider ${field} is outside the editor's safe integer range.`);
    return parsed;
  };
  const collider = { enabled: enabledRaw === "true", offset_x: integer("offset_x"), offset_y: integer("offset_y"), width: integer("width"), height: integer("height") };
  validateBoxCollider(collider, id, name);
  return collider;
}

function validateBoxCollider(collider: BoxCollider, id: string, name = id): void {
  for (const field of ["offset_x", "offset_y", "width", "height"] as const) {
    if (!Number.isSafeInteger(collider[field])) throw new Error(`Node '${id}' ('${name}') collider ${field} must be an integer.`);
  }
  if (collider.width <= 0) throw new Error(`Node '${id}' ('${name}') collider width must be a positive integer.`);
  if (collider.height <= 0) throw new Error(`Node '${id}' ('${name}') collider height must be a positive integer.`);
}

function setNodeCollider(record: RawMapNode, collider: BoxCollider | undefined): void {
  const tableIndex = record.lines.findIndex((line) => line.trim() === "[node_state.nodes.collider]");
  if (tableIndex >= 0) record.lines.splice(tableIndex);
  if (!collider) {
    delete record.value.collider;
    return;
  }
  while (record.lines.length > 1 && !record.lines.at(-1)?.trim()) record.lines.pop();
  record.lines.push(
    "",
    "[node_state.nodes.collider]",
    `enabled = ${collider.enabled}`,
    `offset_x = ${collider.offset_x}`,
    `offset_y = ${collider.offset_y}`,
    `width = ${collider.width}`,
    `height = ${collider.height}`,
  );
  record.value.collider = { ...collider };
}

function parseOptionalTomlString(value: string | undefined): string | undefined {
  if (value === undefined) return undefined;
  try { return parseTomlString(value, "visual"); } catch { return undefined; }
}

function setNodeVisual(record: RawMapNode, visual: string | undefined): void {
  setNodePresentationField(record, "visual", visual);
}

function setNodePresentationField(record: RawMapNode, key: "sprite" | "visual", value: string | undefined): void {
  const index = record.lines.findIndex((line, lineIndex) => lineIndex > 0 && parseTomlAssignment(line)?.key === key);
  if (value === undefined) {
    if (index >= 0) record.lines.splice(index, 1);
    delete record.value[key];
    return;
  }
  if (index >= 0) replaceNodeField(record, key, JSON.stringify(value));
  else {
    // A top-level node field must precede the collider table in TOML.
    const colliderIndex = record.lines.findIndex((line) => line.trim() === "[node_state.nodes.collider]");
    const insertionIndex = colliderIndex >= 0 ? colliderIndex : record.lines.length;
    record.lines.splice(insertionIndex, 0, `${key} = ${JSON.stringify(value)}`);
  }
  record.value[key] = value;
}

function compareNodeIds(left: MapNode, right: MapNode): number {
  const a = BigInt(left.id.slice(1)), b = BigInt(right.id.slice(1));
  return a < b ? -1 : a > b ? 1 : 0;
}

function parseTomlAssignment(line: string): { key: string; value: string } | undefined {
  const separator = line.indexOf("=");
  if (separator < 0) return undefined;
  const key = line.slice(0, separator).trim();
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key)) return undefined;
  return { key, value: stripTomlComment(line.slice(separator + 1)).trim() };
}

function stripTomlComment(value: string): string {
  let quote = "";
  let escaped = false;
  for (let index = 0; index < value.length; index++) {
    const character = value[index];
    if (escaped) { escaped = false; continue; }
    if (quote === '"' && character === "\\") { escaped = true; continue; }
    if (!quote && (character === '"' || character === "'")) { quote = character; continue; }
    if (quote === character) { quote = ""; continue; }
    if (!quote && character === "#") return value.slice(0, index);
  }
  return value;
}

function parseTomlString(value: string | undefined, field: string): string {
  if (value === undefined) throw new Error(`Map node is missing '${field}'.`);
  try {
    if (value.startsWith('"')) {
      const parsed: unknown = JSON.parse(value);
      if (typeof parsed === "string") return parsed;
    } else if (value.startsWith("'") && value.endsWith("'") && value.length >= 2) {
      return value.slice(1, -1).replace(/''/g, "'");
    }
  } catch { /* report the same concise invalid-field error below */ }
  throw new Error(`Map node '${field}' must be a quoted string.`);
}

function parseNodeInteger(value: string | undefined, field: string): number {
  if (value === undefined || !/^-?\d+$/.test(value)) throw new Error(`Map node '${field}' must be an integer.`);
  const number = Number(value);
  if (!Number.isSafeInteger(number)) throw new Error(`Map node '${field}' is outside the supported integer range.`);
  return number;
}

function replaceNodeField(record: RawMapNode, key: string, value: string): void {
  const index = record.lines.findIndex((line, lineIndex) => lineIndex > 0 && parseTomlAssignment(line)?.key === key);
  if (index < 0) throw new Error(`Node '${record.value.id}' is missing '${key}'.`);
  const line = record.lines[index];
  const separator = line.indexOf("=");
  const tail = line.slice(separator + 1);
  const comment = findTomlComment(tail);
  record.lines[index] = `${line.slice(0, separator + 1)} ${value}${comment === undefined ? "" : ` ${tail.slice(comment).trimStart()}`}`;
}

function setNodeScript(record: RawMapNode, script: string | undefined): void {
  const index = record.lines.findIndex((line, lineIndex) => lineIndex > 0 && parseTomlAssignment(line)?.key === "script");
  if (script === undefined) {
    if (index >= 0) {
      const line = record.lines[index];
      const tail = line.slice(line.indexOf("=") + 1);
      const comment = findTomlComment(tail);
      if (comment === undefined) record.lines.splice(index, 1);
      else record.lines[index] = `${line.slice(0, line.search(/\S|$/))}${tail.slice(comment).trimStart()}`;
    }
    delete record.value.script;
    return;
  }
  if (index >= 0) replaceNodeField(record, "script", JSON.stringify(script));
  else {
    const colliderIndex = record.lines.findIndex((line) => line.trim() === "[node_state.nodes.collider]");
    let insertionIndex = colliderIndex >= 0 ? colliderIndex : record.lines.length;
    if (colliderIndex < 0) while (insertionIndex > 1 && !record.lines[insertionIndex - 1].trim()) insertionIndex--;
    record.lines.splice(insertionIndex, 0, `script = ${JSON.stringify(script)}`);
  }
  record.value.script = script;
}

function findTomlComment(value: string): number | undefined {
  let quote = "";
  let escaped = false;
  for (let index = 0; index < value.length; index++) {
    const character = value[index];
    if (escaped) { escaped = false; continue; }
    if (quote === '"' && character === "\\") { escaped = true; continue; }
    if (!quote && (character === '"' || character === "'")) { quote = character; continue; }
    if (quote === character) { quote = ""; continue; }
    if (!quote && character === "#") return index;
  }
  return undefined;
}

function setRootNextId(lines: string[], lineIndex: number, nextId: bigint): void {
  if (lineIndex >= 0 && lineIndex < lines.length) {
    replaceRootValue(lines, lineIndex, String(nextId));
  } else {
    lines.splice(1, 0, `next_id = ${nextId}`);
  }
}

function replaceRootValue(lines: string[], lineIndex: number, value: string): void {
  const line = lines[lineIndex];
  const separator = line.indexOf("=");
  const comment = findTomlComment(line.slice(separator + 1));
  const tail = line.slice(separator + 1);
  lines[lineIndex] = `${line.slice(0, separator + 1)} ${value}${comment === undefined ? "" : ` ${tail.slice(comment).trimStart()}`}`;
}

export function mapCellTokenRange(
  source: string,
  x: number,
  y: number,
  assetIds: ReadonlySet<string>,
): MapTokenRange {
  const model = parseMapDocument(source, assetIds);
  if (!Number.isInteger(x) || !Number.isInteger(y) || x < 0 || y < 0 || x >= model.width || y >= model.height) {
    throw new Error(`map cell (${x}, ${y}) is outside ${model.width}x${model.height} map`);
  }
  const line = model.rowLines[y];
  const matches = [...source.split(/\r\n|\n|\r/)[line].matchAll(/\S+/g)];
  const match = matches[x];
  if (!match || match.index === undefined) throw new Error(`could not locate map cell (${x}, ${y}) in document`);
  return { line, startCharacter: match.index, endCharacter: match.index + match[0].length };
}

export function replaceMapCell(
  source: string,
  x: number,
  y: number,
  tile: string | null,
  assetIds: ReadonlySet<string>,
): string {
  if (tile !== null && !assetIds.has(tile)) throw new Error(`unregistered Bit8 tile '${tile}'`);
  const range = mapCellTokenRange(source, x, y, assetIds);
  const lines = source.split(/\r\n|\n|\r/);
  lines[range.line] = lines[range.line].slice(0, range.startCharacter) + (tile ?? "--") + lines[range.line].slice(range.endCharacter);
  return lines.join("\n");
}
