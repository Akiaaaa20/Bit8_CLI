import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";

/** Local-process boundary: the CLI currently requires a native Bit8 executable and filesystem paths. */
export function runNativeBit8Cli(projectPath: string, args: string[], maxBuffer = 4 * 1024 * 1024): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile("bit8", args, { cwd: projectPath, encoding: "utf8", maxBuffer }, (error, stdout, stderr) => {
      if (error) {
        const detail = stderr.trim() || error.message;
        reject(new Error(error.code === "ENOENT"
          ? "Bit8 CLI was not found in PATH. Install the Bit8 runtime CLI, then reload VS Code."
          : detail));
      } else {
        resolve(stdout);
      }
    });
  });
}

/** Convert a workspace URI only at the boundary that launches the native CLI. */
export function nativeFilePath(uri: string): string {
  let url: URL;
  try {
    url = new URL(uri);
  } catch {
    throw new Error("The Bit8 CLI currently requires local file resources.");
  }
  if (url.protocol !== "file:") throw new Error("The Bit8 CLI currently requires local file resources.");
  return fileURLToPath(url);
}
