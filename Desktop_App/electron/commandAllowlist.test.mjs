import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { HOST_COMMANDS } = require("./commandAllowlist.cjs");

test("v1 paid-media confirmation and direct voice commands cross the Electron boundary", () => {
  for (const command of [
    "confirm_spend",
    "confirm_unbounded_spend",
    "media_generate_confirmed",
    "media_recovery_status",
    "elevenlabs_quote_voice_spend",
    "elevenlabs_confirm_unbounded_voice_spend",
    "workflow_set_budget",
  ]) {
    assert.equal(HOST_COMMANDS.has(command), true, `${command} must be allowlisted`);
  }
});

test("the allowlist remains deny-by-default", () => {
  assert.equal(HOST_COMMANDS.has("arbitrary_host_command"), false);
  assert.equal(HOST_COMMANDS.has("shell:openPath"), false);
});
