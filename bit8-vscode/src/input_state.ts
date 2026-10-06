import type { Bit8Button } from "./runtime_protocol";

const keyButtons: Record<string, Bit8Button> = {
  ArrowUp: "UP",
  ArrowDown: "DOWN",
  ArrowLeft: "LEFT",
  ArrowRight: "RIGHT",
  KeyZ: "A",
  KeyX: "B",
};

export class HeldButtonState {
  buttonsForKeys(keys: unknown): Bit8Button[] {
    if (!Array.isArray(keys)) return [];
    const held = new Set<Bit8Button>();
    for (const key of keys) {
      if (typeof key === "string" && Object.hasOwn(keyButtons, key)) held.add(keyButtons[key]);
    }
    return [...held];
  }
}
