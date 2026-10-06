#!/usr/bin/env node
const readline = require("node:readline");
const fs = require("node:fs");

if (process.argv[2] !== "host") process.exit(2);
if (process.env.BIT8_TEST_STALE) {
  console.error("bit8: unknown command 'host'");
  process.exit(2);
}
if (process.env.BIT8_TEST_ARGS) {
  fs.appendFileSync(process.env.BIT8_TEST_ARGS, `${process.argv.slice(2).join("\n")}\n`);
}

let seq = 0;
console.log(JSON.stringify({ type: "ready", width: 64, height: 64 }));
const input = readline.createInterface({ input: process.stdin });
input.on("line", (line) => {
  const message = JSON.parse(line);
  if (message.type === "stop") {
    input.close();
    process.stdout.write(`${JSON.stringify({ type: "exit" })}\n`, () => process.exit(0));
  }
  if (message.type === "step") {
    setTimeout(() => {
      seq += 1;
      console.log(JSON.stringify({ type: "frame", seq, width: 64, height: 64, pixels: new Array(4096).fill(0) }));
    }, Number(process.env.BIT8_TEST_FRAME_DELAY ?? 0));
  }
});
