import assert from "node:assert/strict";
import test from "node:test";
import { hostBinaryCandidates, resolveHostBinary } from "./sidecar.mjs";

test("a non-dev launch ignores the host override and debug builds", () => {
  const root = "/opt/studio";
  const candidates = hostBinaryCandidates({
    dev: false,
    env: { WORKBENCH_HOST_BIN: "/tmp/injected-host", WORKBENCH_DEV: "1" },
    resourcesPath: "/opt/studio/resources",
    root,
  });
  assert.deepEqual(candidates, [
    "/opt/studio/resources/workbench-host",
    "/opt/studio/host/target/release/workbench-host",
  ]);
  const resolved = resolveHostBinary({
    packaged: true,
    env: { WORKBENCH_DEV: "1", WORKBENCH_HOST_BIN: "/tmp/injected-host" },
    exists: () => false,
    resourcesPath: "/opt/studio/resources",
    root,
  });
  assert.equal(resolved, "/opt/studio/host/target/release/workbench-host");
});

test("a dev launch can use an explicit host binary", () => {
  const resolved = resolveHostBinary({
    packaged: false,
    env: { WORKBENCH_DEV: "1", WORKBENCH_HOST_BIN: "/tmp/dev-host" },
    exists: (candidate) => candidate === "/tmp/dev-host",
    root: "/opt/studio",
  });
  assert.equal(resolved, "/tmp/dev-host");
});
