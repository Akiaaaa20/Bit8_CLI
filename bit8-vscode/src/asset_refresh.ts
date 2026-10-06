import type { RegisteredTilesheet } from "./project_model";
import { parseMapDocument, type MapEditorModel } from "./map_editor_model";

export interface AssetSnapshot {
  tilesheets: RegisteredTilesheet[];
  assetIds: Set<string>;
  map: MapEditorModel;
}

/** Build and validate a complete replacement before an open editor swaps to it. */
export function prepareAssetSnapshot(tilesheets: RegisteredTilesheet[], mapText: string): AssetSnapshot {
  const assetIds = new Set(tilesheets.flatMap((sheet) => sheet.cells.map((cell) => cell.id)));
  return { tilesheets, assetIds, map: parseMapDocument(mapText, assetIds) };
}

const listeners = new Map<string, Set<() => void>>();

/** Notify every open Map Workspace for a project after a successful asset operation. */
export function notifyAssetRegistryChanged(projectUri: string): void {
  for (const listener of [...(listeners.get(projectUri) ?? [])]) listener();
}

export function onAssetRegistryChanged(projectUri: string, listener: () => void): { dispose(): void } {
  let projectListeners = listeners.get(projectUri);
  if (!projectListeners) {
    projectListeners = new Set();
    listeners.set(projectUri, projectListeners);
  }
  projectListeners.add(listener);
  return {
    dispose() {
      projectListeners?.delete(listener);
      if (projectListeners?.size === 0) listeners.delete(projectUri);
    },
  };
}
