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

module.exports = {
  devOrigins,
  isAppUrl,
  externalUrl,
  windowOpenAction,
};
