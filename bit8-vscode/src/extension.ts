import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";
import { Bit8GameSurfaceProvider } from "./game_surface";
import { frontendCapabilities } from "./frontend_capabilities";
import { nativeHostRuntimeBackendFactory } from "./native_host_runtime_backend";
import { bit8CodeTemplates } from "./templates";
import { registerBit8MapEditor } from "./map_editor";
import { registerTilesheetCommands } from "./tilesheet_commands";

let output: vscode.OutputChannel;
let languageClient: LanguageClient | undefined;
let gameSurface: Bit8GameSurfaceProvider | undefined;
const bit8DocumentSelector: vscode.DocumentSelector = [
  { language: "lua" },
  { language: "bit8" },
];

export function activate(context: vscode.ExtensionContext): void {
  output = vscode.window.createOutputChannel("Bit8");
  context.subscriptions.push(output);
  const capabilities = frontendCapabilities({
    canRunNativeRuntime: typeof process !== "undefined" && Boolean(process.versions?.node),
    canEditWorkspace: true,
    canUseCustomEditors: true,
  });
  gameSurface = new Bit8GameSurfaceProvider(
    output,
    (running) => { void vscode.commands.executeCommand("setContext", "bit8.gameRunning", running); },
    capabilities,
    nativeHostRuntimeBackendFactory,
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("bit8.runGame", () => runGame()),
    vscode.commands.registerCommand("bit8.stopGame", () => stopGame()),
    vscode.commands.registerCommand("bit8.restartGame", () => runGame()),
    vscode.commands.registerCommand("bit8.insertGameLoop", () => insertGameLoop()),
    vscode.commands.registerCommand("bit8.useBit8LanguageMode", () => useBit8LanguageMode(context)),
    ...registerTilesheetCommands(),
    registerBit8MapEditor(context),
    vscode.window.registerWebviewViewProvider(
      "bit8.gameSurface",
      gameSurface,
      { webviewOptions: { retainContextWhenHidden: true } },
    ),
    new vscode.Disposable(() => { void gameSurface?.dispose(); }),
    vscode.workspace.onDidOpenTextDocument((document) => {
      if (document.languageId === "bit8") {
        startLanguageClient(context);
      }
    }),
    vscode.languages.registerCompletionItemProvider(
      bit8DocumentSelector,
      { provideCompletionItems: () => provideTemplateCompletions() },
      "T",
      "t",
      "e",
    ),
  );

  if (vscode.workspace.textDocuments.some((document) => document.languageId === "bit8")) {
    startLanguageClient(context);
  }
}

async function useBit8LanguageMode(context: vscode.ExtensionContext): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    void vscode.window.showErrorMessage("Open a Bit8 or Lua file before switching its language mode.");
    return;
  }

  await vscode.languages.setTextDocumentLanguage(editor.document, "bit8");
  startLanguageClient(context);
}

function startLanguageClient(context: vscode.ExtensionContext): void {
  if (languageClient) {
    return;
  }

  const command = process.env.BIT8_LANGUAGE_SERVER || "bit8-language-server";
  output.appendLine(`Starting Bit8 Language Service: ${command}`);
  output.show(true);
  const client = new LanguageClient(
    "bit8LanguageServer",
    "Bit8 Language Service",
    { command },
    {
      documentSelector: [{ language: "bit8" }],
      outputChannel: output,
    },
  );
  languageClient = client;
  context.subscriptions.push(client);
  void client.start().then(() => {
    output.appendLine("Bit8 Language Service started and connected.");
  }).catch((error: unknown) => {
    output.appendLine(`Could not start Bit8 Language Service: ${String(error)}`);
    output.appendLine("Install it with `cargo install --path language-service --locked`, or set BIT8_LANGUAGE_SERVER to its executable path, then reload VS Code.");
    output.show(true);
    void vscode.window.showErrorMessage(
      "Could not start the Bit8 Language Service. Install it with `cargo install --path language-service --locked` or set BIT8_LANGUAGE_SERVER to its executable path, then reload VS Code.",
    );
    if (languageClient === client) {
      languageClient = undefined;
    }
  });
}

export async function deactivate(): Promise<void> {
  await stopGame();
}

async function runGame(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  const folder =
    (editor && vscode.workspace.getWorkspaceFolder(editor.document.uri)) ??
    vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    void vscode.window.showErrorMessage("Open a Bit8 project folder in VS Code before running the game.");
    return;
  }

  if (!gameSurface) return;
  output.show(true);
  try {
    await gameSurface.run(folder.uri.toString());
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    output.appendLine(message);
    void vscode.window.showErrorMessage(message);
  }
}

async function stopGame(): Promise<void> {
  await gameSurface?.stop();
}

async function insertGameLoop(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    void vscode.window.showErrorMessage("Open a Lua or Bit8 file before inserting the Bit8 game loop.");
    return;
  }

  const boilerplate = "func init()\n\nend\n\nfunc update()\n\nend\n\nfunc draw()\n    cls(0)\n\nend";
  const position = editor.selection.active;
  await editor.edit((edit) => edit.insert(position, boilerplate));
}

function provideTemplateCompletions(): vscode.CompletionItem[] {
  const items: vscode.CompletionItem[] = [];
  for (const template of bit8CodeTemplates) {
    const item = new vscode.CompletionItem(template.label, vscode.CompletionItemKind.Snippet);
    item.detail = template.description;
    item.documentation = new vscode.MarkdownString(template.description);
    item.filterText = template.trigger;
    item.insertText = new vscode.SnippetString(template.source);
    items.push(item);
  }

  return items;
}
