import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const { isAppUrl, externalUrl, windowOpenAction, devOrigins } = require("./navigationPolicy.cjs");

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

test("a non-local dev server override is ignored", () => {
  const origins = devOrigins({ VITE_DEV_SERVER_URL: "https://example.com" });
  assert.equal(origins.has("https://example.com"), false);
  assert.equal(origins.has("http://127.0.0.1:1420"), true);
});
