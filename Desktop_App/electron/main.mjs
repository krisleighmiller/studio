import {
  app,
  BrowserWindow,
  Menu,
  dialog,
  ipcMain,
  protocol,
  shell,
} from "electron";
import fs from "node:fs";
import path from "node:path";
import { Readable } from "node:stream";
import { fileURLToPath } from "node:url";
import { Sidecar } from "./sidecar.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const isDev = !app.isPackaged;

// Stable userData path (matches package.json name) even if launched oddly.
if (!app.isPackaged) {
  app.setName("video-workbench");
}

/** @type {BrowserWindow | null} */
let mainWindow = null;
/** @type {Sidecar | null} */
let sidecar = null;

const MEDIA_SCHEME = "workbench-media";

protocol.registerSchemesAsPrivileged([
  {
    scheme: MEDIA_SCHEME,
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      stream: true,
      bypassCSP: true,
      corsEnabled: true,
    },
  },
]);

const RECENT_PROJECTS_LIMIT = 8;

function settingsDir() {
  return path.join(app.getPath("userData"), "config");
}

function recentProjectsPath() {
  return path.join(app.getPath("userData"), "recent-projects.json");
}

function loadRecentProjects() {
  try {
    const raw = fs.readFileSync(recentProjectsPath(), "utf8");
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((item) => typeof item === "string" && item.trim())
      .map((item) => item.replace(/\/+$/, ""))
      .slice(0, RECENT_PROJECTS_LIMIT);
  } catch {
    return [];
  }
}

function saveRecentProjects(paths) {
  const cleaned = [];
  const seen = new Set();
  for (const item of paths) {
    if (typeof item !== "string") continue;
    const normalized = item.replace(/\/+$/, "");
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    cleaned.push(normalized);
    if (cleaned.length >= RECENT_PROJECTS_LIMIT) break;
  }
  fs.mkdirSync(app.getPath("userData"), { recursive: true });
  fs.writeFileSync(recentProjectsPath(), JSON.stringify(cleaned, null, 2));
  return cleaned;
}

function rememberRecentProject(projectPath) {
  const normalized = String(projectPath || "").replace(/\/+$/, "");
  if (!normalized) return loadRecentProjects();
  const next = [
    normalized,
    ...loadRecentProjects().filter((item) => item !== normalized),
  ];
  return saveRecentProjects(next);
}

function recentMenuLabel(projectPath) {
  const name = path.basename(projectPath) || projectPath;
  // Keep menu readable; full path is still sent as the action payload.
  if (projectPath.length <= 72) return `${name} — ${projectPath}`;
  return `${name} — …${projectPath.slice(-56)}`;
}

function mediaUrlFor(filePath) {
  // Base64url path segment avoids Chromium mangling absolute POSIX paths.
  const encoded = Buffer.from(String(filePath), "utf8").toString("base64url");
  return `${MEDIA_SCHEME}://local/${encoded}`;
}

function pathFromMediaUrl(url) {
  const parsed = new URL(url);
  const encoded = parsed.pathname.replace(/^\/+/, "");
  if (!encoded) {
    throw new Error(`Unexpected media URL: ${url}`);
  }
  return Buffer.from(encoded, "base64url").toString("utf8");
}

