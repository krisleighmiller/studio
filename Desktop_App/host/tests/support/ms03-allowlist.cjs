const { test } = require("node:test");
const assert = require("node:assert/strict");
const { HOST_COMMANDS } = require("../../../electron/commandAllowlist.cjs");
test("MS03 admits only the exact typed model-sheet command", () => {
  assert.equal(HOST_COMMANDS.has("start_character_model_sheet"), true);
  for (const command of ["start_character_model_sheet_unchecked", "run_model_sheet", "model_sheet"]) {
    assert.equal(HOST_COMMANDS.has(command), false);
  }
});
