import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

// Execute the actual preload in its isolated context, not a duplicate bridge.
test("Director operation identity survives preload invoke/events and unsubscribe", async () => {
  const ipcRenderer = new EventEmitter();
  const calls = [];
  ipcRenderer.invoke = async (...args) => { calls.push(args); return { reply: "Actual result" }; };
  let bridge;
  vm.runInNewContext(readFileSync(new URL("./preload.cjs", import.meta.url), "utf8"), {
    require: (name) => {
      assert.equal(name, "electron");
      return { ipcRenderer, contextBridge: { exposeInMainWorld: (name, value) => {
        assert.equal(name, "workbench");
        bridge = value;
      } } };
    },
    location: { href: "file:///fixture/dist/index.html" },
    URL,
    process: { platform: "linux" },
  });
  const received = [];
  const off = bridge.onDirectorProgress((payload) => received.push(payload));
  for (const cmd of ["director_turn", "director_approve"]) {
    const args = { progressId: `identity-${cmd}`, ...(cmd === "director_turn" ? { request: { model: "fixture" } } : { id: "approval-token" }) };
    await bridge.invoke(cmd, args);
    assert.deepEqual(calls.at(-1), ["host:invoke", cmd, args]);
    const payload = { requestId: args.progressId, message: `Progress for ${cmd}` };
    ipcRenderer.emit("director-progress", { sender: "main" }, payload);
    assert.equal(received.at(-1), payload);
  }
  off();
  ipcRenderer.emit("director-progress", {}, { requestId: "late", message: "Late event" });
  assert.equal(received.length, 2);
  assert.equal(ipcRenderer.listenerCount("director-progress"), 0);
});
