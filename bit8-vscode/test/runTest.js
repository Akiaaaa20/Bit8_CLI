const path = require("node:path");
const { runTests } = require("@vscode/test-electron");

const extensionDevelopmentPath = path.resolve(__dirname, "..");
const languageServerName = process.platform === "win32"
  ? "bit8-language-server.exe"
  : "bit8-language-server";
process.env.BIT8_LANGUAGE_SERVER ??= path.resolve(
  extensionDevelopmentPath,
  "../language-service/target/debug",
  languageServerName,
);
const options = {
  extensionDevelopmentPath,
  extensionTestsPath: path.resolve(__dirname, "suiteRunner.js"),
  launchArgs: [extensionDevelopmentPath, "--disable-workspace-trust", "--disable-gpu"],
};

if (process.env.VSCODE_EXECUTABLE) {
  options.vscodeExecutablePath = process.env.VSCODE_EXECUTABLE;
}

runTests(options).catch((error) => {
  console.error(error);
  process.exit(1);
});