function mimeForMediaPath(filePath) {
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

/**
 * Serve local media with real HTTP Range support so <video>/<audio> can seek
 * and restart after ended. Plain net.fetch(file://) often leaves seekable empty.
 */
function mediaFileResponse(filePath, request) {
  if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
    return new Response("Not found", { status: 404 });
  }
  const stats = fs.statSync(filePath);
  const size = stats.size;
  const mime = mimeForMediaPath(filePath);
  const rangeHeader = request.headers.get("Range") || request.headers.get("range");

  if (rangeHeader) {
    const match = /bytes=(\d*)-(\d*)/.exec(rangeHeader);
    if (match) {
      let start = match[1] === "" ? 0 : Number.parseInt(match[1], 10);
      let end = match[2] === "" ? size - 1 : Number.parseInt(match[2], 10);
      if (!Number.isFinite(start)) start = 0;
      if (!Number.isFinite(end) || end >= size) end = size - 1;
      if (start < 0 || start >= size || start > end) {
        return new Response(null, {
          status: 416,
          headers: {
            "Content-Range": `bytes */${size}`,
            "Accept-Ranges": "bytes",
          },
        });
      }
      const stream = fs.createReadStream(filePath, { start, end });
      stream.on("error", (error) => {
        console.error("media range stream error", error);
      });
      return new Response(Readable.toWeb(stream), {
        status: 206,
        headers: {
          "Content-Type": mime,
          "Content-Length": String(end - start + 1),
          "Content-Range": `bytes ${start}-${end}/${size}`,
          "Accept-Ranges": "bytes",
          "Cache-Control": "no-cache",
        },
      });
    }
  }

  const stream = fs.createReadStream(filePath);
  stream.on("error", (error) => {
    console.error("media stream error", error);
  });
  return new Response(Readable.toWeb(stream), {
    status: 200,
    headers: {
      "Content-Type": mime,
      "Content-Length": String(size),
      "Accept-Ranges": "bytes",
      "Cache-Control": "no-cache",
    },
  });
}

