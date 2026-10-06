const path = require("node:path");
const Mocha = require("mocha");

function run() {
  const mocha = new Mocha({ ui: "tdd", color: true, timeout: 10000 });
  mocha.addFile(path.resolve(__dirname, "suite.js"));

  return new Promise((resolve, reject) => {
    mocha.run((failures) => {
      if (failures > 0) {
        reject(new Error(`${failures} Bit8 extension test(s) failed.`));
      } else {
        resolve();
      }
    });
  });
}

module.exports = { run };
