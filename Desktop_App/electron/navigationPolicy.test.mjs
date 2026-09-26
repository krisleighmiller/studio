import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const {
  isAppUrl,
  externalUrl,
  windowOpenAction,
  devOrigins,
  externalOpenDecision,
  devServerUrl,
  launchTarget,
  contentSecurityPolicy,
} = require("./navigationPolicy.cjs");

const appIndex = "/home/kris/Content_Creation/Studio/Desktop_App/dist/index.html";

test("dev server origins stay in the app", () => {
  assert.equal(isAppUrl("http://127.0.0.1:1420/", { appIndex }), true);
  assert.equal(isAppUrl("http://localhost:1420/index.html", { appIndex }), false);
  assert.equal(isAppUrl("https://example.com/", { appIndex }), false);
  assert.equal(isAppUrl("http://127.0.0.1:9999/", { appIndex }), false);
});

test("only the packaged index file is an app file URL", () => {
  assert.equal(isAppUrl(pathToFileURL(appIndex).href, { appIndex }), true);
  assert.equal(isAppUrl("file:///tmp/other.html", { appIndex }), false);
});

test("external links are http(s) only", () => {
  assert.equal(externalUrl("https://example.com/docs"), "https://example.com/docs");
  assert.equal(externalUrl("http://example.com"), "http://example.com/");
  assert.equal(externalUrl("javascript:alert(1)"), null);
  assert.equal(externalUrl("file:///etc/passwd"), null);
  assert.equal(externalUrl("https://user:pass@example.com"), null);
});

test("window.open is denied", () => {
  assert.equal(windowOpenAction(), "deny");
});

test("preview links open externally only after confirm", () => {
  const target = "https://example.com/docs";
  assert.equal(externalOpenDecision(target, false).open, false);
  assert.equal(externalOpenDecision(target, false).reason, "cancelled");
  assert.deepEqual(externalOpenDecision(target, true), { open: true, url: "https://example.com/docs" });
  assert.equal(externalOpenDecision("javascript:alert(1)", true).open, false);
});

test("the Electron window stays on the app", () => {
  assert.equal(isAppUrl("https://example.com/docs", { appIndex }), false);
  assert.equal(isAppUrl("http://127.0.0.1:1420/index.html", { appIndex }), true);
});

test("remote preview images are blocked by the content security policy", () => {
  const policy = contentSecurityPolicy(false);
  assert.match(policy, /img-src 'self' data: blob: workbench-media:/);
  assert.doesNotMatch(policy, /img-src[^;]*https:/);
  assert.doesNotMatch(policy, /img-src[^;]*http:/);
});

test("a packaged or non-dev launch loads the built index", () => {
  const distIndex = "/opt/studio/dist/index.html";
  const packaged = launchTarget({
    dev: true,
    packaged: true,
    env: { VITE_DEV_SERVER_URL: "http://127.0.0.1:1420" },
    distIndex,
  });
  assert.equal(packaged.kind, "file");
  assert.equal(packaged.file, distIndex);
  const production = launchTarget({ dev: false, packaged: false, distIndex });
  assert.equal(production.kind, "file");
  assert.equal(production.file, distIndex);
  assert.equal(JSON.stringify(packaged).includes("127.0.0.1"), false);
});

test("a dev launch stays on the loopback server", () => {
  const target = launchTarget({
    dev: true,
    packaged: false,
    env: { VITE_DEV_SERVER_URL: "http://example.com:1420" },
    distIndex: "/opt/studio/dist/index.html",
  });
  assert.deepEqual(target, { kind: "url", url: "http://127.0.0.1:1420" });
  assert.equal(
    devServerUrl({ VITE_DEV_SERVER_URL: "http://127.0.0.1:4310" }),
    "http://127.0.0.1:4310",
  );
});

test("a non-local dev server override is ignored", () => {
  const origins = devOrigins({ VITE_DEV_SERVER_URL: "https://example.com" });
  assert.equal(origins.has("https://example.com"), false);
  assert.equal(origins.has("http://127.0.0.1:1420"), true);
});
