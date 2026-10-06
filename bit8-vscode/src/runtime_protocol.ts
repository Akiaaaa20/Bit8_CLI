export type Bit8Button = "UP" | "DOWN" | "LEFT" | "RIGHT" | "A" | "B";

export type HostMessage =
  | { type: "ready"; width: 64; height: 64 }
  | { type: "frame"; seq: number; width: 64; height: 64; pixels: number[] }
  | { type: "error"; message: string }
  | { type: "exit" };

/** Pure NDJSON payload validation shared by any frontend backend adapter. */
export function parseHostMessage(line: string): HostMessage {
  const value: unknown = JSON.parse(line);
  if (!value || typeof value !== "object" || !("type" in value)) {
    throw new Error("Bit8 host sent a message without a type.");
  }

  const message = value as Record<string, unknown>;
  if (message.type === "ready" && message.width === 64 && message.height === 64) {
    return { type: "ready", width: 64, height: 64 };
  }
  if (message.type === "frame") {
    const pixels = message.pixels;
    if (
      typeof message.seq === "number" && Number.isSafeInteger(message.seq) &&
      message.width === 64 && message.height === 64 && Array.isArray(pixels) &&
      pixels.length === 64 * 64 && pixels.every(isU32)
    ) {
      return { type: "frame", seq: message.seq, width: 64, height: 64, pixels };
    }
    throw new Error("Bit8 host sent an invalid 64x64 framebuffer message.");
  }
  if (message.type === "error" && typeof message.message === "string") {
    return { type: "error", message: message.message };
  }
  if (message.type === "exit") return { type: "exit" };
  throw new Error(`Bit8 host sent an unknown or invalid message type: ${String(message.type)}.`);
}

function isU32(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 0 && value <= 0xffffffff;
}
