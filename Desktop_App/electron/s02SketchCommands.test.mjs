import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { HOST_COMMANDS } = require("./commandAllowlist.cjs");

test("s02 sketch trim and transition commands are allowlisted", () => {
  for (const command of [
    "workflow_set_shot_trim",
    "workflow_set_shot_transition",
  ]) {
    assert.equal(HOST_COMMANDS.has(command), true, `${command} must be allowlisted`);
  }
});

test("neighboring sketch commands remain allowlisted", () => {
  for (const command of [
    "workflow_save_sketch",
    "workflow_reorder_animatic",
    "workflow_reorder_cuts",
    "workflow_approve_animatic",
    "workflow_attach_scratch_audio",
    "workflow_clear_scratch_audio",
    "workflow_set_animatic_duration",
    "workflow_set_max_cuts",
    "workflow_remove_cut",
    "workflow_reopen_sketch",
  ]) {
    assert.equal(HOST_COMMANDS.has(command), true, `${command} must remain allowlisted`);
  }
});

test("arbitrary unrelated commands remain denied", () => {
  assert.equal(HOST_COMMANDS.has("arbitrary_host_command"), false);
  assert.equal(HOST_COMMANDS.has("shell:openPath"), false);
  assert.equal(HOST_COMMANDS.has("workflow_fake_command"), false);
});
