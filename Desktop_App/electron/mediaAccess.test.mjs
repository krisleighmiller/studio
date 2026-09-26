import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { mediaAllowed } = require("./mediaAccess.cjs");

const project = "/projects/Lite_Challenge";

test("media stays inside the open project", () => {
  const options = { sessionRoot: project, grants: [], realPath: (value) => value };
  assert.equal(mediaAllowed(`${project}/Renders/shot.png`, options), true);
  assert.equal(mediaAllowed(`${project}-evil/shot.png`, options), false);
  assert.equal(mediaAllowed("/etc/passwd.png", options), false);
  assert.equal(mediaAllowed(`${project}/Notes/secret.txt`, options), false);
});

test("a grant can read one outside file and nothing beside it", () => {
  const grant = "/tmp/dialog/ref.png";
  const options = { sessionRoot: project, grants: [grant], realPath: (value) => value };
  assert.equal(mediaAllowed(grant, options), true);
  assert.equal(mediaAllowed("/tmp/dialog/other.png", options), false);
});

test("a symlink that leaves the project is refused", () => {
  const options = {
    sessionRoot: project,
    grants: [],
    realPath: (value) => (value.endsWith("link.png") ? "/outside/secret.png" : value),
  };
  assert.equal(mediaAllowed(`${project}/link.png`, options), false);
});

test("settings files stay out of the media protocol", () => {
  const settings = "/home/kris/.config/video-workbench/config/settings.json";
  const preview = "/home/kris/.config/video-workbench/config/preview.png";
  const options = {
    sessionRoot: project,
    grants: ["/home/kris/.config/video-workbench/config"],
    realPath: (value) => value,
  };
  assert.equal(mediaAllowed(settings, options), false);
  assert.equal(mediaAllowed(preview, options), false);
});
