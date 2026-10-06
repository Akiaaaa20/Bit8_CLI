import * as vscode from "vscode";
import type { FrontendCapabilities } from "./frontend_capabilities";
import type { RuntimeBackend, RuntimeBackendFactory, RuntimeFrame } from "./runtime_backend";
import type { Bit8Button, HostMessage } from "./runtime_protocol";
import { HeldButtonState } from "./input_state";

const FRAME_MS = 1000 / 60;
type Output = vscode.OutputChannel;

/** Explorer surface owns presentation state and the one active Bit8 host session. */
export class Bit8GameSurfaceProvider implements vscode.WebviewViewProvider {
  private view: vscode.WebviewView | undefined;
  private viewReady = false;
  private client: RuntimeBackend | undefined;
  private latestFrame: RuntimeFrame | undefined;
  private heldButtons: Bit8Button[] = [];
  private generation = 0;

  constructor(
    private readonly output: Output,
    private readonly onRunningChanged: (running: boolean) => void,
    private readonly capabilities: Readonly<FrontendCapabilities>,
    private readonly createRuntimeBackend?: RuntimeBackendFactory,
  ) {}

  resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    this.viewReady = false;
    view.webview.options = { enableScripts: true, localResourceRoots: [] };
    view.webview.html = gameSurfaceHtml(view.webview);
    view.webview.onDidReceiveMessage((message: unknown) => this.handleMessage(message));
    view.onDidChangeVisibility(() => {
      if (!view.visible) this.clearInput();
      else this.postLatestFrame();
    });
    view.onDidDispose(() => {
      if (this.view === view) {
        this.view = undefined;
        this.viewReady = false;
      }
      this.clearInput();
      // Hiding/disposal of the view is not a request to stop the host session.
    });
  }

  async run(projectUri: string): Promise<void> {
    const generation = ++this.generation;
    await this.stopCurrentSession();
    if (generation !== this.generation) return;

    if (!this.capabilities.canRunNativeRuntime || !this.createRuntimeBackend) {
      throw new Error("This Bit8 frontend cannot run the native Bit8 runtime.");
    }

    this.output.appendLine(`Starting Bit8 runtime backend for ${projectUri}.`);
    let client: RuntimeBackend;
    try {
      client = this.createRuntimeBackend(projectUri, (line) => this.output.appendLine(line));
    } catch (error) {
      this.onRunningChanged(false);
      throw error;
    }
    this.client = client;
    this.latestFrame = undefined;
    this.clearInput();
    this.onRunningChanged(true);

    try {
      await client.waitUntilReady();
      if (generation !== this.generation || this.client !== client) return;
      void this.pumpFrames(client, generation);
      await vscode.commands.executeCommand("workbench.view.explorer");
      this.view?.show(true);
    } catch (error) {
      if (this.client === client) {
        this.client = undefined;
        this.onRunningChanged(false);
      }
      await client.stop();
      if (generation === this.generation) throw error;
    }
  }

  async stop(): Promise<void> {
    ++this.generation;
    await this.stopCurrentSession();
  }

  async dispose(): Promise<void> {
    await this.stop();
  }

  private async stopCurrentSession(): Promise<void> {
    const client = this.client;
    this.client = undefined;
    this.clearInput();
    this.post({ type: "clearInput" });
    this.onRunningChanged(false);
    if (client) await client.stop();
  }

  private async pumpFrames(client: RuntimeBackend, generation: number): Promise<void> {
    while (this.client === client && generation === this.generation && client.running) {
      const startedAt = Date.now();
      try {
        const frame = await client.step([...this.heldButtons]);
        if (this.client !== client || generation !== this.generation) return;
        this.latestFrame = frame;
        this.postLatestFrame();
      } catch (error) {
        if (this.client === client && generation === this.generation) {
          const message = error instanceof Error ? error.message : String(error);
          this.output.appendLine(`Bit8 game error: ${message}`);
          void vscode.window.showErrorMessage(`Bit8 game error: ${message}`);
          await this.stopCurrentSession();
        }
        return;
      }
      const delay = Math.max(0, FRAME_MS - (Date.now() - startedAt));
      if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
    }
    if (this.client === client && generation === this.generation && !client.running) {
      this.client = undefined;
      this.clearInput();
      this.onRunningChanged(false);
    }
  }

  private handleMessage(value: unknown): void {
    if (!value || typeof value !== "object" || !("type" in value)) return;
    const message = value as Record<string, unknown>;
    if (message.type === "ready") {
      this.viewReady = true;
      this.postLatestFrame();
    } else if (message.type === "input") {
      this.heldButtons = toBit8Buttons(message.keys);
    } else if (message.type === "blur") {
      this.clearInput();
    }
  }

  private clearInput(): void {
    this.heldButtons = [];
  }

  private postLatestFrame(): void {
    if (!this.viewReady || !this.view?.visible || !this.latestFrame) return;
    void this.view.webview.postMessage(this.latestFrame);
  }

  private post(message: unknown): void {
    if (this.viewReady && this.view?.visible) void this.view.webview.postMessage(message);
  }
}

