import type { SpriteSummary } from "./project_model";

/** Shared frontend presentation precedence; preview resolution belongs to Rust. */
export function nodePresentation(
  node: { type: string; sprite?: string; visual?: string; scriptVisual?: string },
  sprites: readonly SpriteSummary[],
  tiles: ReadonlySet<string>,
): { tile?: string; source: string } {
  if (node.type !== "Node") return { source: "Camera" };
  if (node.sprite !== undefined) {
    const preview = sprites.find((sprite) => sprite.name === node.sprite)?.preview;
    return { source: "Sprite", ...(preview && tiles.has(preview) ? { tile: preview } : {}) };
  }
  if (node.visual && tiles.has(node.visual)) return { source: "Visual", tile: node.visual };
  if (node.scriptVisual && tiles.has(node.scriptVisual)) return { source: "Script", tile: node.scriptVisual };
  return { source: "Generic" };
}
