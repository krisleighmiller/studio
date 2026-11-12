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

test("shot dialogue import crosses only the admitted native command boundary", () => {
  assert.equal(HOST_COMMANDS.has("workflow_import_performance"), true);
  assert.equal(HOST_COMMANDS.has("workflow_generate_performance"), false);
});

test("authored script-dialogue resolution is admitted without opening generic host commands", () => {
  assert.equal(HOST_COMMANDS.has("workflow_resolve_script_dialogue"), true);
  assert.equal(HOST_COMMANDS.has("workflow_resolve_script_dialogue_unchecked"), false);
});

test("Phase 4 workflow commands stay allowlisted (removal must fail CI)", () => {
  for (const command of [
    // Chain: approval head, dependent render, continuity, assembly, reconcile, QC.
    "workflow_approve_intent",
    "workflow_approve_animatic",
    "workflow_set_visual_source",
    "workflow_prepare_render",
    "workflow_bind_continuity_frame",
    "workflow_set_take_status",
    "workflow_prepare_assemble",
    "workflow_check_deliverable",
    // Desk plumbing: import, and workflow_status which reconciles job state
    // from on-disk evidence on read.
    "workflow_import",
    "workflow_status",
    // Review-export opening reaches the app file-open path through
    // classify_file; opening a media tab also reads it.
    "classify_file",
    "read_media_bytes",
  ]) {
    assert.equal(HOST_COMMANDS.has(command), true, `${command} must stay allowlisted for Phase 4`);
  }
});
