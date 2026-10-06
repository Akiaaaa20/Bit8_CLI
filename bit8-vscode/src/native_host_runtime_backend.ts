import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createInterface, type Interface as ReadlineInterface } from "node:readline";
import type { RuntimeBackend, RuntimeBackendFactory, RuntimeFrame, RuntimeLog } from "./runtime_backend";
import { parseHostMessage, type Bit8Button } from "./runtime_protocol";

type Output = { appendLine(value: string): void };

export const nativeHostRuntimeBackendFactory: RuntimeBackendFactory = (projectUri, log) =>
  NativeHostRuntimeBackend.launch(projectUri, { appendLine: log });

/** Native-only adapter for `bit8 host <project>` and its existing NDJSON protocol. */
export class NativeHostRuntimeBackend implements RuntimeBackend {
  private readonly child: ChildProcessWithoutNullStreams;
  private readonly lines: ReadlineInterface;
  private readonly ready: Promise<void>;
  private resolveReady!: () => void;
  private rejectReady!: (error: Error) => void;
  private pendingFrame: { resolve: (frame: RuntimeFrame) => void; reject: (error: Error) => void } | undefined;
  private resolveExit!: () => void;
  private readonly exited: Promise<void>;
  private hasExited = false;
  private receivedExit = false;
  private stopping = false;
  private readyReceived = false;
  private readonly stderrTail: string[] = [];

  get running(): boolean {
    return !this.hasExited && !this.receivedExit;
  }

  private constructor(projectPath: string, private readonly output: Output, executable: string) {
    this.ready = new Promise<void>((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    this.exited = new Promise<void>((resolve) => {
      this.resolveExit = resolve;
    });
    this.child = spawn(executable, ["host", projectPath], {
      cwd: projectPath,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.lines = createInterface({ input: this.child.stdout });
    this.lines.on("line", (line) => this.handleLine(line));
    this.child.stderr.on("data", (data: Buffer) => {
      for (const line of data.toString().split(/\r?\n/)) {
        if (line.length > 0) {
          this.stderrTail.push(line);
          if (this.stderrTail.length > 20) this.stderrTail.shift();
          this.output.appendLine(`[bit8 host] ${line}`);
        }
      }
    });
    this.child.on("error", (error: NodeJS.ErrnoException) => {
      const message = error.code === "ENOENT"
        ? "Bit8 runtime was not found in PATH. Install the Bit8 runtime first, then restart VS Code."
        : `Could not start Bit8 host: ${error.message}`;
      this.fail(new Error(message));
    });
    this.child.on("close", (code, signal) => {
      this.hasExited = true;
      this.lines.close();
      this.resolveExit();
      this.output.appendLine(`Bit8 host exited (code ${code ?? "none"}, signal ${signal ?? "none"}).`);
      const ended = new Error(`Bit8 host exited (code ${code ?? "none"}, signal ${signal ?? "none"}).`);
      if (!this.readyReceived) {
        const detail = this.stderrTail.length > 0 ? ` stderr: ${this.stderrTail.join(" | ")}` : "";
        this.fail(new Error(`Bit8 host exited before ready (${ended.message})${detail}`));
      } else if (!this.stopping) this.rejectPending(ended);
    });
  }

  static async start(projectUri: string, output: Output, executable = "bit8"): Promise<NativeHostRuntimeBackend> {
    const backend = NativeHostRuntimeBackend.launch(projectUri, output, executable);
    try {
      await backend.waitUntilReady();
      return backend;
    } catch (error) {
      await backend.stop();
      throw error;
    }
  }

  static launch(projectUri: string, output: Output, executable = "bit8"): NativeHostRuntimeBackend {
    return new NativeHostRuntimeBackend(localPathFromUri(projectUri), output, executable);
  }

  async waitUntilReady(): Promise<void> {
    await this.ready;
    this.output.appendLine("Bit8 host ready (64x64 framebuffer).");
  }

  step(buttons: Bit8Button[]): Promise<RuntimeFrame> {
    if (this.hasExited || this.receivedExit) return Promise.reject(new Error("Bit8 host is not running."));
    if (this.pendingFrame) return Promise.reject(new Error("A Bit8 frame request is already in flight."));

    return new Promise((resolve, reject) => {
      this.pendingFrame = { resolve, reject };
      this.child.stdin.write(`${JSON.stringify({ type: "step", buttons })}\n`, (error) => {
        if (error) this.rejectPending(new Error(`Could not send a frame to Bit8 host: ${error.message}`));
      });
    });
  }

  async stop(): Promise<void> {
    if (this.hasExited) return;
    this.stopping = true;
    if (!this.readyReceived) this.rejectReady(new Error("Bit8 game stopped during startup."));
    this.rejectPending(new Error("Bit8 game stopped."));
    if (!this.child.stdin.destroyed && !this.child.stdin.writableEnded) {
      this.child.stdin.write('{"type":"stop"}\n');
    }
    if (await waitFor(this.exited, 1000)) return;
    this.child.kill();
    await waitFor(this.exited, 1000);
  }

  private handleLine(line: string): void {
    try {
      const message = parseHostMessage(line);
      if (message.type === "ready") {
        this.readyReceived = true;
        this.resolveReady();
      } else if (message.type === "frame") {
        if (!this.pendingFrame) throw new Error("Bit8 host returned a frame without a pending request.");
        const pending = this.pendingFrame;
        this.pendingFrame = undefined;
        pending.resolve(message);
      } else if (message.type === "error") {
        const error = new Error(message.message);
        if (!this.readyReceived) this.rejectReady(error);
        this.rejectPending(error);
      } else if (message.type === "exit") {
        this.receivedExit = true;
        this.rejectPending(new Error("Bit8 host exited."));
      }
    } catch (error) {
      this.fail(error instanceof Error ? error : new Error(String(error)));
    }
  }

  private fail(error: Error): void {
    this.output.appendLine(error.message);
    this.rejectReady(error);
    this.rejectPending(error);
    if (!this.hasExited) this.child.kill();
  }

  private rejectPending(error: Error): void {
    if (this.pendingFrame) {
      const pending = this.pendingFrame;
      this.pendingFrame = undefined;
      pending.reject(error);
    }
  }
}

function localPathFromUri(projectUri: string): string {
  let url: URL;
  try {
    url = new URL(projectUri);
  } catch {
    throw new Error("The native Bit8 runtime requires a local file project.");
  }
  if (url.protocol !== "file:") throw new Error("The native Bit8 runtime is unavailable for this workspace resource.");
  return fileURLToPath(url);
}

async function waitFor(promise: Promise<void>, milliseconds: number): Promise<boolean> {
  let timer: NodeJS.Timeout | undefined;
  const result = await Promise.race([
    promise.then(() => true),
    new Promise<boolean>((resolve) => { timer = setTimeout(() => resolve(false), milliseconds); }),
  ]);
  if (timer) clearTimeout(timer);
  return result;
}
