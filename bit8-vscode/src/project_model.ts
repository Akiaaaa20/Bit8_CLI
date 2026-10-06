/** Bit8 project metadata validated without VS Code, Node, or DOM dependencies. */
export interface RegisteredTilesheetCell {
  id: string;
  x: number;
  y: number;
  solid: boolean;
  /** Runtime-mapped 8x8 palette indices in row-major order; 0 is transparent. */
  pixels: number[];
}

export interface RegisteredTilesheet {
  group: string;
  file: string;
  width: number;
  height: number;
  columns: number;
  rows: number;
  /** Exact 0xRRGGBB palette values used by the Rust Runtime. */
  palette: number[];
  cells: RegisteredTilesheetCell[];
}

export interface TilesheetInspectionReport {
  tilesheets: RegisteredTilesheet[];
}

export function parseTilesheetInspection(value: unknown): RegisteredTilesheet[] {
  if (!isRecord(value) || !Array.isArray(value.tilesheets)) {
    throw new Error("The Bit8 CLI returned an invalid tilesheet report.");
  }
  return value.tilesheets.map((item: unknown, index: number) => {
    if (!isRecord(item) || typeof item.group !== "string" || !/^[A-Z]+$/.test(item.group)) {
      throw new Error(`The Bit8 CLI returned invalid tilesheet metadata at entry ${index + 1}.`);
    }
    const file = safeProjectRelativePath(item.file);
    const width = positiveInteger(item.width, `tilesheet '${file}' width`);
    const height = positiveInteger(item.height, `tilesheet '${file}' height`);
    const columns = positiveInteger(item.columns, `tilesheet '${file}' columns`);
    const rows = positiveInteger(item.rows, `tilesheet '${file}' rows`);
    if (!Array.isArray(item.palette) || item.palette.length !== 16 ||
        item.palette.some((color) => !Number.isSafeInteger(color) || (color as number) < 0 || (color as number) > 0xffffff)) {
      throw new Error(`The Bit8 CLI returned an invalid Runtime palette for tilesheet '${file}'.`);
    }
    if (width % 8 !== 0 || height % 8 !== 0 || columns !== width / 8 || rows !== height / 8) {
      throw new Error(`The Bit8 CLI returned inconsistent cell dimensions for tilesheet '${file}'.`);
    }
    if (!Array.isArray(item.cells) || item.cells.length !== columns * rows) {
      throw new Error(`The Bit8 CLI returned invalid cells for tilesheet '${file}'.`);
    }
    const ids = new Set<string>();
    const cells = item.cells.map((cell: unknown) => {
      if (!isRecord(cell) || typeof cell.id !== "string" || cell.id.length === 0 ||
          !Number.isSafeInteger(cell.x) || !Number.isSafeInteger(cell.y) ||
          (cell.x as number) < 0 || (cell.x as number) >= columns ||
          (cell.y as number) < 0 || (cell.y as number) >= rows || ids.has(cell.id) ||
          !Array.isArray(cell.pixels) || cell.pixels.length !== 64 ||
          cell.pixels.some((pixel) => !Number.isSafeInteger(pixel) || (pixel as number) < 0 || (pixel as number) > 15)) {
        throw new Error(`The Bit8 CLI returned invalid cell metadata for tilesheet '${file}'.`);
      }
      ids.add(cell.id);
      if (cell.solid !== undefined && typeof cell.solid !== "boolean") {
        throw new Error(`The Bit8 CLI returned invalid collision metadata for tilesheet '${file}' cell '${cell.id}'.`);
      }
      return { id: cell.id, x: cell.x as number, y: cell.y as number, solid: cell.solid ?? false, pixels: cell.pixels as number[] };
    });
    return { group: item.group, file, width, height, columns, rows, palette: item.palette as number[], cells };
  });
}

export function safeProjectRelativePath(value: unknown): string {
  if (typeof value !== "string" || value.length === 0 || value.includes("\\") ||
      value.startsWith("/") || /^[A-Za-z]:/.test(value) ||
      value.split("/").some((part) => part === "" || part === "." || part === "..")) {
    throw new Error("The Bit8 CLI returned a path outside the project.");
  }
  return value;
}

function positiveInteger(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`The Bit8 CLI returned invalid ${name}.`);
  }
  return value;
}

export interface SpriteAnimationSummary { name: string; frames: string[]; fps: number; loop: boolean }
export interface SpriteSummary { name: string; preview: string | null; animations?: SpriteAnimationSummary[] }

/** Consume core-resolved previews. Never infer idle/animation ordering here. */
export function parseSpriteInspection(value: unknown): SpriteSummary[] {
  if (!isRecord(value)) throw new Error("Invalid Bit8 project inspection.");
  if (value.sprites === undefined) return []; // Older CLI / projects without definitions.
  if (!Array.isArray(value.sprites)) throw new Error("Invalid Bit8 Sprite inspection.");
  const names = new Set<string>();
  return value.sprites.map((item: unknown) => {
    if (!isRecord(item) || typeof item.name !== "string" || !item.name.trim() || names.has(item.name) ||
        (item.preview !== null && typeof item.preview !== "string")) throw new Error("Invalid Bit8 Sprite inspection.");
    names.add(item.name);
    if (item.animations === undefined) return { name: item.name, preview: item.preview as string | null }; // Older CLI.
    if (!Array.isArray(item.animations)) throw new Error("Invalid Bit8 Sprite animations.");
    const animationNames = new Set<string>();
    const animations = item.animations.map((animation: unknown): SpriteAnimationSummary => {
      if (!isRecord(animation) || typeof animation.name !== "string" || !animation.name.trim() || animationNames.has(animation.name) ||
          !Array.isArray(animation.frames) || animation.frames.length === 0 || !animation.frames.every((frame: unknown) => typeof frame === "string" && /^[A-Z]+[1-9][0-9]*$/.test(frame)) ||
          typeof animation.fps !== "number" || !Number.isInteger(animation.fps) || animation.fps < 1 || animation.fps > 30 || typeof animation.loop !== "boolean") {
        throw new Error("Invalid Bit8 Sprite animations.");
      }
      animationNames.add(animation.name);
      return { name: animation.name, frames: [...animation.frames] as string[], fps: animation.fps, loop: animation.loop };
    });
    return { name: item.name, preview: item.preview as string | null, animations };
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}
