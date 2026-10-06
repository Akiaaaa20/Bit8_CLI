import * as vscode from "vscode";
import { parseTilesheetInspection } from "./project_model";
import { nativeFilePath, runNativeBit8Cli } from "./native_cli";
import { notifyAssetRegistryChanged } from "./asset_refresh";

interface TilesheetChange {
  group: string;
  file: string;
}

export function registerTilesheetCommands(): vscode.Disposable[] {
  return [
    vscode.commands.registerCommand("bit8.addTilesheet", (uri?: vscode.Uri) => runTilesheetAction(uri, "add")),
    vscode.commands.registerCommand("bit8.applyTilesheetName", (uri?: vscode.Uri) => runTilesheetAction(uri, "name")),
  ];
}

/** Persist collision metadata through Rust project-data validation, then refresh open maps. */
export async function setTilesheetCellSolid(projectUri: vscode.Uri, id: string, solid: boolean): Promise<void> {
  const projectPath = nativeFilePath(projectUri.toString());
  await runNativeBit8Cli(projectPath, ["tilesheet", "solid", projectPath, id, String(solid)]);
  notifyAssetRegistryChanged(projectUri.toString());
}

async function runTilesheetAction(uri: vscode.Uri | undefined, action: "add" | "name"): Promise<void> {
  const target = uri ?? activeResource();
  if (!target || !target.path.toLowerCase().endsWith(".png")) {
    void vscode.window.showErrorMessage("Select a PNG in a Bit8 project before running this command.");
    return;
  }
  const folder = vscode.workspace.getWorkspaceFolder(target);
  if (!folder || target.scheme !== "file" || folder.uri.scheme !== "file") {
    void vscode.window.showErrorMessage("Bit8 tilesheet registration requires a local project workspace.");
    return;
  }

  try {
    const projectPath = nativeFilePath(folder.uri.toString());
    let group: string | undefined;
    if (action === "name") {
      const reportText = await runNativeBit8Cli(projectPath, ["inspect", "tilesheets", projectPath, "--json"]);
      const sheets = parseTilesheetInspection(JSON.parse(reportText) as unknown);
      group = sheets.find((sheet) =>
        vscode.Uri.joinPath(folder.uri, ...sheet.file.split("/")).toString() === target.toString(),
      )?.group;
      if (!group) throw new Error("This PNG is not a registered Bit8 tilesheet.");
    }
    const args = action === "add"
      ? ["tilesheet", "add", projectPath, nativeFilePath(target.toString()), "--json"]
      : ["tilesheet", "name", projectPath, group!, "--json"];
    const stdout = await runNativeBit8Cli(projectPath, args, 1024 * 1024);
    const change = JSON.parse(stdout) as TilesheetChange;
    if (!change || typeof change.group !== "string" || typeof change.file !== "string") {
      throw new Error("The Bit8 CLI returned invalid tilesheet registration data.");
    }
    notifyAssetRegistryChanged(folder.uri.toString());
    const resultingUri = vscode.Uri.joinPath(folder.uri, ...change.file.split("/"));
    void vscode.window.showInformationMessage(`Bit8 tilesheet ${change.file} registered as group ${change.group}.`);
    await vscode.commands.executeCommand("vscode.open", resultingUri);
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    void vscode.window.showErrorMessage(`Could not update Bit8 tilesheet: ${detail}`);
  }
}

function activeResource(): vscode.Uri | undefined {
  const input = vscode.window.tabGroups.activeTabGroup.activeTab?.input;
  if (input instanceof vscode.TabInputText || input instanceof vscode.TabInputCustom) return input.uri;
  return vscode.window.activeTextEditor?.document.uri;
}
