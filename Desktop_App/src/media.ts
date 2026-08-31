export type MediaKind = "image" | "video" | "audio";
export type OpenKind = "text" | MediaKind | "unsupported";

export const PLAYBACK_RATES = [0.25, 0.5, 0.75, 1] as const;
export type PlaybackRate = (typeof PLAYBACK_RATES)[number];

export function isMediaKind(kind: string): kind is MediaKind {
  return kind === "image" || kind === "video" || kind === "audio";
}

export function mimeForPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "mp4":
    case "m4v":
      return "video/mp4";
    case "webm":
      return "video/webm";
    case "mov":
      return "video/quicktime";
    case "mkv":
      return "video/x-matroska";
    case "wav":
      return "audio/wav";
    case "mp3":
      return "audio/mpeg";
    case "ogg":
    case "opus":
      return "audio/ogg";
    case "flac":
      return "audio/flac";
    case "m4a":
    case "aac":
      return "audio/mp4";
    case "png":
      return "image/png";
    case "jpg":
    case "jpeg":
      return "image/jpeg";
    case "gif":
      return "image/gif";
    case "webp":
      return "image/webp";
    case "bmp":
      return "image/bmp";
    default:
      return "application/octet-stream";
  }
}

export function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const total = Math.floor(seconds);
  const mins = Math.floor(total / 60);
  const secs = total % 60;
  return `${mins}:${secs.toString().padStart(2, "0")}`;
}

export function nextPlaybackRate(
  current: number,
  direction: 1 | -1,
): PlaybackRate {
  const index = PLAYBACK_RATES.findIndex((rate) => rate === current);
  const from = index === -1 ? PLAYBACK_RATES.indexOf(1) : index;
  const next = Math.min(
    PLAYBACK_RATES.length - 1,
    Math.max(0, from + direction),
  );
  return PLAYBACK_RATES[next];
}

export function normalizePlaybackRate(value: number): PlaybackRate {
  const match = PLAYBACK_RATES.find((rate) => rate === value);
  return match ?? 1;
}