function toBit8Buttons(keys: unknown): Bit8Button[] {
  const state = new HeldButtonState();
  return state.buttonsForKeys(keys);
}

function gameSurfaceHtml(webview: vscode.Webview): string {
  const nonce = createNonce();
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
  <style nonce="${nonce}">
    html, body, #surface { width: 100%; height: 100%; margin: 0; overflow: hidden; }
    body { background: var(--vscode-sideBar-background); }
    #surface { display: flex; align-items: flex-start; justify-content: flex-start; width: 100%; height: 100%; padding: 0; }
    canvas { display: block; width: 64px; height: 64px; image-rendering: pixelated; image-rendering: crisp-edges; outline: none; }
  </style>
</head>
<body>
  <div id="surface"><canvas id="framebuffer" width="64" height="64" tabindex="0" aria-label="Bit8 game framebuffer"></canvas></div>
  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    const surface = document.getElementById('surface');
    const canvas = document.getElementById('framebuffer');
    const context = canvas.getContext('2d', { alpha: false });
    const held = new Set();
    const keys = new Set(['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'KeyZ', 'KeyX']);
    context.imageSmoothingEnabled = false;

    function sendInput() { vscode.postMessage({ type: 'input', keys: Array.from(held) }); }
    function clearInput() { held.clear(); sendInput(); }
    function resizeCanvas() {
      const size = Math.max(0, Math.min(surface.clientWidth, surface.clientHeight));
      canvas.style.width = size + 'px';
      canvas.style.height = size + 'px';
    }
    new ResizeObserver(resizeCanvas).observe(surface);
    resizeCanvas();
    canvas.addEventListener('keydown', (event) => {
      if (!keys.has(event.code) || document.activeElement !== canvas || !document.hasFocus()) return;
      event.preventDefault();
      if (!held.has(event.code)) { held.add(event.code); sendInput(); }
    });
    canvas.addEventListener('keyup', (event) => {
      if (!keys.has(event.code)) return;
      if (held.delete(event.code)) sendInput();
    });
    canvas.addEventListener('click', () => canvas.focus());
    canvas.addEventListener('blur', clearInput);
    window.addEventListener('blur', clearInput);
    document.addEventListener('visibilitychange', () => { if (document.hidden) clearInput(); });
    window.addEventListener('message', (event) => {
      const message = event.data;
      if (message.type === 'clearInput') { clearInput(); return; }
      if (message.type !== 'frame' || !Array.isArray(message.pixels) || message.pixels.length !== 4096) return;
      const image = context.createImageData(64, 64);
      for (let i = 0; i < 4096; i++) {
        const color = message.pixels[i] >>> 0;
        const offset = i * 4;
        image.data[offset] = (color >>> 16) & 255;
        image.data[offset + 1] = (color >>> 8) & 255;
        image.data[offset + 2] = color & 255;
        image.data[offset + 3] = 255;
      }
      context.putImageData(image, 0, 0);
    });
    vscode.postMessage({ type: 'ready' });
  </script>
</body>
</html>`;
}

function createNonce(): string {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  return Array.from({ length: 32 }, () => alphabet[Math.floor(Math.random() * alphabet.length)]).join("");
}
