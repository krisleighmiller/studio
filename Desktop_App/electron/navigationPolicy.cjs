const { pathToFileURL } = require("node:url");
const path = require("node:path");

function devOrigins(env = process.env) {
  const origins = new Set(["http://127.0.0.1:1420"]);
  const extra = env.VITE_DEV_SERVER_URL;
  if (extra) {
    try {
      const url = new URL(extra);
      if (url.hostname === "127.0.0.1" && url.protocol === "http:") {
        origins.add(url.origin);
      }
    } catch {
      // Ignore a malformed dev-server override.
    }
  }
  return origins;
}

function isAppUrl(target, options = {}) {
  let url;
  try {
    url = new URL(target);
  } catch {
    return false;
  }
  if (url.protocol === "file:") {
    if (!options.appIndex) return false;
    const appUrl = pathToFileURL(path.resolve(options.appIndex));
    return url.pathname === appUrl.pathname;
  }
  const origins = options.devOrigins || devOrigins(options.env);
  return origins.has(url.origin);
}

function externalUrl(target) {
  let url;
  try {
    url = new URL(String(target || ""));
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  if (url.username || url.password) return null;
  return url.toString();
}

function windowOpenAction() {
  return "deny";
}

function externalOpenDecision(target, confirmed) {
  const url = externalUrl(target);
  if (!url) return { open: false, reason: "blocked" };
  if (!confirmed) return { open: false, reason: "cancelled" };
  return { open: true, url };
}

function devServerUrl(env = process.env) {
  const fallback = "http://127.0.0.1:1420";
  const extra = env.VITE_DEV_SERVER_URL;
  if (!extra) return fallback;
  try {
    const url = new URL(extra);
    if (url.protocol === "http:" && url.hostname === "127.0.0.1") return url.origin;
  } catch {
    // Ignore a malformed override and stay on the loopback dev server.
  }
  return fallback;
}

function launchTarget({ dev, packaged, env = process.env, distIndex }) {
  if (dev && !packaged) {
    return { kind: "url", url: devServerUrl(env) };
  }
  return { kind: "file", file: path.resolve(distIndex) };
}

function contentSecurityPolicy(isDev) {
  return [
    "default-src 'self'",
    "script-src 'self'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob: workbench-media:",
    "media-src 'self' blob: workbench-media:",
    isDev
      ? "connect-src 'self' http://127.0.0.1:1420 ws://127.0.0.1:1420"
      : "connect-src 'self'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'none'",
    "frame-src 'none'",
  ].join("; ");
}

module.exports = {
  devOrigins,
  isAppUrl,
  externalUrl,
  windowOpenAction,
  externalOpenDecision,
  devServerUrl,
  launchTarget,
  contentSecurityPolicy,
};
