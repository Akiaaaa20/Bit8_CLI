import type { Bit8Button, HostMessage } from "./runtime_protocol";

export type RuntimeFrame = Extract<HostMessage, { type: "frame" }>;

/** Frontend-facing runtime contract. It contains project identity and Bit8 data, never editor concepts. */
export interface RuntimeBackend {
  readonly running: boolean;
  waitUntilReady(): Promise<void>;
  step(buttons: Bit8Button[]): Promise<RuntimeFrame>;
  stop(): Promise<void>;
}

export type RuntimeLog = (message: string) => void;
export type RuntimeBackendFactory = (projectUri: string, log: RuntimeLog) => RuntimeBackend;
