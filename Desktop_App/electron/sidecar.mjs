import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/**
 * Resolve the workbench-host binary for dev and packaged builds.
 */
function unpackagedDev() {
  return Boolean(process.defaultApp) || process.env.ELECTRON_IS_DEV === "1";
}

export function resolveHostBinary() {
  if (process.env.WORKBENCH_HOST_BIN) {
    return process.env.WORKBENCH_HOST_BIN;
  }
  const root = path.resolve(__dirname, "..");
  const candidates = [];
  if (process.resourcesPath && !unpackagedDev()) {
    candidates.push(path.join(process.resourcesPath, "workbench-host"));
  }
  // Prefer in-tree builds; also honor CARGO_TARGET_DIR (e.g. CI / sandboxes).
  // Unpackaged `electron .` must prefer debug so `npm run dev:host` wins over a stale release binary.
  const targetRoots = [
    path.join(root, "host", "target"),
    process.env.CARGO_TARGET_DIR,
  ].filter(Boolean);
  for (const targetRoot of targetRoots) {
    const debug = path.join(targetRoot, "debug", "workbench-host");
    const release = path.join(targetRoot, "release", "workbench-host");
    if (unpackagedDev()) {
      candidates.push(debug, release);
    } else {
      candidates.push(release, debug);
    }
  }
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) return candidate;
  }
  return path.join(root, "host", "target", "debug", "workbench-host");
}

export class Sidecar {
  #child = null;
  #pending = new Map();
  #nextId = 1;
  #eventHandlers = new Set();
  #settingsDir;
  #bin = null;
  #binMtime = 0;

  constructor({ settingsDir }) {
    this.#settingsDir = settingsDir;
  }

  start() {
    if (this.#child) return;
    const bin = resolveHostBinary();
    if (!fs.existsSync(bin)) {
      throw new Error(
        `workbench-host binary not found at ${bin}. Run: (cd host && cargo build)`,
      );
    }
    this.#bin = bin;
    this.#binMtime = fs.statSync(bin).mtimeMs;
    console.error(`[workbench-host] using ${bin}`);
    this.#child = spawn(bin, [], {
      stdio: ["pipe", "pipe", "pipe"],
      env: {
        ...process.env,
        WORKBENCH_SETTINGS_DIR: this.#settingsDir,
      },
    });

    this.#child.stderr.on("data", (chunk) => {
      const text = chunk.toString().trim();
      if (text) console.error(`[workbench-host] ${text}`);
    });

    const rl = createInterface({ input: this.#child.stdout });
    rl.on("line", (line) => this.#onLine(line));

    this.#child.on("exit", (code, signal) => {
      console.error(`workbench-host exited code=${code} signal=${signal}`);
      const err = new Error(`workbench-host exited (code=${code})`);
      for (const [, pending] of this.#pending) {
        pending.reject(err);
      }
      this.#pending.clear();
      this.#child = null;
    });
  }

  stop() {
    if (!this.#child) return;
    this.#child.kill();
    this.#child = null;
  }

  onEvent(handler) {
    this.#eventHandlers.add(handler);
    return () => this.#eventHandlers.delete(handler);
  }

  #restartIfBinaryChanged() {
    if (!this.#bin || !unpackagedDev()) return;
    let mtime = 0;
    try {
      mtime = fs.statSync(this.#bin).mtimeMs;
    } catch {
      return;
    }
    if (!(mtime > this.#binMtime)) return;
    console.error("[workbench-host] binary changed — restarting sidecar");
    this.stop();
    this.start();
  }

  invoke(cmd, args = {}) {
    this.#restartIfBinaryChanged();
    if (!this.#child || !this.#child.stdin.writable) {
      return Promise.reject(new Error("workbench-host is not running"));
    }
    const id = String(this.#nextId++);
    const payload = JSON.stringify({ id, cmd, args });
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#child.stdin.write(payload + "\n", (error) => {
        if (error) {
          this.#pending.delete(id);
          reject(error);
        }
      });
    });
  }

  #onLine(line) {
    const trimmed = line.trim();
    if (!trimmed) return;
    let message;
    try {
      message = JSON.parse(trimmed);
    } catch (error) {
      console.error("workbench-host: bad JSON line", trimmed, error);
      return;
    }

    if (message.event) {
      for (const handler of this.#eventHandlers) {
        try {
          handler(message.event, message.payload);
        } catch (error) {
          console.error("sidecar event handler error", error);
        }
      }
      return;
    }

    const id = message.id != null ? String(message.id) : null;
    if (!id || !this.#pending.has(id)) {
      return;
    }
    const pending = this.#pending.get(id);
    this.#pending.delete(id);
    if (message.ok) {
      pending.resolve(message.result);
    } else {
      pending.reject(new Error(message.error || "Host command failed"));
    }
  }
}
