const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { pathToFileURL } = require("node:url");
const { after, test } = require("node:test");
const { NativeHostRuntimeBackend } = require("../out/native_host_runtime_backend");
const { parseHostMessage } = require("../out/runtime_protocol");
const { HeldButtonState } = require("../out/input_state");

const tempDirectories = [];
after(() => {
  for (const directory of tempDirectories) fs.rmSync(directory, { recursive: true, force: true });
});

test("NDJSON messages are parsed and framebuffer shape is validated", () => {
  assert.deepEqual(parseHostMessage('{"type":"ready","width":64,"height":64}'), {
    type: "ready", width: 64, height: 64,
  });
  assert.deepEqual(parseHostMessage('{"type":"error","message":"main.b8:3: bad call"}'), {
    type: "error", message: "main.b8:3: bad call",
  });
  assert.throws(() => parseHostMessage("not json"));
  assert.throws(() => parseHostMessage('{"type":"frame","seq":1,"width":64,"height":64,"pixels":[]}'), /invalid 64x64/);
  assert.throws(() => parseHostMessage('{"type":"mystery"}'), /unknown or invalid/);
});

test("held physical keys map to the complete Bit8 button state", () => {
  const input = new HeldButtonState();
  assert.deepEqual(input.buttonsForKeys(["ArrowUp", "ArrowRight", "KeyZ", "KeyX", "F1"]), ["UP", "RIGHT", "A", "B"]);
  assert.deepEqual(input.buttonsForKeys(["ArrowLeft"]), ["LEFT"]);
  assert.deepEqual(input.buttonsForKeys([]), []);
});

test("native host backend waits for frames, enforces one in-flight request, and stops cleanly", async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-host-client-test-"));
  tempDirectories.push(directory);
  const executable = path.join(directory, "bit8");
  fs.copyFileSync(path.join(__dirname, "fixtures", "mock-bit8.js"), executable);
  fs.chmodSync(executable, 0o755);
  process.env.BIT8_TEST_FRAME_DELAY = "75";
  const outputLines = [];

  try {
    const client = await NativeHostRuntimeBackend.start(pathToFileURL(directory).toString(), { appendLine: (line) => outputLines.push(line) }, executable);
    assert.ok(outputLines.some((line) => line.includes("host ready")));
    const first = client.step(["RIGHT"]);
    await assert.rejects(client.step(["RIGHT", "A"]), /already in flight/);
    const frame = await first;
    assert.equal(frame.seq, 1);
    assert.equal(frame.pixels.length, 4096);
    await client.stop();
    assert.equal(client.running, false);
    assert.ok(outputLines.some((line) => line.includes("host exited")));
  } finally {
    delete process.env.BIT8_TEST_FRAME_DELAY;
  }
});

test("native host backend includes CLI stderr when startup exits before ready", async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "bit8-stale-cli-test-"));
  tempDirectories.push(directory);
  const executable = path.join(directory, "bit8");
  fs.copyFileSync(path.join(__dirname, "fixtures", "mock-bit8.js"), executable);
  fs.chmodSync(executable, 0o755);
  process.env.BIT8_TEST_STALE = "1";
  try {
    await assert.rejects(
      NativeHostRuntimeBackend.start(pathToFileURL(directory).toString(), { appendLine: () => {} }, executable),
      /unknown command 'host'/,
    );
  } finally {
    delete process.env.BIT8_TEST_STALE;
  }
});
