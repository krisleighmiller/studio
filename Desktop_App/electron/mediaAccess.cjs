const path = require("node:path");

const MEDIA_EXTENSIONS = new Set([
  ".mp4",
  ".webm",
  ".mov",
  ".mkv",
  ".m4v",
  ".wav",
  ".mp3",
  ".ogg",
  ".flac",
  ".m4a",
  ".aac",
  ".png",
  ".jpg",
  ".jpeg",
  ".webp",
  ".gif",
  ".bmp",
]);

function mediaMime(filePath) {
  switch (path.extname(filePath).toLowerCase()) {
    case ".mp4":
      return "video/mp4";
    case ".webm":
      return "video/webm";
    case ".mov":
      return "video/quicktime";
    case ".mkv":
      return "video/x-matroska";
    case ".m4v":
      return "video/x-m4v";
    case ".wav":
      return "audio/wav";
    case ".mp3":
      return "audio/mpeg";
    case ".ogg":
      return "audio/ogg";
    case ".flac":
      return "audio/flac";
    case ".m4a":
      return "audio/mp4";
    case ".aac":
      return "audio/aac";
    case ".png":
      return "image/png";
    case ".jpg":
    case ".jpeg":
      return "image/jpeg";
    case ".webp":
      return "image/webp";
    case ".gif":
      return "image/gif";
    case ".bmp":
      return "image/bmp";
    default:
      return "application/octet-stream";
  }
}

function isInside(root, target) {
  if (!root || !target) return false;
  const rel = path.relative(path.resolve(root), path.resolve(target));
  return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
}

function isSettingsPath(filePath) {
  const parts = path.resolve(filePath).split(path.sep);
  const configAt = parts.lastIndexOf("config");
  if (configAt < 1) return false;
  return parts[configAt - 1] === "video-workbench";
}

function mediaAllowed(filePath, options = {}) {
  const resolve = options.realPath || ((value) => path.resolve(String(value)));
  const resolved = resolve(filePath);
  if (!resolved) return false;
  if (!MEDIA_EXTENSIONS.has(path.extname(resolved).toLowerCase())) return false;
  if (isSettingsPath(resolved)) return false;
  if (options.sessionRoot && isInside(options.sessionRoot, resolved)) return true;
  for (const grant of options.grants || []) {
    if (isInside(grant, resolved)) return true;
  }
  return false;
}

module.exports = {
  mediaMime,
  mediaAllowed,
  isInside,
  isSettingsPath,
};