function createMenu(recentProjects = loadRecentProjects()) {
  const send = (id) => {
    mainWindow?.webContents.send("menu", id);
  };

  const recentSubmenu =
    recentProjects.length > 0
      ? [
          ...recentProjects.map((projectPath) => ({
            label: recentMenuLabel(projectPath),
            click: () =>
              send(
                `open-recent:${Buffer.from(projectPath, "utf8").toString("base64url")}`,
              ),
          })),
          { type: "separator" },
          {
            label: "Clear Recent",
            click: () => {
              saveRecentProjects([]);
              createMenu([]);
            },
          },
        ]
      : [{ label: "No Recent Projects", enabled: false }];

  const template = [
    {
      label: "File",
      submenu: [
        {
          label: "Open Project…",
          accelerator: "CmdOrCtrl+O",
          click: () => send("open-project"),
        },
        {
          label: "Open Recent",
          submenu: recentSubmenu,
        },
        {
          label: "Close Project",
          click: () => send("close-project"),
        },
        { type: "separator" },
        {
          label: "Save",
          accelerator: "CmdOrCtrl+S",
          click: () => send("save"),
        },
        {
          label: "Save As…",
          accelerator: "CmdOrCtrl+Shift+S",
          click: () => send("save-as"),
        },
        { type: "separator" },
        { role: "quit" },
      ],
    },
    {
      label: "Edit",
      // Use Chromium roles for clipboard/undo so shortcuts hit the focused
      // control (chat compose, find bar, editor). Custom click handlers were
      // stealing Ctrl+V/C/X from the Director chat textarea.
      submenu: [
        { role: "undo" },
        { role: "redo" },
        { type: "separator" },
        { role: "cut" },
        { role: "copy" },
        { role: "paste" },
        { type: "separator" },
        { role: "selectAll" },
        { type: "separator" },
        {
          label: "Find…",
          accelerator: "CmdOrCtrl+F",
          click: () => send("find"),
        },
        {
          label: "Replace…",
          accelerator: "CmdOrCtrl+H",
          click: () => send("replace"),
        },
        { type: "separator" },
        {
          label: "Settings…",
          accelerator: "CmdOrCtrl+,",
          click: () => send("open-settings"),
        },
      ],
    },
    {
      label: "View",
      submenu: [
        {
          label: "Refresh Explorer",
          accelerator: "CmdOrCtrl+R",
          click: () => send("refresh-explorer"),
        },
        { type: "separator" },
        {
          label: "Toggle Markdown Preview",
          accelerator: "CmdOrCtrl+Shift+P",
          click: () => send("toggle-markdown-preview"),
        },
        { type: "separator" },
        {
          label: "Character Voice…",
          accelerator: "CmdOrCtrl+Alt+V",
          click: () => send("open-voice"),
        },
        {
          label: "Production Desk",
          accelerator: "CmdOrCtrl+Alt+P",
          click: () => send("open-workflow"),
        },
        {
          label: "Toggle Chat",
          accelerator: "CmdOrCtrl+Alt+H",
          click: () => send("toggle-chat"),
        },
        { type: "separator" },
        { role: "toggleDevTools" },
        // Do not use bare role:reload — its default Ctrl+R steals Refresh Explorer.
        {
          label: "Reload Window",
          accelerator: "CmdOrCtrl+Shift+R",
          role: "reload",
        },
      ],
    },
    {
      label: "Help",
      submenu: [
        {
          label: "Copy Chat Transcript",
          accelerator: "CmdOrCtrl+Shift+C",
          click: () => send("copy-chat"),
        },
        {
          label: "Save Chat Transcript…",
          accelerator: "CmdOrCtrl+Shift+E",
          click: () => send("save-chat"),
        },
        { type: "separator" },
        {
          label: "Documentation",
          accelerator: "F1",
          click: () => send("open-docs"),
        },
        {
          label: "About Video Workbench",
          click: async () => {
            await dialog.showMessageBox({
              type: "info",
              title: "About Video Workbench",
              message: "Video Workbench",
              detail:
                "Stoppable desktop production desk for video projects.\nElectron + Rust host sidecar.",
            });
          },
        },
      ],
    },
  ];

  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

/**
 * Wayland/Linux menu accelerators are flaky; route app shortcuts here so they
 * always reach the renderer (and Ctrl+R cannot be stolen by Reload).
 */
function wireAppShortcuts(win) {
  win.webContents.on("before-input-event", (event, input) => {
    if (input.type !== "keyDown") return;

    if (input.key === "F1") {
      event.preventDefault();
      win.webContents.send("menu", "open-docs");
      return;
    }

    const ctrl = input.control || input.meta;
    if (!ctrl) return;

    const key = input.key.length === 1 ? input.key.toLowerCase() : input.key;
    /** @type {string | null} */
    let action = null;

    if (key === "o" && !input.shift && !input.alt) action = "open-project";
    else if (key === "s" && input.shift) action = "save-as";
    else if (key === "s" && !input.alt) action = "save";
    else if (key === "f" && !input.shift && !input.alt) action = "find";
    else if (key === "h" && !input.shift && !input.alt) action = "replace";
    else if (key === "p" && input.shift && !input.alt) {
      action = "toggle-markdown-preview";
    } else if (key === "," && !input.shift && !input.alt) action = "open-settings";
    else if (key === "r" && !input.shift && !input.alt) action = "refresh-explorer";
    else if (key === "v" && input.alt) action = "open-voice";
    else if (key === "p" && input.alt) action = "open-workflow";
    else if (key === "h" && input.alt) action = "toggle-chat";
    else if (key === "c" && input.shift && !input.alt) action = "copy-chat";
    else if (key === "e" && input.shift && !input.alt) action = "save-chat";

    if (!action) return;
    event.preventDefault();
    win.webContents.send("menu", action);
  });
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 960,
    minHeight: 640,
    title: "Video Workbench",
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  wireAppShortcuts(mainWindow);

  if (isDev) {
    const url = process.env.VITE_DEV_SERVER_URL || "http://localhost:1420";
    void mainWindow.loadURL(url);
  } else {
    void mainWindow.loadFile(path.join(__dirname, "..", "dist", "index.html"));
  }

  mainWindow.on("closed", () => {
    mainWindow = null;
  });
}

function migrateSettings() {
  const dest = path.join(settingsDir(), "settings.json");
  if (fs.existsSync(dest)) return;

  // One-time copy from the old Tauri app config dirs (if present).
  const home = app.getPath("home");
  const candidates = [
    path.join(home, ".config", "com.kris.videoworkbench", "settings.json"),
    path.join(home, ".config", "com.kris.video-workbench", "settings.json"),
    path.join(home, ".config", "video-workbench", "settings.json"),
  ];

  for (const candidate of candidates) {
    if (!fs.existsSync(candidate)) continue;
    try {
      fs.mkdirSync(settingsDir(), { recursive: true });
      fs.copyFileSync(candidate, dest);
      console.log(`Migrated settings from ${candidate}`);
      return;
    } catch (error) {
      console.error("Settings migrate failed", error);
    }
  }
}

function registerIpc() {
  ipcMain.handle("host:invoke", async (_event, cmd, args) => {
    if (!sidecar) throw new Error("Host sidecar is not started");
    return sidecar.invoke(cmd, args ?? {});
  });

  ipcMain.on("host:toMediaUrlSync", (event, filePath) => {
    event.returnValue = mediaUrlFor(String(filePath));
  });

  ipcMain.handle("dialog:open", async (_event, opts = {}) => {
    const properties = [];
    if (opts.directory) properties.push("openDirectory");
    else properties.push("openFile");
    if (opts.multiple) properties.push("multiSelections");

    const result = await dialog.showOpenDialog({
      title: opts.title,
      defaultPath: opts.defaultPath,
      properties,
      filters: opts.filters?.map((f) => ({
        name: f.name,
        extensions: f.extensions,
      })),
    });
    if (result.canceled || result.filePaths.length === 0) return null;
    if (opts.multiple) return result.filePaths;
    return result.filePaths[0] ?? null;
  });

  ipcMain.handle("dialog:save", async (_event, opts = {}) => {
    const result = await dialog.showSaveDialog({
      title: opts.title,
      defaultPath: opts.defaultPath,
      filters: opts.filters?.map((f) => ({
        name: f.name,
        extensions: f.extensions,
      })),
    });
    if (result.canceled || !result.filePath) return null;
    return result.filePath;
  });

  ipcMain.handle("dialog:ask", async (_event, message, opts = {}) => {
    const result = await dialog.showMessageBox({
      type:
        opts.kind === "warning"
          ? "warning"
          : opts.kind === "error"
            ? "error"
            : "question",
      title: opts.title || "Confirm",
      message: String(message),
      buttons: ["Cancel", "OK"],
      defaultId: 1,
      cancelId: 0,
    });
    return result.response === 1;
  });

  ipcMain.handle("shell:openPath", async (_event, target) => {
    return shell.openPath(String(target));
  });

  ipcMain.handle("recent:list", async () => loadRecentProjects());

  ipcMain.handle("recent:remember", async (_event, projectPath) => {
    const next = rememberRecentProject(projectPath);
    createMenu(next);
    return next;
  });

  ipcMain.handle("recent:remove", async (_event, projectPath) => {
    const normalized = String(projectPath || "").replace(/\/+$/, "");
    const next = saveRecentProjects(
      loadRecentProjects().filter((item) => item !== normalized),
    );
    createMenu(next);
    return next;
  });
}

app.whenReady().then(() => {
  protocol.handle(MEDIA_SCHEME, (request) => {
    try {
      const filePath = pathFromMediaUrl(request.url);
      return mediaFileResponse(filePath, request);
    } catch (error) {
      console.error("media protocol error", error);
      return new Response("Not found", { status: 404 });
    }
  });

  migrateSettings();

  sidecar = new Sidecar({ settingsDir: settingsDir() });
  sidecar.start();
  sidecar.onEvent((event, payload) => {
    if (event === "director-progress") {
      mainWindow?.webContents.send("director-progress", payload);
    }
  });

  registerIpc();
  createMenu();
  createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("before-quit", () => {
  sidecar?.stop();
});
