import assert from "node:assert/strict";
import { test } from "node:test";
import { generateScriptDraft } from "./scriptDraft.mjs";
import { createRequire } from "node:module";
const { HOST_COMMANDS } = createRequire(import.meta.url)("./commandAllowlist.cjs");
test("renderer can request generation but cannot prepare, approve or replay a host snapshot", () => {
  assert.ok(HOST_COMMANDS.has("workflow_generate_script"));
  assert.ok(!HOST_COMMANDS.has("prepare_script_draft"));
  assert.ok(!HOST_COMMANDS.has("finish_script_draft"));
});
test("native denial consumes the host snapshot without renderer approval fields", async () => {
  const calls = [];
  const invoke = async (cmd, args) => {
    calls.push([cmd,args]);
    if (cmd === "prepare_script_draft") return { id: "host-owned", message: "Exact Title + Notes / writer" };
    return { cancelled: true };
  };
  const result = await generateScriptDraft(invoke, async (message, opts) => { assert.equal(message,"Exact Title + Notes / writer"); assert.equal(opts.title,"Generate script from notes"); return false; }, () => true, { title:"Rain", approved:true });
  assert.deepEqual(result,{cancelled:true});
  assert.deepEqual(calls[1],["finish_script_draft",{id:"host-owned",approved:false}]);
});
test("project switch while native dialog is open forces denial even if OK was clicked", async () => {
  const calls=[];
  const invoke=async (cmd,args) => { calls.push([cmd,args]); return cmd === "prepare_script_draft" ? {id:"one",message:"Notes"} : {cancelled:true}; };
  await generateScriptDraft(invoke,async () => true,() => false,{});
  assert.deepEqual(calls[1],["finish_script_draft",{id:"one",approved:false}]);
});
test("approval forwards only host snapshot id, not mutable renderer input", async () => {
  const args={notes:"original"}; const calls=[];
  const invoke=async (cmd,values) => { calls.push([cmd,structuredClone(values)]); return cmd === "prepare_script_draft" ? {id:"two",message:"original"} : {script:"Scene 1\nfixture"}; };
  await generateScriptDraft(invoke,async () => { args.notes="tampered"; return true; },() => true,args);
  assert.deepEqual(calls[1],["finish_script_draft",{id:"two",approved:true}]);
});
