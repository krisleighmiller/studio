import {
  ask,
  bindHostEvents,
  decodeOpenRecentMenuAction,
  closeProjectSession,
  invoke,
  openDialog,
  openProjectFolder,
  rememberRecentProject,
  useRecentProject,
  removeRecentProject,
  saveDialog,
  toMediaUrlAsync,
} from "./host";
import { isMarkdownPath, renderMarkdown } from "./markdown";
import workbenchGuideMd from "./docs/workbench-guide.md?raw";
import {
  formatTime,
  isMediaKind,
  normalizePlaybackRate,
  nextPlaybackRate,
  PLAYBACK_RATES,
  type MediaKind,
  type OpenKind,
  type PlaybackRate,
} from "./media";
import {
  bindSettingsHost,
  getSettings,
  loadAppSettings,
  populateChatModelSelect,
  persistSettings,
  renderSettingsPanel,
  setSelectedChatModelFromSelect,
  settingsAreDirty,
} from "./settingsPanel";
import { bindVoiceHost, openVoicePanelContext, renderVoicePanel } from "./voicePanel";
import {
  bindWorkflowDesk,
  clearWorkflowDesk,
  loadWorkflowDesk,
  openJobsDrawer,
  setStudioWorkArea,
  refreshWorkflowDesk,
  renderWorkflowDesk,
  attachDirectorContext,
  detachDirectorContext,
  directorContextEntries,
  directorScopeHeadline,
  directorTurnSelection,
  omittedDirectorEntries,
  workflowSelection,
  workflowStatusLine,
} from "./workflowDesk";

type DirEntry = {
  name: string;
  path: string;
  isDir: boolean;
};

type TabKind = "text" | "settings" | "docs" | "voice" | "workflow" | MediaKind;

type EditorTab = {
  id: string;
  kind: TabKind;
  path: string | null;
  name: string;
  contents: string;
  original: string;
  dirty: boolean;
  undoStack: string[];
  redoStack: string[];
  preview: boolean;
  selectionStart: number;
  selectionEnd: number;
  playbackRate: PlaybackRate;
};

type ChatMessage = {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
};

type ChatTab = {
  id: string;
  title: string;
  messages: ChatMessage[];
  busy: boolean;
  /** Explicit file/folder paths attached to this chat as context. */
  attachments: string[];
};

const WORKBENCH_PATH_MIME = "application/x-workbench-path";

type ContextMenuItem =
  | { type: "item"; label: string; action: () => void; disabled?: boolean }
  | { type: "separator" };

const UNSUPPORTED_HINT =
  "That file type is not supported in the editor yet.";
const MAX_UNDO = 200;
const OPEN_FILTERS = [
  {
    name: "Text",
    extensions: [
      "md",
      "markdown",
      "txt",
      "yaml",
      "yml",
      "json",
      "toml",
      "csv",
      "ts",
      "js",
      "css",
      "html",
      "xml",
      "prompt",
    ],
  },
  {
    name: "Media",
    extensions: [
      "png",
      "jpg",
      "jpeg",
      "gif",
      "webp",
      "bmp",
      "mp4",
      "webm",
      "mov",
      "mkv",
      "wav",
      "mp3",
      "ogg",
      "flac",
      "m4a",
    ],
  },
  { name: "All Files", extensions: ["*"] },
];

const els = {
  projectName: document.querySelector<HTMLElement>("#project-name")!,
  shellProjectName: document.querySelector<HTMLElement>("#shell-project-name")!,
  shellWorkArea: document.querySelector<HTMLSelectElement>("#shell-work-area")!,
  homeTitle: document.querySelector<HTMLElement>("#home-title")!,
  homeDetail: document.querySelector<HTMLElement>("#home-detail")!,
  assetsList: document.querySelector<HTMLElement>("#assets-list")!,
  navRail: document.querySelector<HTMLElement>("#nav-rail")!,
  jobsBtn: document.querySelector<HTMLButtonElement>("#jobs-btn")!,
  settingsBtn: document.querySelector<HTMLButtonElement>("#settings-btn")!,
  fileTree: document.querySelector<HTMLElement>("#file-tree")!,
  newFileBtn: document.querySelector<HTMLButtonElement>("#new-file-btn")!,
  newFolderBtn: document.querySelector<HTMLButtonElement>("#new-folder-btn")!,
  editorTabs: document.querySelector<HTMLElement>("#editor-tabs")!,
  editorEmpty: document.querySelector<HTMLElement>("#editor-empty")!,
  editor: document.querySelector<HTMLTextAreaElement>("#editor")!,
  preview: document.querySelector<HTMLElement>("#markdown-preview")!,
  settingsView: document.querySelector<HTMLElement>("#settings-view")!,
  workflowView: document.querySelector<HTMLElement>("#workflow-view")!,
  voiceView: document.querySelector<HTMLElement>("#voice-view")!,
  docsView: document.querySelector<HTMLElement>("#docs-view")!,
  mediaView: document.querySelector<HTMLElement>("#media-view")!,
  chatModelSelect: document.querySelector<HTMLSelectElement>("#chat-model-select")!,
  imageStage: document.querySelector<HTMLElement>("#image-stage")!,
  avStage: document.querySelector<HTMLElement>("#av-stage")!,
  mediaImage: document.querySelector<HTMLImageElement>("#media-image")!,
  mediaVideo: document.querySelector<HTMLVideoElement>("#media-video")!,
  mediaAudio: document.querySelector<HTMLAudioElement>("#media-audio")!,
  videoTitle: document.querySelector<HTMLElement>("#video-title")!,
  audioStage: document.querySelector<HTMLElement>("#audio-stage")!,
  audioTitle: document.querySelector<HTMLElement>("#audio-title")!,
  mediaPlayPause: document.querySelector<HTMLButtonElement>("#media-play-pause")!,
  mediaSeek: document.querySelector<HTMLInputElement>("#media-seek")!,
  mediaVolume: document.querySelector<HTMLInputElement>("#media-volume")!,
  mediaTime: document.querySelector<HTMLElement>("#media-time")!,
  editorToolbar: document.querySelector<HTMLElement>("#editor-toolbar")!,
  saveBtn: document.querySelector<HTMLButtonElement>("#save-btn")!,
  saveAsBtn: document.querySelector<HTMLButtonElement>("#save-as-btn")!,
  renameFileBtn: document.querySelector<HTMLButtonElement>("#rename-file-btn")!,
  deleteFileBtn: document.querySelector<HTMLButtonElement>("#delete-file-btn")!,
  togglePreviewBtn: document.querySelector<HTMLButtonElement>("#toggle-preview-btn")!,
  findBar: document.querySelector<HTMLElement>("#find-bar")!,
  findInput: document.querySelector<HTMLInputElement>("#find-input")!,
  replaceInput: document.querySelector<HTMLInputElement>("#replace-input")!,
  replaceField: document.querySelector<HTMLElement>("#replace-field")!,
  replaceActions: document.querySelector<HTMLElement>("#replace-actions")!,
  findCount: document.querySelector<HTMLElement>("#find-count")!,
  findPrev: document.querySelector<HTMLButtonElement>("#find-prev")!,
  findNext: document.querySelector<HTMLButtonElement>("#find-next")!,
  replaceOne: document.querySelector<HTMLButtonElement>("#replace-one")!,
  replaceAll: document.querySelector<HTMLButtonElement>("#replace-all")!,
  findClose: document.querySelector<HTMLButtonElement>("#find-close")!,
  chatTabs: document.querySelector<HTMLElement>("#chat-tabs")!,
  chatMessages: document.querySelector<HTMLElement>("#chat-messages")!,
  chatForm: document.querySelector<HTMLFormElement>("#chat-form")!,
  chatAttachments: document.querySelector<HTMLElement>("#chat-attachments")!,
  directorContext: document.querySelector<HTMLElement>("#director-context")!,
  directorScope: document.querySelector<HTMLElement>("#director-scope")!,
  chatInput: document.querySelector<HTMLTextAreaElement>("#chat-input")!,
  chatSendBtn: document.querySelector<HTMLButtonElement>("#chat-send-btn")!,
  chatStopBtn: document.querySelector<HTMLButtonElement>("#chat-stop-btn")!,
  newChatTab: document.querySelector<HTMLButtonElement>("#new-chat-tab")!,
  copyChatBtn: document.querySelector<HTMLButtonElement>("#copy-chat-btn")!,
  saveChatBtn: document.querySelector<HTMLButtonElement>("#save-chat-btn")!,
  statusLeft: document.querySelector<HTMLElement>("#status-left")!,
  statusRight: document.querySelector<HTMLElement>("#status-right")!,
  explorerPanel: document.querySelector<HTMLElement>("#explorer-panel")!,
  chatPanel: document.querySelector<HTMLElement>("#chat-panel")!,
  chatSplitter: document.querySelector<HTMLElement>('[data-splitter="chat"]')!,
  toggleChatBtn: document.querySelector<HTMLButtonElement>("#toggle-chat-btn")!,
  chatRail: document.querySelector<HTMLButtonElement>("#chat-rail")!,
  contextMenu: document.querySelector<HTMLElement>("#context-menu")!,
};

const SHELL_SURFACES = ["home", "studio", "files", "assets", "voices", "settings"] as const;
type ShellSurface = (typeof SHELL_SURFACES)[number];

let shellSurface: ShellSurface = "home";
let shellReturn: ShellSurface = "home";
let filesEditorId: string | null = null;
let assetRenderSeq = 0;

let projectRoot: string | null = null;
let selectedPath: string | null = null;
let editorTabs: EditorTab[] = [];
let activeEditorId: string | null = null;
let chatTabs: ChatTab[] = [];
let activeChatId: string | null = null;
let chatCounter = 1;
let chatSendInFlight = false;
let untitledCounter = 1;
let applyingHistory = false;
let lastEditorSnapshot = "";
let findMatchIndex = -1;
let pendingCreate: { parentDir: string; kind: "file" | "folder" } | null = null;
let chatCollapsed = false;
let workflowLoadedRoot: string | null = null;
let pendingRename: { path: string; isDir: boolean } | null = null;
let createCommitInFlight = false;
let renameCommitInFlight = false;
let mediaLoadToken = 0;
let mediaSeekDragging = false;
let mediaVolume = 1;
const expandedDirs = new Set<string>();
/** Drops in-flight full tree rebuilds that would clobber a newer render. */
let explorerRenderSeq = 0;
/** Explorer file clipboard (Cut/Copy → Paste into a folder). */
let explorerClipboard: { paths: string[]; mode: "cut" | "copy" } | null = null;
function uid(prefix: string): string {
  return `${prefix}-${crypto.randomUUID()}`;
}

function setStatus(left: string, right = ""): void {
  els.statusLeft.textContent = left;
  els.statusRight.textContent = right;
}

function basename(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] || path;
}

function dirname(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const index = normalized.lastIndexOf("/");
  if (index <= 0) return normalized;
  return normalized.slice(0, index);
}

function joinPath(dir: string, name: string): string {
  const left = dir.replace(/\\/g, "/").replace(/\/+$/, "");
  return `${left}/${name}`;
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
}

function relativeToProject(path: string): string | null {
  if (!projectRoot) return null;
  const root = normalizePath(projectRoot).replace(/\/+$/, "");
  const absolute = normalizePath(path);
  if (absolute === root) return ".";
  if (absolute.startsWith(`${root}/`)) {
    return absolute.slice(root.length + 1);
  }
  return null;
}

function pathForCopy(): string | null {
  if (selectedPath) return selectedPath;
  const tab = activeEditor();
  return tab?.path ?? null;
}

function isValidEntryName(name: string): boolean {
  if (!name.trim()) return false;
  if (name === "." || name === "..") return false;
  return !/[\\/]/.test(name);
}

function createChatTab(title?: string): ChatTab {
  const tab: ChatTab = {
    id: uid("chat"),
    title: title ?? `Chat ${chatCounter++}`,
    busy: false,
    attachments: [],
    messages: [],
  };
  chatTabs.push(tab);
  activeChatId = tab.id;
  return tab;
}

function addPathToChat(path: string): void {
  const tab = activeChat();
  if (!tab) {
    createChatTab("Director");
  }
  const target = activeChat();
  if (!target) return;
  const normalized = normalizePath(path);
  if (!normalized) return;
  if (target.attachments.some((item) => normalizePath(item) === normalized)) {
    setStatus("Already attached to chat", basename(normalized));
    renderChatAttachments();
    return;
  }
  target.attachments.push(normalized);
  renderChatAttachments();
  setStatus("Added to chat", basename(normalized));
  setChatCollapsed(false);
}

function removeChatAttachment(path: string): void {
  const tab = activeChat();
  if (!tab) return;
  const normalized = normalizePath(path);
  tab.attachments = tab.attachments.filter(
    (item) => normalizePath(item) !== normalized,
  );
  renderChatAttachments();
}

function renderDirectorContext(): void {
  els.directorScope.textContent = directorScopeHeadline();
  const entries = directorContextEntries().filter((entry) => entry.kind !== "workArea");
  const omitted = omittedDirectorEntries().filter((entry) => entry.kind !== "workArea");
  els.directorContext.replaceChildren();
  if (!entries.length && !omitted.length) {
    els.directorContext.classList.add("hidden");
    return;
  }
  els.directorContext.classList.remove("hidden");
  const heading = document.createElement("p");
  heading.className = "director-context-heading";
  heading.textContent = "Attached context";
  const list = document.createElement("ul");
  for (const entry of entries) {
    const item = document.createElement("li");
    const name = document.createElement("span");
    name.textContent = entry.name;
    name.title = entry.label;
    const remove = document.createElement("button");
    remove.type = "button";
    remove.title = "Remove from the next Director turn";
    remove.setAttribute("aria-label", `Remove ${entry.name}`);
    remove.textContent = "×";
    remove.addEventListener("click", () => detachDirectorContext(entry.kind, entry.id));
    item.append(name, remove);
    list.append(item);
  }
  els.directorContext.append(heading, list);
  if (!omitted.length) return;
  const restore = document.createElement("div");
  restore.className = "director-context-restore";
  for (const entry of omitted) {
    const add = document.createElement("button");
    add.type = "button";
    add.className = "ghost-button";
    add.textContent = `Attach ${entry.name}`;
    add.title = entry.label;
    add.addEventListener("click", () => attachDirectorContext(entry.kind, entry.id));
    restore.append(add);
  }
  els.directorContext.append(restore);
}

function renderChatAttachments(): void {
  const tab = activeChat();
  els.chatAttachments.innerHTML = "";
  if (!tab || tab.attachments.length === 0) {
    els.chatAttachments.classList.add("hidden");
    return;
  }
  els.chatAttachments.classList.remove("hidden");
  for (const path of tab.attachments) {
    const chip = document.createElement("div");
    chip.className = "chat-attachment-chip";
    chip.title = path;
    const label = document.createElement("span");
    label.textContent = basename(path);
    const remove = document.createElement("button");
    remove.type = "button";
    remove.title = "Remove from chat";
    remove.setAttribute("aria-label", `Remove ${basename(path)}`);
    remove.textContent = "×";
    remove.addEventListener("click", () => removeChatAttachment(path));
    chip.append(label, remove);
    els.chatAttachments.append(chip);
  }
}

function activeEditor(): EditorTab | undefined {
  return editorTabs.find((tab) => tab.id === activeEditorId);
}

function activeChat(): ChatTab | undefined {
  return chatTabs.find((tab) => tab.id === activeChatId);
}

function tabIsMarkdown(tab: EditorTab): boolean {
  return tab.kind === "text" && isMarkdownPath(tab.path ?? tab.name);
}

function isTextTab(
  tab: EditorTab | undefined,
): tab is EditorTab & { kind: "text" } {
  return Boolean(tab && tab.kind === "text");
}

function isSettingsTab(
  tab: EditorTab | undefined,
): tab is EditorTab & { kind: "settings" } {
  return Boolean(tab && tab.kind === "settings");
}

function isDocsTab(
  tab: EditorTab | undefined,
): tab is EditorTab & { kind: "docs" } {
  return Boolean(tab && tab.kind === "docs");
}

function isWorkflowTab(
  tab: EditorTab | undefined,
): tab is EditorTab & { kind: "workflow" } {
  return Boolean(tab && tab.kind === "workflow");
}

function isAppLevelTab(tab: EditorTab): boolean {
  return tab.kind === "settings" || tab.kind === "docs" || tab.kind === "voice";
}

function hideSpecialViews(): void {
  els.settingsView.classList.add("hidden");
  els.workflowView.classList.add("hidden");
  els.voiceView.classList.add("hidden");
  els.docsView.classList.add("hidden");
}

function isFileTab(tab: EditorTab): boolean {
  return tab.kind === "text" || tab.kind === "docs" || isMediaKind(tab.kind);
}

function rememberFileTab(tab: EditorTab | undefined): void {
  if (tab && isFileTab(tab)) filesEditorId = tab.id;
}

function paintSurface(): void {
  const app = document.querySelector("#app");
  app?.classList.remove(...SHELL_SURFACES.map((name) => `surface-${name}`));
  app?.classList.add(`surface-${shellSurface}`);
  for (const button of els.navRail.querySelectorAll<HTMLButtonElement>(".nav-item")) {
    const on = button.dataset.surface === shellSurface;
    button.classList.toggle("active", on);
    if (on) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  }
  els.settingsBtn.setAttribute("aria-pressed", shellSurface === "settings" ? "true" : "false");
  const labels: Record<ShellSurface, string> = {
    home: "Home",
    studio: "Studio",
    files: "Files",
    assets: "Assets",
    voices: "Voices",
    settings: "Settings",
  };
  document.querySelector("#editor-panel")?.setAttribute("aria-label", labels[shellSurface]);
  updateShellChrome();
}

function updateShellChrome(): void {
  const name = projectRoot ? basename(projectRoot) : "No project";
  els.shellProjectName.textContent = name;
  els.shellProjectName.title = projectRoot ?? "";
  const area = projectRoot ? workflowSelection().workArea : null;
  els.shellWorkArea.disabled = !area;
  if (area) els.shellWorkArea.value = area;
}

function renderHome(): void {
  if (!projectRoot) {
    els.homeTitle.textContent = "Production Studio";
    els.homeDetail.textContent = "Open a project folder to work on scenes, cuts, and review exports.";
    return;
  }
  els.homeTitle.textContent = basename(projectRoot);
  const status = workflowStatusLine();
  els.homeDetail.textContent = status ? `${projectRoot}\n${status}` : projectRoot;
}

function showFileBackedSurface(): void {
  const preferred = editorTabs.find((tab) => tab.id === filesEditorId && isFileTab(tab));
  const fallback = [...editorTabs].reverse().find((tab) => isFileTab(tab));
  activeEditorId = (preferred ?? fallback)?.id ?? null;
  renderEditor();
}

function revealSurfaceBody(): void {
  if (shellSurface === "studio") {
    openWorkflowTab();
    return;
  }
  if (shellSurface === "voices") {
    openVoiceTab();
    return;
  }
  if (shellSurface === "settings") {
    openSettingsTab();
    return;
  }
  if (shellSurface === "files" || shellSurface === "assets") {
    showFileBackedSurface();
    if (shellSurface === "assets") void renderAssetBrowser();
    return;
  }
  renderHome();
}

function showShell(next: ShellSurface): void {
  if (next === "settings" && shellSurface === "settings") {
    shellSurface = shellReturn === "settings" ? "home" : shellReturn;
    paintSurface();
    revealSurfaceBody();
    return;
  }
  if (next === shellSurface) {
    paintSurface();
    if (next === "assets") void renderAssetBrowser();
    if (next === "home") renderHome();
    if (next === "studio") renderWorkflowDesk();
    return;
  }
  if (shellSurface !== "settings" && next === "settings") shellReturn = shellSurface;
  shellSurface = next;
  paintSurface();
  revealSurfaceBody();
}

function placeFileSurface(tab: EditorTab, forceFiles = false): void {
  rememberFileTab(tab);
  if (!forceFiles && (shellSurface === "files" || shellSurface === "assets")) return;
  shellSurface = "files";
  paintSurface();
}

const MEDIA_EXTENSIONS = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "bmp",
  "mp4",
  "m4v",
  "webm",
  "mov",
  "mkv",
  "wav",
  "mp3",
  "ogg",
  "opus",
  "flac",
  "m4a",
  "aac",
]);

async function collectProjectMedia(
  dir: string,
  out: DirEntry[],
  depth: number,
): Promise<void> {
  if (depth > 8 || out.length >= 500) return;
  let entries: DirEntry[];
  try {
    entries = await loadChildren(dir);
  } catch {
    return;
  }
  for (const entry of entries) {
    if (entry.name.startsWith(".") || entry.name === "node_modules") continue;
    if (entry.isDir) {
      await collectProjectMedia(entry.path, out, depth + 1);
      continue;
    }
    const ext = entry.name.split(".").pop()?.toLowerCase() ?? "";
    if (MEDIA_EXTENSIONS.has(ext)) out.push(entry);
  }
}

async function renderAssetBrowser(): Promise<void> {
  const seq = ++assetRenderSeq;
  if (!projectRoot) {
    els.assetsList.innerHTML =
      '<p class="empty-state">Open a project to browse its media files.</p>';
    return;
  }
  const found: DirEntry[] = [];
  await collectProjectMedia(projectRoot, found, 0);
  if (seq !== assetRenderSeq) return;
  if (!found.length) {
    els.assetsList.innerHTML = '<p class="empty-state">No media files in this project.</p>';
    return;
  }
  const openPath = editorTabs.find((tab) => tab.id === activeEditorId)?.path ?? null;
  els.assetsList.replaceChildren();
  for (const entry of found) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `asset-row${entry.path === openPath ? " active" : ""}`;
    const relative = projectRoot && entry.path.startsWith(projectRoot)
      ? entry.path.slice(projectRoot.length).replace(/^[/\\]/, "")
      : entry.path;
    const name = document.createElement("span");
    name.textContent = entry.name;
    const where = document.createElement("small");
    where.textContent = relative;
    button.append(name, where);
    button.addEventListener("click", () => {
      selectedPath = entry.path;
      void openFile(entry.path);
    });
    els.assetsList.append(button);
  }
}

function setChatCollapsed(collapsed: boolean): void {
  chatCollapsed = collapsed;
  els.chatPanel.classList.toggle("collapsed", collapsed);
  els.chatSplitter.classList.toggle("chat-collapsed", collapsed);
  els.chatRail.classList.toggle("hidden", !collapsed);
  const label = collapsed ? "Show Director" : "Hide Director";
  els.toggleChatBtn.title = `${label} (Ctrl+Alt+H)`;
  els.toggleChatBtn.setAttribute("aria-label", label);
  els.toggleChatBtn.setAttribute("aria-pressed", collapsed ? "true" : "false");
}

function toggleChatPanel(): void {
  setChatCollapsed(!chatCollapsed);
}

async function confirmAction(
  message: string,
  title = "Confirm",
): Promise<boolean> {
  return ask(message, { title, kind: "warning" });
}

function isMediaTab(
  tab: EditorTab | undefined,
): tab is EditorTab & { kind: MediaKind } {
  return Boolean(tab && isMediaKind(tab.kind));
}

function refreshChatModelSelect(): void {
  populateChatModelSelect(els.chatModelSelect);
}

function openSettingsTab(): void {
  syncTabFromEditor();
  const existing = editorTabs.find((tab) => tab.kind === "settings");
  if (existing) {
    activeEditorId = existing.id;
    renderEditor();
    return;
  }
  pauseAllMedia();
  const tab: EditorTab = {
    id: uid("editor"),
    kind: "settings",
    path: null,
    name: "Settings",
    contents: "",
    original: "",
    dirty: false,
    undoStack: [],
    redoStack: [],
    preview: false,
    selectionStart: 0,
    selectionEnd: 0,
    playbackRate: 1,
  };
  editorTabs.push(tab);
  activeEditorId = tab.id;
  renderEditor();
  setStatus("Settings");
}

function openDocsTab(): void {
  syncTabFromEditor();
  const existing = editorTabs.find((tab) => tab.kind === "docs");
  if (existing) {
    activeEditorId = existing.id;
    placeFileSurface(existing, true);
    renderEditor();
    return;
  }
  pauseAllMedia();
  const tab: EditorTab = {
    id: uid("editor"),
    kind: "docs",
    path: null,
    name: "Documentation",
    contents: "",
    original: "",
    dirty: false,
    undoStack: [],
    redoStack: [],
    preview: false,
    selectionStart: 0,
    selectionEnd: 0,
    playbackRate: 1,
  };
  editorTabs.push(tab);
  activeEditorId = tab.id;
  placeFileSurface(tab, true);
  renderEditor();
  setStatus("Documentation");
}

function openVoiceTab(): void {
  syncTabFromEditor();
  const existing = editorTabs.find((tab) => tab.kind === "voice");
  if (existing) {
    activeEditorId = existing.id;
    renderEditor();
    openVoicePanelContext(projectRoot, selectedPath);
    return;
  }
  pauseAllMedia();
  const tab: EditorTab = {
    id: uid("editor"),
    kind: "voice",
    path: null,
    name: "Character Voice",
    contents: "",
    original: "",
    dirty: false,
    undoStack: [],
    redoStack: [],
    preview: false,
    selectionStart: 0,
    selectionEnd: 0,
    playbackRate: 1,
  };
  editorTabs.push(tab);
  activeEditorId = tab.id;
  renderEditor();
  openVoicePanelContext(projectRoot, selectedPath);
  setStatus("Character Voice");
}

function workflowTabRecord(): EditorTab {
  return {
    id: uid("editor"),
    kind: "workflow",
    path: null,
    name: "Production",
    contents: "",
    original: "",
    dirty: false,
    undoStack: [],
    redoStack: [],
    preview: false,
    selectionStart: 0,
    selectionEnd: 0,
    playbackRate: 1,
  };
}

function openWorkflowTab(): void {
  syncTabFromEditor();
  const existing = editorTabs.find((tab) => tab.kind === "workflow");
  if (existing) {
    activeEditorId = existing.id;
    renderEditor();
    return;
  }
  pauseAllMedia();
  const tab = workflowTabRecord();
  editorTabs.unshift(tab);
  activeEditorId = tab.id;
  renderEditor();
  setStatus("Studio", workflowStatusLine() || "View → Studio");
}

async function loadProjectWorkflow(root: string): Promise<void> {
  if (workflowLoadedRoot !== root) {
    workflowLoadedRoot = root;
    await loadWorkflowDesk(root);
  }
  const selection = workflowSelection();
  if (selection.path) selectedPath = selection.path;
  showShell("studio");
  setStatus("Studio", workflowStatusLine());
}

function isAvTab(tab: EditorTab | undefined): tab is EditorTab {
  return Boolean(tab && (tab.kind === "video" || tab.kind === "audio"));
}

function activeMediaElement(): HTMLMediaElement | null {
  const tab = activeEditor();
  if (!tab) return null;
  if (tab.kind === "video") return els.mediaVideo;
  if (tab.kind === "audio") return els.mediaAudio;
  return null;
}

function pauseAllMedia(): void {
  els.mediaVideo.pause();
  els.mediaAudio.pause();
}

function updateMediaTimeDisplay(): void {
  const media = activeMediaElement();
  if (!media) {
    els.mediaTime.textContent = "0:00 / 0:00";
    els.mediaSeek.value = "0";
    els.mediaSeek.max = "0";
    return;
  }

  const duration = Number.isFinite(media.duration) ? media.duration : 0;
  els.mediaTime.textContent = `${formatTime(media.currentTime)} / ${formatTime(duration)}`;
  els.mediaSeek.max = String(duration || 0);
  if (!mediaSeekDragging) {
    els.mediaSeek.value = String(media.currentTime || 0);
  }
}

function applyMediaVolume(volume: number): void {
  mediaVolume = Math.min(1, Math.max(0, volume));
  els.mediaVolume.value = String(mediaVolume);
  els.mediaVideo.volume = mediaVolume;
  els.mediaAudio.volume = mediaVolume;
  els.mediaVideo.muted = mediaVolume === 0;
  els.mediaAudio.muted = mediaVolume === 0;
}

function seekActiveMedia(seconds: number): void {
  const media = activeMediaElement();
  if (!media) return;
  const duration = Number.isFinite(media.duration) ? media.duration : 0;
  // Allow seek-to-0 even when duration is still unknown (common right after reload).
  const next =
    duration > 0
      ? Math.min(duration, Math.max(0, seconds))
      : Math.max(0, seconds);
  try {
    media.currentTime = next;
  } catch (error) {
    setStatus(`Seek failed: ${error}`);
  }
  updateMediaTimeDisplay();
}

function attachMediaSource(media: HTMLMediaElement, src: string, path: string): void {
  // Always re-bind. Skipping when dataset.path matches left a black frame after
  // tab close (src cleared) or after ended + failed seek on custom protocols.
  media.pause();
  media.removeAttribute("src");
  media.load();
  media.src = src;
  media.dataset.path = path;
  media.load();
}

function updateRateButtons(rate: PlaybackRate): void {
  for (const button of document.querySelectorAll<HTMLButtonElement>(".rate-btn")) {
    const value = Number(button.dataset.rate);
    button.classList.toggle("active", value === rate);
  }
}

function applyPlaybackRate(rate: PlaybackRate): void {
  const tab = activeEditor();
  if (!isAvTab(tab)) return;
  tab.playbackRate = rate;
  const media = activeMediaElement();
  if (media) media.playbackRate = rate;
  updateRateButtons(rate);
  setStatus(`Playback ${rate}x`, tab.path ?? tab.name);
}

function bumpPlaybackRate(direction: 1 | -1): void {
  const tab = activeEditor();
  if (!isAvTab(tab)) return;
  applyPlaybackRate(nextPlaybackRate(tab.playbackRate, direction));
}

async function mediaSrcFor(path: string, _kind: MediaKind): Promise<string> {
  // Privileged workbench-media:// protocol streams local files (incl. video scrub).
  return toMediaUrlAsync(path);
}

async function showMediaTab(tab: EditorTab): Promise<void> {
  if (!tab.path || !isMediaTab(tab)) return;
  const loadId = ++mediaLoadToken;
  pauseAllMedia();

  els.editor.classList.add("hidden");
  els.preview.classList.add("hidden");
  els.editorToolbar.classList.add("hidden");
  els.mediaView.classList.remove("hidden");

  if (tab.kind === "image") {
    els.avStage.classList.add("hidden");
    els.imageStage.classList.remove("hidden");
    try {
      const src = await mediaSrcFor(tab.path, "image");
      if (loadId !== mediaLoadToken || activeEditorId !== tab.id) return;
      els.mediaImage.src = src;
      els.mediaImage.alt = tab.name;
    } catch (error) {
      setStatus(`Could not load image: ${error}`, tab.path);
    }
    return;
  }

  els.imageStage.classList.add("hidden");
  els.avStage.classList.remove("hidden");
  const rate = normalizePlaybackRate(tab.playbackRate);
  tab.playbackRate = rate;
  updateRateButtons(rate);

  try {
    const src = await mediaSrcFor(tab.path, tab.kind);
    if (loadId !== mediaLoadToken || activeEditorId !== tab.id) return;

    if (tab.kind === "video") {
      els.audioStage.classList.add("hidden");
      els.videoTitle.classList.remove("hidden");
      els.videoTitle.textContent = tab.name;
      els.mediaVideo.classList.remove("hidden");
      els.mediaAudio.removeAttribute("src");
      els.mediaAudio.load();
      delete els.mediaAudio.dataset.path;
      attachMediaSource(els.mediaVideo, src, tab.path);
      els.mediaVideo.playbackRate = rate;
      els.mediaVideo.volume = mediaVolume;
    } else {
      els.mediaVideo.classList.add("hidden");
      els.videoTitle.classList.add("hidden");
      els.mediaVideo.removeAttribute("src");
      els.mediaVideo.load();
      delete els.mediaVideo.dataset.path;
      els.audioStage.classList.remove("hidden");
      els.audioTitle.textContent = tab.name;
      attachMediaSource(els.mediaAudio, src, tab.path);
      els.mediaAudio.playbackRate = rate;
      els.mediaAudio.volume = mediaVolume;
    }
    els.mediaVolume.value = String(mediaVolume);
    updateMediaTimeDisplay();
    updatePlayPauseButton();
  } catch (error) {
    setStatus(`Could not load media: ${error}`, tab.path);
  }
}

function hideMediaView(): void {
  pauseAllMedia();
  els.mediaView.classList.add("hidden");
  els.imageStage.classList.add("hidden");
  els.avStage.classList.add("hidden");
  els.videoTitle.classList.add("hidden");
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (
    target === els.editor ||
    target === els.chatInput ||
    target === els.findInput ||
    target === els.replaceInput
  ) {
    return true;
  }
  if (target instanceof HTMLElement) {
    if (target.classList.contains("tree-name-input")) return true;
    if (target.closest(".tree-name-input")) return true;
    const tag = target.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || target.isContentEditable) {
      return true;
    }
  }
  return false;
}

function pathForFileOps(): string | null {
  if (selectedPath && projectRoot) {
    const root = normalizePath(projectRoot);
    const selected = normalizePath(selectedPath);
    if (selected === root) return null;
    if (selected === root || selected.startsWith(`${root}/`)) {
      return selectedPath;
    }
  }
  const tab = activeEditor();
  if (tab?.path && tab.kind !== "settings") return tab.path;
  return null;
}

function focusEditor(): void {
  if (!els.editor.classList.contains("hidden")) {
    els.editor.focus();
  }
}

/** Ignore dismiss events that fire in the same gesture as opening the menu. */
let contextMenuOpenedAt = 0;

function hideContextMenu(): void {
  els.contextMenu.classList.add("hidden");
  els.contextMenu.innerHTML = "";
}

function dismissContextMenu(event?: Event): void {
  if (performance.now() - contextMenuOpenedAt < 250) return;
  if (
    event?.target instanceof Node &&
    els.contextMenu.contains(event.target)
  ) {
    return;
  }
  hideContextMenu();
}

function showContextMenu(clientX: number, clientY: number, items: ContextMenuItem[]): void {
  // Keep the menu on document.body so panel overflow/transform cannot clip it.
  if (els.contextMenu.parentElement !== document.body) {
    document.body.append(els.contextMenu);
  }

  els.contextMenu.innerHTML = "";
  for (const item of items) {
    if (item.type === "separator") {
      const sep = document.createElement("div");
      sep.className = "menu-separator";
      els.contextMenu.append(sep);
      continue;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.role = "menuitem";
    button.textContent = item.label;
    button.disabled = Boolean(item.disabled);
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      hideContextMenu();
      item.action();
    });
    els.contextMenu.append(button);
  }

  contextMenuOpenedAt = performance.now();
  els.contextMenu.classList.remove("hidden");
  const menuRect = els.contextMenu.getBoundingClientRect();
  const x = Math.min(clientX, window.innerWidth - menuRect.width - 8);
  const y = Math.min(clientY, window.innerHeight - menuRect.height - 8);
  els.contextMenu.style.left = `${Math.max(8, x)}px`;
  els.contextMenu.style.top = `${Math.max(8, y)}px`;
}

function syncTabFromEditor(): void {
  const tab = activeEditor();
  if (!isTextTab(tab) || tab.preview) return;
  tab.contents = els.editor.value;
  tab.selectionStart = els.editor.selectionStart;
  tab.selectionEnd = els.editor.selectionEnd;
  tab.dirty = tab.contents !== tab.original;
}

function pushUndoSnapshot(tab: EditorTab, previous: string): void {
  if (applyingHistory) return;
  if (tab.undoStack[tab.undoStack.length - 1] === previous) return;
  tab.undoStack.push(previous);
  if (tab.undoStack.length > MAX_UNDO) {
    tab.undoStack.shift();
  }
  tab.redoStack = [];
}

function setEditorValue(
  value: string,
  selectionStart?: number,
  selectionEnd?: number,
): void {
  els.editor.value = value;
  const start = selectionStart ?? value.length;
  const end = selectionEnd ?? start;
  els.editor.setSelectionRange(start, end);
  lastEditorSnapshot = value;
}

function updateExplorerActionState(): void {
  const enabled = Boolean(projectRoot);
  els.newFileBtn.disabled = !enabled;
  els.newFolderBtn.disabled = !enabled;
}

function syncSettingsTabDirty(): void {
  const tab = editorTabs.find((item) => item.kind === "settings");
  if (!tab) return;
  tab.dirty = settingsAreDirty();
  renderEditorTabs();
  if (activeEditorId === tab.id) {
    setStatus("Settings", tab.dirty ? "Unsaved settings changes" : "");
  }
}

function renderEditorTabs(): void {
  els.editorTabs.innerHTML = "";
  for (const tab of editorTabs) {
    if (tab.kind === "workflow" || tab.kind === "voice" || tab.kind === "settings") continue;
    const dirty = tab.dirty;
    const button = document.createElement("button");
    button.type = "button";
    button.className = `tab${tab.id === activeEditorId ? " active" : ""}${dirty ? " dirty" : ""}`;
    button.role = "tab";
    button.title =
      tab.kind === "docs"
        ? "Workbench documentation"
        : (tab.path ?? "Untitled (not saved to project yet)");
    button.dataset.tabId = tab.id;
    button.innerHTML = `<span class="tab-label"></span>`;
    button.querySelector(".tab-label")!.textContent = tab.name;

    const close = document.createElement("button");
    close.type = "button";
    close.className = "tab-close";
    close.title = "Close";
    close.setAttribute("aria-label", `Close ${tab.name}`);
    close.textContent = "×";
    close.addEventListener("click", (event) => {
      event.stopPropagation();
      void closeEditorTab(tab.id);
    });

    button.append(close);
    button.addEventListener("click", () => {
      syncTabFromEditor();
      activeEditorId = tab.id;
      rememberFileTab(tab);
      renderEditor();
    });
    button.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      event.stopPropagation();
      syncTabFromEditor();
      activeEditorId = tab.id;
      renderEditor();
      showEditorTabContextMenu(event.clientX, event.clientY);
    });
    els.editorTabs.append(button);
  }
}

function updatePreview(): void {
  const tab = activeEditor();
  if (!tab || !tab.preview) {
    els.preview.innerHTML = "";
    return;
  }
  els.preview.innerHTML = renderMarkdown(tab.contents);
}

function renderEditor(): void {
  renderEditorTabs();
  const tab = activeEditor();
  if (!tab) {
    hideMediaView();
    hideSpecialViews();
    els.editor.classList.add("hidden");
    els.preview.classList.add("hidden");
    els.editorToolbar.classList.add("hidden");
    els.editorEmpty.classList.remove("hidden");
    els.editor.value = "";
    lastEditorSnapshot = "";
    setStatus(projectRoot ? `Project: ${projectRoot}` : "Ready", "");
    return;
  }

  els.editorEmpty.classList.add("hidden");

  if (tab.kind === "settings") {
    hideMediaView();
    hideSpecialViews();
    els.preview.classList.add("hidden");
    els.editor.classList.add("hidden");
    els.editorToolbar.classList.add("hidden");
    if (!els.findBar.classList.contains("hidden")) {
      closeFindBar();
    }
    els.settingsView.classList.remove("hidden");
    renderSettingsPanel();
    setStatus("Settings", settingsAreDirty() ? "Unsaved settings changes" : "");
    return;
  }

  if (tab.kind === "workflow") {
    hideMediaView();
    hideSpecialViews();
    els.preview.classList.add("hidden");
    els.editor.classList.add("hidden");
    els.editorToolbar.classList.add("hidden");
    if (!els.findBar.classList.contains("hidden")) {
      closeFindBar();
    }
    els.workflowView.classList.remove("hidden");
    renderWorkflowDesk();
    setStatus("Studio", workflowStatusLine() || "View → Studio");
    return;
  }

  if (tab.kind === "voice") {
    hideMediaView();
    hideSpecialViews();
    els.preview.classList.add("hidden");
    els.editor.classList.add("hidden");
    els.editorToolbar.classList.add("hidden");
    if (!els.findBar.classList.contains("hidden")) {
      closeFindBar();
    }
    els.voiceView.classList.remove("hidden");
    renderVoicePanel();
    setStatus("Character Voice", "View → Character Voice");
    return;
  }

  if (tab.kind === "docs") {
    hideMediaView();
    hideSpecialViews();
    els.preview.classList.add("hidden");
    els.editor.classList.add("hidden");
    els.editorToolbar.classList.add("hidden");
    if (!els.findBar.classList.contains("hidden")) {
      closeFindBar();
    }
    els.docsView.classList.remove("hidden");
    els.docsView.innerHTML = renderMarkdown(workbenchGuideMd);
    setStatus("Documentation", "Help → Documentation");
    return;
  }

  hideSpecialViews();

  if (tab.kind !== "text") {
    els.preview.classList.add("hidden");
    els.editor.classList.add("hidden");
    if (!els.findBar.classList.contains("hidden")) {
      closeFindBar();
    }
    const canManage = Boolean(tab.path);
    els.editorToolbar.classList.toggle("hidden", !canManage);
    els.saveBtn.classList.add("hidden");
    els.saveAsBtn.classList.add("hidden");
    els.togglePreviewBtn.classList.add("hidden");
    els.renameFileBtn.classList.toggle("hidden", !canManage);
    els.deleteFileBtn.classList.toggle("hidden", !canManage);
    void showMediaTab(tab);
    setStatus(
      tab.kind === "image"
        ? "Image"
        : `Media · ${tab.playbackRate}x · Shift+, / Shift+. change speed`,
      tab.path ?? tab.name,
    );
    return;
  }

  hideMediaView();
  const markdown = tabIsMarkdown(tab);
  els.editorToolbar.classList.remove("hidden");
  els.saveBtn.classList.remove("hidden");
  els.saveAsBtn.classList.remove("hidden");
  els.togglePreviewBtn.classList.toggle("hidden", !markdown);
  els.togglePreviewBtn.textContent = tab.preview ? "Edit" : "Preview";
  const canManageFile = Boolean(tab.path);
  els.renameFileBtn.classList.toggle("hidden", !canManageFile);
  els.deleteFileBtn.classList.toggle("hidden", !canManageFile);

  if (tab.preview && markdown) {
    els.editor.classList.add("hidden");
    els.preview.classList.remove("hidden");
    updatePreview();
  } else {
    tab.preview = false;
    els.preview.classList.add("hidden");
    els.editor.classList.remove("hidden");
    if (els.editor.value !== tab.contents) {
      setEditorValue(tab.contents, tab.selectionStart, tab.selectionEnd);
    } else {
      lastEditorSnapshot = tab.contents;
      els.editor.setSelectionRange(tab.selectionStart, tab.selectionEnd);
    }
  }

  if (!els.findBar.classList.contains("hidden")) {
    updateFindCount();
  }

  setStatus(
    tab.preview ? "Markdown preview" : tab.dirty ? "Unsaved changes" : "Ready",
    tab.path ?? "Untitled",
  );
}

function renderChatTabs(): void {
  els.chatTabs.innerHTML = "";
  els.chatTabs.classList.toggle("hidden", chatTabs.length < 2);
  for (const tab of chatTabs) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `tab${tab.id === activeChatId ? " active" : ""}`;
    button.role = "tab";
    button.innerHTML = `<span class="tab-label"></span>`;
    button.querySelector(".tab-label")!.textContent = tab.title;

    if (chatTabs.length > 1) {
      const close = document.createElement("button");
      close.type = "button";
      close.className = "tab-close";
      close.title = "Close chat";
      close.textContent = "×";
      close.addEventListener("click", (event) => {
        event.stopPropagation();
        closeChatTab(tab.id);
      });
      button.append(close);
    }

    button.addEventListener("click", () => {
      activeChatId = tab.id;
      renderChat();
    });
    els.chatTabs.append(button);
  }
}

function renderChat(): void {
  renderChatTabs();
  renderDirectorContext();
  renderChatAttachments();
  const tab = activeChat();
  els.chatMessages.innerHTML = "";
  if (!tab) {
    setChatWorkingChrome(false);
    return;
  }

  for (const message of tab.messages) {
    const bubble = document.createElement("div");
    bubble.className = `chat-bubble ${message.role === "user" ? "user" : ""}`;
    bubble.innerHTML = `<div class="role"></div><p></p>`;
    const roleLabel =
      message.role === "user"
        ? "You"
        : message.role === "assistant"
          ? "Director"
          : "Workbench";
    bubble.querySelector(".role")!.textContent = roleLabel;
    const approval = message.text.match(/\[\[workbench_approve:([A-Za-z0-9]+)\]\]/);
    bubble.querySelector("p")!.textContent = message.text
      .replace(/\[\[workbench_approve:[A-Za-z0-9]+\]\]/, "")
      .trim();
    if (approval && message.role === "assistant") {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "primary-button";
      button.textContent = "Approve";
      const actionId = approval[1];
      button.addEventListener("click", () => {
        void approveDirectorAction(message.id, actionId);
      });
      bubble.append(button);
    }
    els.chatMessages.append(bubble);
  }

  const busy = tab.busy || chatSendInFlight;
  if (busy) {
    const pending = document.createElement("div");
    pending.className = "chat-bubble chat-pending";
    pending.setAttribute("aria-live", "polite");
    pending.innerHTML =
      `<div class="role">Director</div>` +
      `<p><span class="chat-pending-label">Working on your request</span>` +
      `<span class="chat-pending-dots" aria-hidden="true"><i></i><i></i><i></i></span></p>`;
    els.chatMessages.append(pending);
  }

  els.chatMessages.scrollTop = els.chatMessages.scrollHeight;
  setChatWorkingChrome(busy);
}

async function loadChildren(path: string): Promise<DirEntry[]> {
  return invoke<DirEntry[]>("list_dir", { path });
}

function bindTreeItemContext(
  row: HTMLElement,
  entry: DirEntry | null,
  createParent: string | null,
): void {
  row.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    if (entry) {
      selectedPath = entry.path;
      els.fileTree
        .querySelectorAll(".tree-item.selected")
        .forEach((node) => node.classList.remove("selected"));
      row.classList.add("selected");
    }
    showExplorerContextMenu(event.clientX, event.clientY, entry, createParent);
  });
}

function makeCreateRow(depth: number): HTMLLIElement {
  const item = document.createElement("li");
  const row = document.createElement("div");
  row.className = "tree-item creating";
  row.style.paddingLeft = `${8 + depth * 12}px`;

  const chevron = document.createElement("span");
  chevron.className = "chevron";

  const kind = document.createElement("span");
  kind.className = "kind";
  kind.textContent = pendingCreate?.kind === "folder" ? "d" : "·";

  const input = document.createElement("input");
  input.type = "text";
  input.className = "tree-name-input";
  input.value =
    pendingCreate?.kind === "folder" ? "new-folder" : "untitled.md";
  input.setAttribute(
    "aria-label",
    pendingCreate?.kind === "folder" ? "New folder name" : "New file name",
  );

  row.append(chevron, kind, input);
  item.append(row);

  const finish = (commit: boolean) => {
    if (createCommitInFlight) return;
    if (!commit) {
      cancelPendingCreate();
      return;
    }
    void commitPendingCreate(input.value);
  };

  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") {
      event.preventDefault();
      finish(true);
    } else if (event.key === "Escape") {
      event.preventDefault();
      finish(false);
    }
  });

  input.addEventListener("blur", () => {
    // Allow Escape/Enter handlers to run first.
    window.setTimeout(() => {
      if (pendingCreate && !createCommitInFlight) {
        finish(true);
      }
    }, 0);
  });

  input.addEventListener("click", (event) => event.stopPropagation());
  input.addEventListener("mousedown", (event) => event.stopPropagation());

  // Focus after the row is in the DOM.
  window.requestAnimationFrame(() => {
    input.focus();
    input.select();
  });

  return item;
}

async function renderTreeNode(
  parent: HTMLElement,
  path: string,
  depth: number,
): Promise<void> {
  let entries: DirEntry[];
  try {
    entries = await loadChildren(path);
  } catch (error) {
    const note = document.createElement("p");
    note.className = "empty-state";
    note.textContent = String(error);
    parent.append(note);
    return;
  }

  const list = document.createElement("ul");
  list.className = depth === 0 ? "tree-list" : "tree-children";

  if (
    pendingCreate &&
    normalizePath(pendingCreate.parentDir) === normalizePath(path)
  ) {
    list.append(makeCreateRow(depth));
  }

  for (const entry of entries) {
    if (
      pendingRename &&
      normalizePath(pendingRename.path) === normalizePath(entry.path)
    ) {
      list.append(makeRenameRow(entry, depth));
      continue;
    }

    const item = document.createElement("li");
    const row = document.createElement("button");
    row.type = "button";
    row.className = `tree-item${
      selectedPath && normalizePath(selectedPath) === normalizePath(entry.path)
        ? " selected"
        : ""
    }`;
    row.dataset.path = entry.path;
    row.dataset.depth = String(depth);
    row.style.paddingLeft = `${8 + depth * 12}px`;

    const chevron = document.createElement("span");
    chevron.className = "chevron";
    chevron.textContent = entry.isDir
      ? expandedDirs.has(entry.path)
        ? "▼"
        : "▶"
      : "";

    const kind = document.createElement("span");
    kind.className = "kind";
    kind.textContent = entry.isDir ? "d" : "·";

    const label = document.createElement("span");
    label.textContent = entry.name;

    row.append(chevron, kind, label);
    row.tabIndex = 0;
    row.draggable = true;
    if (
      explorerClipboard?.mode === "cut" &&
      explorerClipboard.paths.some(
        (path) => normalizePath(path) === normalizePath(entry.path),
      )
    ) {
      row.classList.add("cut");
    }
    row.addEventListener("dragstart", (event) => {
      event.dataTransfer?.setData(WORKBENCH_PATH_MIME, entry.path);
      event.dataTransfer?.setData("text/plain", entry.path);
      if (event.dataTransfer) {
        event.dataTransfer.effectAllowed = "copyMove";
      }
    });
    row.addEventListener("dragend", () => {
      els.fileTree
        .querySelectorAll(".tree-item.drop-target")
        .forEach((node) => node.classList.remove("drop-target"));
    });
    // Drop onto a folder (or a file's parent) to move; hold Ctrl/Alt to copy.
    bindExplorerDropTarget(row, entry.isDir ? entry.path : dirname(entry.path));
    row.addEventListener("click", () => {
      void onTreeActivate(entry, row);
    });
    bindTreeItemContext(row, entry, entry.isDir ? entry.path : dirname(entry.path));
    item.append(row);

    if (entry.isDir && expandedDirs.has(entry.path)) {
      const childHost = document.createElement("div");
      item.append(childHost);
      await renderTreeNode(childHost, entry.path, depth + 1);
    }

    list.append(item);
  }

  parent.append(list);
}

function explorerRowByPath(path: string | null): HTMLElement | null {
  if (!path) return null;
  const want = normalizePath(path);
  for (const row of els.fileTree.querySelectorAll<HTMLElement>(".tree-item[data-path]")) {
    if (normalizePath(row.dataset.path ?? "") === want) {
      return row;
    }
  }
  return null;
}

function syncExplorerSelection(path: string | null): void {
  for (const node of els.fileTree.querySelectorAll(".tree-item.selected")) {
    node.classList.remove("selected");
  }
  explorerRowByPath(path)?.classList.add("selected");
}

function restoreExplorerViewport(scrollTop: number, scrollLeft: number): void {
  els.fileTree.scrollTop = scrollTop;
  els.fileTree.scrollLeft = scrollLeft;
  if (pendingCreate || pendingRename) return;
  explorerRowByPath(selectedPath)?.focus({ preventScroll: true });
}

async function renderExplorer(): Promise<void> {
  const seq = ++explorerRenderSeq;
  const scrollTop = els.fileTree.scrollTop;
  const scrollLeft = els.fileTree.scrollLeft;
  updateExplorerActionState();
  if (!projectRoot) {
    pendingCreate = null;
    pendingRename = null;
    els.projectName.textContent = "No project";
    els.projectName.title = "";
    updateShellChrome();
    els.fileTree.innerHTML =
      '<p class="empty-state">Open a project folder from <strong>File → Open Project…</strong></p>';
    if (shellSurface === "assets") void renderAssetBrowser();
    return;
  }

  els.projectName.textContent = basename(projectRoot);
  els.projectName.title = projectRoot;
  updateShellChrome();

  const next = document.createElement("div");
  await renderTreeNode(next, projectRoot, 0);
  if (seq !== explorerRenderSeq) return;

  els.fileTree.replaceChildren(...next.childNodes);
  restoreExplorerViewport(scrollTop, scrollLeft);
  if (shellSurface === "assets") void renderAssetBrowser();
}

async function onTreeActivate(
  entry: DirEntry,
  row: HTMLButtonElement,
): Promise<void> {
  selectedPath = entry.path;
  syncExplorerSelection(entry.path);

  if (entry.isDir) {
    const expanding = !expandedDirs.has(entry.path);
    if (expanding) {
      expandedDirs.add(entry.path);
    } else {
      expandedDirs.delete(entry.path);
    }
    const chevron = row.querySelector(".chevron");
    if (chevron) {
      chevron.textContent = expanding ? "▼" : "▶";
    }
    const item = row.parentElement;
    if (!item) return;
    if (expanding) {
      const childHost = document.createElement("div");
      item.append(childHost);
      const depth = Number(row.dataset.depth ?? "0");
      await renderTreeNode(childHost, entry.path, depth + 1);
    } else {
      row.nextElementSibling?.remove();
    }
    row.focus({ preventScroll: true });
    row.scrollIntoView({ block: "nearest", inline: "nearest" });
    return;
  }

  await openFile(entry.path);
}

async function copyPath(absolute: boolean): Promise<void> {
  const path = pathForCopy();
  if (!path) {
    setStatus("Nothing selected to copy a path from");
    return;
  }
  if (absolute) {
    await navigator.clipboard.writeText(path);
    setStatus("Copied path", path);
    return;
  }
  const relative = relativeToProject(path);
  if (!relative) {
    setStatus("Path is outside the open project", path);
    return;
  }
  await navigator.clipboard.writeText(relative);
  setStatus("Copied relative path", relative);
}

async function createEntryAt(
  parentDir: string,
  kind: "file" | "folder",
): Promise<void> {
  if (!projectRoot) {
    setStatus("Open a project first");
    return;
  }

  pendingRename = null;
  pendingCreate = { parentDir, kind };
  expandedDirs.add(parentDir);
  await renderExplorer();
  setStatus(
    kind === "file"
      ? "Name the new file — Enter to create, Esc to cancel"
      : "Name the new folder — Enter to create, Esc to cancel",
  );
}

function cancelPendingCreate(): void {
  if (!pendingCreate) return;
  pendingCreate = null;
  void renderExplorer();
  setStatus("Create cancelled");
}

async function commitPendingCreate(rawName: string): Promise<void> {
  if (!pendingCreate || createCommitInFlight) return;

  const name = rawName.trim();
  if (!isValidEntryName(name)) {
    setStatus("Enter a single name without path separators");
    const input = els.fileTree.querySelector<HTMLInputElement>(".tree-name-input");
    input?.focus();
    input?.select();
    return;
  }

  const { parentDir, kind } = pendingCreate;
  const target = joinPath(parentDir, name);
  createCommitInFlight = true;
  try {
    if (kind === "file") {
      await invoke("create_file", { path: target });
    } else {
      await invoke("create_dir", { path: target });
    }
  } catch (error) {
    createCommitInFlight = false;
    setStatus(String(error));
    const input = els.fileTree.querySelector<HTMLInputElement>(".tree-name-input");
    input?.focus();
    input?.select();
    return;
  }

  pendingCreate = null;
  createCommitInFlight = false;
  selectedPath = target;
  expandedDirs.add(parentDir);
  await renderExplorer();
  if (kind === "file") {
    await openFile(target);
  }
  setStatus(kind === "file" ? "File created" : "Folder created", target);
}

function retargetOpenPaths(fromPath: string, toPath: string): void {
  const from = normalizePath(fromPath);
  const to = normalizePath(toPath);
  const prefix = `${from}/`;

  for (const tab of editorTabs) {
    if (!tab.path) continue;
    const current = normalizePath(tab.path);
    if (current === from) {
      tab.path = toPath;
      tab.name = basename(toPath);
    } else if (current.startsWith(prefix)) {
      tab.path = `${to}${current.slice(from.length)}`;
      tab.name = basename(tab.path);
    }
  }

  if (selectedPath) {
    const current = normalizePath(selectedPath);
    if (current === from) selectedPath = toPath;
    else if (current.startsWith(prefix)) {
      selectedPath = `${to}${current.slice(from.length)}`;
    }
  }

  const nextExpanded = new Set<string>();
  for (const dir of expandedDirs) {
    const current = normalizePath(dir);
    if (current === from) nextExpanded.add(toPath);
    else if (current.startsWith(prefix)) {
      nextExpanded.add(`${to}${current.slice(from.length)}`);
    } else {
      nextExpanded.add(dir);
    }
  }
  expandedDirs.clear();
  for (const dir of nextExpanded) expandedDirs.add(dir);

  for (const chat of chatTabs) {
    chat.attachments = chat.attachments.map((path) => {
      const current = normalizePath(path);
      if (current === from) return toPath;
      if (current.startsWith(prefix)) return `${to}${current.slice(from.length)}`;
      return path;
    });
  }

  if (els.mediaVideo.dataset.path) {
    const current = normalizePath(els.mediaVideo.dataset.path);
    if (current === from) els.mediaVideo.dataset.path = toPath;
    else if (current.startsWith(prefix)) {
      els.mediaVideo.dataset.path = `${to}${current.slice(from.length)}`;
    }
  }
  if (els.mediaAudio.dataset.path) {
    const current = normalizePath(els.mediaAudio.dataset.path);
    if (current === from) els.mediaAudio.dataset.path = toPath;
    else if (current.startsWith(prefix)) {
      els.mediaAudio.dataset.path = `${to}${current.slice(from.length)}`;
    }
  }

}

function explorerPanelFocused(): boolean {
  const active = document.activeElement;
  return Boolean(active && els.explorerPanel.contains(active));
}

function setExplorerClipboard(paths: string[], mode: "cut" | "copy"): void {
  const cleaned = paths
    .map((path) => path.trim())
    .filter((path) => path && projectRoot && normalizePath(path) !== normalizePath(projectRoot));
  if (cleaned.length === 0) {
    explorerClipboard = null;
    return;
  }
  explorerClipboard = { paths: cleaned, mode };
}

async function cutExplorerEntry(path: string | null = pathForFileOps()): Promise<void> {
  if (!path || !projectRoot) {
    setStatus("Select a file or folder to cut");
    return;
  }
  if (normalizePath(path) === normalizePath(projectRoot)) {
    setStatus("Cannot cut the project root");
    return;
  }
  setExplorerClipboard([path], "cut");
  await renderExplorer();
  setStatus("Cut — paste into a folder (Ctrl+V or context menu)", basename(path));
}

async function copyExplorerEntry(path: string | null = pathForFileOps()): Promise<void> {
  if (!path || !projectRoot) {
    setStatus("Select a file or folder to copy");
    return;
  }
  if (normalizePath(path) === normalizePath(projectRoot)) {
    setStatus("Cannot copy the project root");
    return;
  }
  setExplorerClipboard([path], "copy");
  await renderExplorer();
  setStatus("Copied — paste into a folder (Ctrl+V or context menu)", basename(path));
}

async function pathExistsInParent(parentDir: string, name: string): Promise<boolean> {
  try {
    const kids = await loadChildren(parentDir);
    return kids.some((entry) => entry.name === name);
  } catch {
    return false;
  }
}

async function uniqueDestPath(parentDir: string, name: string): Promise<string> {
  if (!(await pathExistsInParent(parentDir, name))) {
    return joinPath(parentDir, name);
  }
  const dot = name.lastIndexOf(".");
  const hasExt = dot > 0;
  const stem = hasExt ? name.slice(0, dot) : name;
  const ext = hasExt ? name.slice(dot) : "";
  for (let i = 2; i < 1000; i += 1) {
    const candidate = `${stem} (${i})${ext}`;
    if (!(await pathExistsInParent(parentDir, candidate))) {
      return joinPath(parentDir, candidate);
    }
  }
  throw new Error(`Could not find a free name for ${name}`);
}

function wouldNestInsideSelf(source: string, destDir: string): boolean {
  const from = normalizePath(source);
  const into = normalizePath(destDir);
  return into === from || into.startsWith(`${from}/`);
}

async function transferExplorerPaths(
  sources: string[],
  destDir: string,
  mode: "cut" | "copy",
): Promise<void> {
  if (!projectRoot) {
    setStatus("Open a project first");
    return;
  }
  const destNorm = normalizePath(destDir);
  const rootNorm = normalizePath(projectRoot);
  if (destNorm !== rootNorm && !destNorm.startsWith(`${rootNorm}/`)) {
    setStatus("Destination must stay inside the open project");
    return;
  }

  let lastDest = "";
  for (const source of sources) {
    const fromNorm = normalizePath(source);
    if (fromNorm === rootNorm) {
      setStatus("Cannot move/copy the project root");
      return;
    }
    if (mode === "cut" && normalizePath(dirname(source)) === destNorm) {
      setStatus("Already in that folder", basename(source));
      continue;
    }
    if (wouldNestInsideSelf(source, destDir)) {
      setStatus("Cannot place a folder inside itself", basename(source));
      return;
    }
    const dest = await uniqueDestPath(destDir, basename(source));
    try {
      if (mode === "cut") {
        await invoke("rename_path", { from: source, to: dest });
        retargetOpenPaths(source, dest);
      } else {
        await invoke("copy_path", { from: source, to: dest });
      }
      lastDest = dest;
    } catch (error) {
      setStatus(`Could not ${mode === "cut" ? "move" : "copy"}: ${error}`);
      await renderExplorer();
      renderEditor();
      renderChatAttachments();
      return;
    }
  }

  if (mode === "cut") {
    explorerClipboard = null;
  }
  if (lastDest) {
    selectedPath = lastDest;
    expandedDirs.add(destDir);
  }
  await renderExplorer();
  renderEditor();
  renderChatAttachments();
  setStatus(
    mode === "cut" ? "Moved" : "Copied",
    lastDest || destDir,
  );
}

async function pasteExplorerClipboard(
  destDir: string | null = null,
): Promise<void> {
  if (!explorerClipboard || explorerClipboard.paths.length === 0) {
    setStatus("Nothing to paste — cut or copy a file first");
    return;
  }
  const target =
    destDir ??
    (selectedPath
      ? await (async () => {
          // Paste into selected folder, or into the parent of a selected file.
          try {
            const parent = dirname(selectedPath!);
            const kids = await loadChildren(parent);
            const match = kids.find(
              (entry) => normalizePath(entry.path) === normalizePath(selectedPath!),
            );
            return match?.isDir ? selectedPath! : parent;
          } catch {
            return dirname(selectedPath!);
          }
        })()
      : projectRoot);
  if (!target) {
    setStatus("Select a destination folder to paste into");
    return;
  }
  await transferExplorerPaths(
    explorerClipboard.paths,
    target,
    explorerClipboard.mode,
  );
}

function bindExplorerDropTarget(
  row: HTMLElement,
  destDir: string,
): void {
  row.addEventListener("dragover", (event) => {
    const types = event.dataTransfer?.types;
    if (!types || !Array.from(types).includes(WORKBENCH_PATH_MIME)) return;
    event.preventDefault();
    event.stopPropagation();
    const copy = event.ctrlKey || event.metaKey || event.altKey;
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = copy ? "copy" : "move";
    }
    row.classList.add("drop-target");
  });
  row.addEventListener("dragleave", (event) => {
    const related = event.relatedTarget as Node | null;
    if (related && row.contains(related)) return;
    row.classList.remove("drop-target");
  });
  row.addEventListener("drop", (event) => {
    event.preventDefault();
    event.stopPropagation();
    row.classList.remove("drop-target");
    const source =
      event.dataTransfer?.getData(WORKBENCH_PATH_MIME) ||
      event.dataTransfer?.getData("text/plain") ||
      "";
    if (!source.startsWith("/")) return;
    const copy = event.ctrlKey || event.metaKey || event.altKey;
    void transferExplorerPaths([source.trim()], destDir, copy ? "copy" : "cut");
  });
}

function closeTabsForDeletedPath(deletedPath: string): void {
  const deleted = normalizePath(deletedPath);
  const prefix = `${deleted}/`;
  const doomed = editorTabs.filter((tab) => {
    if (!tab.path) return false;
    const current = normalizePath(tab.path);
    return current === deleted || current.startsWith(prefix);
  });
  editorTabs = editorTabs.filter((tab) => !doomed.includes(tab));
  if (!editorTabs.some((tab) => tab.id === activeEditorId)) {
    activeEditorId = editorTabs[0]?.id ?? null;
  }
  if (selectedPath) {
    const current = normalizePath(selectedPath);
    if (current === deleted || current.startsWith(prefix)) {
      selectedPath = null;
    }
  }
  for (const chat of chatTabs) {
    chat.attachments = chat.attachments.filter((path) => {
      const current = normalizePath(path);
      return current !== deleted && !current.startsWith(prefix);
    });
  }
  const keepExpanded = [...expandedDirs].filter((dir) => {
    const current = normalizePath(dir);
    return current !== deleted && !current.startsWith(prefix);
  });
  expandedDirs.clear();
  for (const dir of keepExpanded) expandedDirs.add(dir);
}

async function beginRename(path: string | null = pathForFileOps()): Promise<void> {
  if (!path || !projectRoot) {
    setStatus("Select a file or folder to rename");
    return;
  }
  if (normalizePath(path) === normalizePath(projectRoot)) {
    setStatus("Cannot rename the project root");
    return;
  }
  pendingCreate = null;
  let isDir = false;
  try {
    const parent = dirname(path);
    const siblings = await loadChildren(parent);
    const match = siblings.find((entry) => normalizePath(entry.path) === normalizePath(path));
    isDir = match?.isDir ?? false;
  } catch {
    // Fall back to "no extension => maybe folder" only if listing fails.
    isDir = !basename(path).includes(".");
  }
  pendingRename = { path, isDir };
  expandedDirs.add(dirname(path));
  await renderExplorer();
  setStatus("Rename — Enter to confirm, Esc to cancel", basename(path));
}

function cancelPendingRename(): void {
  if (!pendingRename) return;
  pendingRename = null;
  void renderExplorer();
  setStatus("Rename cancelled");
}

async function commitPendingRename(rawName: string): Promise<void> {
  if (!pendingRename || renameCommitInFlight) return;
  const name = rawName.trim();
  if (!isValidEntryName(name)) {
    setStatus("Enter a single name without path separators");
    const input = els.fileTree.querySelector<HTMLInputElement>(".tree-name-input");
    input?.focus();
    input?.select();
    return;
  }
  const from = pendingRename.path;
  const to = joinPath(dirname(from), name);
  if (normalizePath(from) === normalizePath(to)) {
    pendingRename = null;
    await renderExplorer();
    setStatus("Name unchanged");
    return;
  }
  renameCommitInFlight = true;
  try {
    await invoke("rename_path", { from, to });
  } catch (error) {
    renameCommitInFlight = false;
    setStatus(String(error));
    const input = els.fileTree.querySelector<HTMLInputElement>(".tree-name-input");
    input?.focus();
    input?.select();
    return;
  }
  pendingRename = null;
  renameCommitInFlight = false;
  retargetOpenPaths(from, to);
  selectedPath = to;
  await renderExplorer();
  renderEditor();
  renderChatAttachments();
  setStatus("Renamed", to);
}

async function entryIsDirectory(path: string): Promise<boolean> {
  try {
    const parent = dirname(path);
    const kids = await loadChildren(parent);
    return (
      kids.find((entry) => normalizePath(entry.path) === normalizePath(path))
        ?.isDir ?? false
    );
  } catch {
    return !basename(path).includes(".");
  }
}

/** Release editor/media handles under a path so folder deletes are not blocked. */
function releaseOpenHandlesUnder(targetPath: string): void {
  const target = normalizePath(targetPath);
  const prefix = `${target}/`;
  const hits = (candidate: string | null | undefined): boolean => {
    if (!candidate) return false;
    const current = normalizePath(candidate);
    return current === target || current.startsWith(prefix);
  };

  if (hits(els.mediaVideo.dataset.path) || hits(els.mediaAudio.dataset.path)) {
    pauseAllMedia();
    els.mediaVideo.removeAttribute("src");
    els.mediaAudio.removeAttribute("src");
    delete els.mediaVideo.dataset.path;
    delete els.mediaAudio.dataset.path;
    try {
      els.mediaVideo.load();
      els.mediaAudio.load();
    } catch {
      /* ignore */
    }
  }

  // Drop tabs first so nothing keeps a stale handle after delete.
  closeTabsForDeletedPath(targetPath);
}

async function deleteEntry(path: string | null = pathForFileOps()): Promise<void> {
  if (!path || !projectRoot) {
    setStatus("Select a file or folder to delete");
    return;
  }
  if (normalizePath(path) === normalizePath(projectRoot)) {
    setStatus("Cannot delete the project root");
    return;
  }
  const label = basename(path);
  const isDir = await entryIsDirectory(path);
  const proceed = await confirmAction(
    isDir
      ? `Delete folder "${label}" and everything inside it?\n\n${path}\n\nThis cannot be undone.`
      : `Delete "${label}"?\n\n${path}\n\nThis cannot be undone.`,
    "Delete",
  );
  if (!proceed) {
    setStatus("Delete cancelled");
    return;
  }
  releaseOpenHandlesUnder(path);
  // Give Chromium a tick to drop media streams before the host unlinks.
  await new Promise<void>((resolve) => window.setTimeout(resolve, 40));
  try {
    await invoke("delete_path", { path });
  } catch (error) {
    setStatus(`Could not delete: ${error}`);
    await renderExplorer();
    renderEditor();
    return;
  }
  await renderExplorer();
  renderEditor();
  renderChatAttachments();
  setStatus(isDir ? "Deleted folder" : "Deleted", path);
}

function makeRenameRow(entry: DirEntry, depth: number): HTMLLIElement {
  const item = document.createElement("li");
  const row = document.createElement("div");
  row.className = "tree-item creating";
  row.style.paddingLeft = `${8 + depth * 12}px`;

  const chevron = document.createElement("span");
  chevron.className = "chevron";
  chevron.textContent = entry.isDir ? "▼" : "";

  const kind = document.createElement("span");
  kind.className = "kind";
  kind.textContent = entry.isDir ? "d" : "·";

  const input = document.createElement("input");
  input.type = "text";
  input.className = "tree-name-input";
  input.value = entry.name;
  input.setAttribute("aria-label", "Rename");

  row.append(chevron, kind, input);
  item.append(row);

  const finish = (commit: boolean) => {
    if (renameCommitInFlight) return;
    if (!commit) {
      cancelPendingRename();
      return;
    }
    void commitPendingRename(input.value);
  };

  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") {
      event.preventDefault();
      finish(true);
    } else if (event.key === "Escape") {
      event.preventDefault();
      finish(false);
    }
  });
  input.addEventListener("blur", () => {
    window.setTimeout(() => {
      if (pendingRename && !renameCommitInFlight) finish(true);
    }, 0);
  });
  input.addEventListener("click", (event) => event.stopPropagation());
  input.addEventListener("mousedown", (event) => event.stopPropagation());

  window.requestAnimationFrame(() => {
    input.focus();
    const dot = entry.name.lastIndexOf(".");
    if (!entry.isDir && dot > 0) {
      input.setSelectionRange(0, dot);
    } else {
      input.select();
    }
  });

  return item;
}

function showExplorerContextMenu(
  x: number,
  y: number,
  entry: DirEntry | null,
  createParent: string | null,
): void {
  const parent = createParent ?? projectRoot;
  const copyTarget = entry?.path ?? selectedPath ?? projectRoot;
  const opsTarget =
    entry?.path &&
    projectRoot &&
    normalizePath(entry.path) !== normalizePath(projectRoot)
      ? entry.path
      : null;
  showContextMenu(x, y, [
    {
      type: "item",
      label: "New File…",
      disabled: !parent,
      action: () => {
        if (parent) void createEntryAt(parent, "file");
      },
    },
    {
      type: "item",
      label: "New Folder…",
      disabled: !parent,
      action: () => {
        if (parent) void createEntryAt(parent, "folder");
      },
    },
    { type: "separator" },
    {
      type: "item",
      label: "Cut",
      disabled: !opsTarget,
      action: () => {
        if (opsTarget) void cutExplorerEntry(opsTarget);
      },
    },
    {
      type: "item",
      label: "Copy",
      disabled: !opsTarget,
      action: () => {
        if (opsTarget) void copyExplorerEntry(opsTarget);
      },
    },
    {
      type: "item",
      label: "Paste",
      disabled: !explorerClipboard || !parent,
      action: () => {
        if (parent) void pasteExplorerClipboard(parent);
      },
    },
    { type: "separator" },
    {
      type: "item",
      label: "Rename…",
      disabled: !opsTarget,
      action: () => {
        if (opsTarget) void beginRename(opsTarget);
      },
    },
    {
      type: "item",
      label: "Delete…",
      disabled: !opsTarget,
      action: () => {
        if (opsTarget) void deleteEntry(opsTarget);
      },
    },
    { type: "separator" },
    {
      type: "item",
      label: "Add to Chat",
      disabled: !copyTarget,
      action: () => {
        if (copyTarget) addPathToChat(copyTarget);
      },
    },
    {
      type: "item",
      label: "Open Character Voice…",
      disabled: !copyTarget && !projectRoot,
      action: () => {
        if (copyTarget) selectedPath = copyTarget;
        openVoiceTab();
      },
    },
    {
      type: "item",
      label: "Copy Path",
      disabled: !copyTarget,
      action: () => {
        if (!copyTarget) return;
        selectedPath = copyTarget;
        void copyPath(true);
      },
    },
    {
      type: "item",
      label: "Copy Relative Path",
      disabled: !copyTarget || !projectRoot,
      action: () => {
        if (!copyTarget) return;
        selectedPath = copyTarget;
        void copyPath(false);
      },
    },
  ]);
}

function tabIsDirty(tab: EditorTab): boolean {
  return tab.kind === "settings" ? settingsAreDirty() : tab.dirty;
}

function showEditorTextContextMenu(x: number, y: number): void {
  const tab = activeEditor();
  const canEdit = isTextTab(tab) && !tab.preview;
  const hasSelection =
    canEdit && els.editor.selectionStart !== els.editor.selectionEnd;
  showContextMenu(x, y, [
    {
      type: "item",
      label: "Cut",
      disabled: !canEdit || !hasSelection,
      action: () => {
        void cutSelection();
      },
    },
    {
      type: "item",
      label: "Copy",
      disabled: !hasSelection && document.activeElement !== els.editor,
      action: () => {
        void copySelection();
      },
    },
    {
      type: "item",
      label: "Paste",
      disabled: !canEdit,
      action: () => {
        void pasteClipboard();
      },
    },
    { type: "separator" },
    {
      type: "item",
      label: "Select All",
      disabled: !canEdit,
      action: () => selectAllInEditor(),
    },
    { type: "separator" },
    {
      type: "item",
      label: "Find…",
      disabled: !canEdit,
      action: () => openFindBar(false),
    },
    {
      type: "item",
      label: "Replace…",
      disabled: !canEdit,
      action: () => openFindBar(true),
    },
  ]);
}

function showEditorTabContextMenu(x: number, y: number): void {
  const tab = activeEditor();
  const canSave = isTextTab(tab);
  const hasTabs = editorTabs.length > 0;
  const hasOthers = editorTabs.length > 1;
  const hasSaved = editorTabs.some((item) => !tabIsDirty(item));
  showContextMenu(x, y, [
    {
      type: "item",
      label: "New Text File",
      action: () => openUntitledFile(),
    },
    {
      type: "item",
      label: "Open File…",
      disabled: !projectRoot,
      action: () => {
        void openFileDialog();
      },
    },
    { type: "separator" },
    {
      type: "item",
      label: "Add to Chat",
      disabled: !tab?.path,
      action: () => {
        if (tab?.path) addPathToChat(tab.path);
      },
    },
    { type: "separator" },
    {
      type: "item",
      label: "Save",
      disabled: !canSave,
      action: () => {
        void saveActiveFile();
      },
    },
    {
      type: "item",
      label: "Save As…",
      disabled: !canSave,
      action: () => {
        void saveActiveFileAs();
      },
    },
    { type: "separator" },
    {
      type: "item",
      label: "Close",
      disabled: !tab,
      action: () => {
        if (tab) void closeEditorTab(tab.id);
      },
    },
    {
      type: "item",
      label: "Close Others",
      disabled: !hasOthers,
      action: () => {
        void closeOtherEditorTabs();
      },
    },
    {
      type: "item",
      label: "Close Saved",
      disabled: !hasSaved,
      action: () => {
        void closeSavedEditorTabs();
      },
    },
    {
      type: "item",
      label: "Close All",
      disabled: !hasTabs,
      action: () => {
        void closeAllEditorTabs();
      },
    },
  ]);
}

function openUntitledFile(): void {
  syncTabFromEditor();
  pauseAllMedia();
  const tab: EditorTab = {
    id: uid("editor"),
    kind: "text",
    path: null,
    name: `Untitled-${untitledCounter++}`,
    contents: "",
    original: "",
    dirty: true,
    undoStack: [],
    redoStack: [],
    preview: false,
    selectionStart: 0,
    selectionEnd: 0,
    playbackRate: 1,
  };
  editorTabs.push(tab);
  activeEditorId = tab.id;
  placeFileSurface(tab, true);
  renderEditor();
  focusEditor();
  setStatus("Untitled buffer — save to add it to the project");
}

async function openFile(path: string): Promise<void> {
  const existing = editorTabs.find((tab) => tab.path === path);
  if (existing) {
    syncTabFromEditor();
    pauseAllMedia();
    activeEditorId = existing.id;
    placeFileSurface(existing);
    renderEditor();
    if (shellSurface === "assets") void renderAssetBrowser();
    if (existing.kind === "text") focusEditor();
    return;
  }

  const kind = await invoke<OpenKind>("classify_file", { path });
  if (kind === "unsupported") {
    setStatus("Unsupported file type", path);
    window.alert(UNSUPPORTED_HINT);
    return;
  }

  syncTabFromEditor();
  pauseAllMedia();

  if (kind === "text") {
    const contents = await invoke<string>("read_text_file", { path });
    const tab: EditorTab = {
      id: uid("editor"),
      kind: "text",
      path,
      name: basename(path),
      contents,
      original: contents,
      dirty: false,
      undoStack: [],
      redoStack: [],
      preview: false,
      selectionStart: 0,
      selectionEnd: 0,
      playbackRate: 1,
    };
    editorTabs.push(tab);
    activeEditorId = tab.id;
    renderEditor();
    focusEditor();
    return;
  }

  if (!isMediaKind(kind)) {
    setStatus("Unsupported file type", path);
    return;
  }

  const tab: EditorTab = {
    id: uid("editor"),
    kind,
    path,
    name: basename(path),
    contents: "",
    original: "",
    dirty: false,
    undoStack: [],
    redoStack: [],
    preview: false,
    selectionStart: 0,
    selectionEnd: 0,
    playbackRate: 1,
  };
  editorTabs.push(tab);
  activeEditorId = tab.id;
  placeFileSurface(tab);
  renderEditor();
  if (shellSurface === "assets") void renderAssetBrowser();
}

async function openFileDialog(): Promise<void> {
  if (!projectRoot) {
    setStatus("Open a project first");
    return;
  }
  const selected = await openDialog({
    multiple: false,
    directory: false,
    title: "Open File",
    defaultPath: projectRoot,
    filters: OPEN_FILTERS,
  });
  if (!selected || Array.isArray(selected)) return;
  await openFile(selected);
  selectedPath = selected;
  const parent = dirname(selected);
  if (parent) expandedDirs.add(parent);
  await renderExplorer();
}

async function closeEditorTab(
  id: string,
  options?: { force?: boolean },
): Promise<boolean> {
  const tab = editorTabs.find((item) => item.id === id);
  if (!tab) return false;

  if (activeEditorId === id) {
    syncTabFromEditor();
    pauseAllMedia();
  }

  if (!options?.force) {
    if (tab.kind === "settings" && settingsAreDirty()) {
      const proceed = await confirmAction(
        "Settings have unsaved changes. Close anyway?",
        "Unsaved settings",
      );
      if (!proceed) return false;
    } else if (tab.dirty) {
      const proceed = await confirmAction(
        `"${tab.name}" has unsaved changes. Close anyway?`,
        "Unsaved changes",
      );
      if (!proceed) return false;
    }
  }

  if (tab.path && tab.kind !== "text") {
    const stillOpen = editorTabs.some(
      (item) => item.id !== id && item.path === tab.path,
    );
    if (!stillOpen) {
      if (els.mediaVideo.dataset.path === tab.path) {
        els.mediaVideo.removeAttribute("src");
        els.mediaVideo.load();
        delete els.mediaVideo.dataset.path;
      }
      if (els.mediaAudio.dataset.path === tab.path) {
        els.mediaAudio.removeAttribute("src");
        els.mediaAudio.load();
        delete els.mediaAudio.dataset.path;
      }
    }
  }

  const index = editorTabs.findIndex((item) => item.id === id);
  editorTabs = editorTabs.filter((item) => item.id !== id);
  if (activeEditorId === id) {
    const next = editorTabs[index] ?? editorTabs[index - 1] ?? null;
    activeEditorId = next?.id ?? null;
  }
  renderEditor();
  return true;
}

async function closeEditorTabsByIds(ids: string[]): Promise<void> {
  if (ids.length === 0) return;
  syncTabFromEditor();
  const dirtyCount = ids.reduce((count, id) => {
    const tab = editorTabs.find((item) => item.id === id);
    return tab && tabIsDirty(tab) ? count + 1 : count;
  }, 0);
  if (dirtyCount > 0) {
    const proceed = await confirmAction(
      dirtyCount === 1
        ? "1 tab has unsaved changes. Close anyway?"
        : `${dirtyCount} tabs have unsaved changes. Close anyway?`,
      "Unsaved changes",
    );
    if (!proceed) {
      setStatus("Close cancelled");
      return;
    }
  }
  for (const id of ids) {
    await closeEditorTab(id, { force: true });
  }
  setStatus(
    ids.length === 1 ? "Closed 1 tab" : `Closed ${ids.length} tabs`,
  );
}

async function closeOtherEditorTabs(): Promise<void> {
  const keepId = activeEditorId;
  if (!keepId) return;
  const ids = editorTabs
    .filter((tab) => tab.id !== keepId)
    .map((tab) => tab.id);
  await closeEditorTabsByIds(ids);
}

async function closeSavedEditorTabs(): Promise<void> {
  syncTabFromEditor();
  const ids = editorTabs
    .filter((tab) => !tabIsDirty(tab))
    .map((tab) => tab.id);
  if (ids.length === 0) {
    setStatus("No saved tabs to close");
    return;
  }
  for (const id of ids) {
    await closeEditorTab(id, { force: true });
  }
  setStatus(
    ids.length === 1 ? "Closed 1 saved tab" : `Closed ${ids.length} saved tabs`,
  );
}

async function closeAllEditorTabs(): Promise<void> {
  const ids = editorTabs.map((tab) => tab.id);
  await closeEditorTabsByIds(ids);
}

function closeChatTab(id: string): void {
  if (chatTabs.length <= 1) return;
  const index = chatTabs.findIndex((item) => item.id === id);
  chatTabs = chatTabs.filter((item) => item.id !== id);
  if (activeChatId === id) {
    const next = chatTabs[index] ?? chatTabs[index - 1] ?? null;
    activeChatId = next?.id ?? null;
  }
  renderChat();
}

async function confirmOpenProjectSwitch(): Promise<boolean> {
  syncTabFromEditor();
  const dirtyFiles = editorTabs.some(
    (tab) => tab.kind !== "settings" && tab.kind !== "docs" && tab.dirty,
  );
  const dirtySettings = settingsAreDirty();
  if (!dirtyFiles && !dirtySettings) return true;
  return confirmAction(
    dirtySettings && !dirtyFiles
      ? "Settings have unsaved changes. Open a new project anyway?"
      : "You have unsaved changes. Open a new project anyway?",
    "Open project",
  );
}

async function openProjectAt(
  selected: string,
  options?: { sessionReady?: boolean },
): Promise<void> {
  const root = selected.replace(/\/+$/, "");
  if (!options?.sessionReady) {
    if (!(await confirmOpenProjectSwitch())) return;
    try {
      await useRecentProject(root);
    } catch {
      setStatus("Recent project folder is missing", root);
      void removeRecentProject(root);
      return;
    }
  }
  try {
    await invoke("list_dir", { path: root });
  } catch {
    setStatus("Recent project folder is missing", root);
    void removeRecentProject(root);
    return;
  }

  projectRoot = root;
  selectedPath = null;
  expandedDirs.clear();
  expandedDirs.add(root);
  // Settings / docs / voice are app-level — keep those tabs across project switches.
  editorTabs = editorTabs.filter((tab) => isAppLevelTab(tab));
  activeEditorId = editorTabs[0]?.id ?? null;
  workflowLoadedRoot = null;
  clearWorkflowDesk();
  els.mediaVideo.removeAttribute("src");
  els.mediaAudio.removeAttribute("src");
  delete els.mediaVideo.dataset.path;
  delete els.mediaAudio.dataset.path;
  closeFindBar();
  await renderExplorer();
  renderEditor();
  setStatus("Project opened", root);
  void rememberRecentProject(root);
  await loadProjectWorkflow(root);
  await renderExplorer();
}

async function openProject(): Promise<void> {
  if (!(await confirmOpenProjectSwitch())) return;
  const selected = await openProjectFolder();
  if (!selected) return;
  await openProjectAt(selected, { sessionReady: true });
}

async function closeProject(): Promise<void> {
  if (!projectRoot) return;
  syncTabFromEditor();
  const dirtyFiles = editorTabs.some(
    (tab) => tab.kind !== "settings" && tab.kind !== "docs" && tab.dirty,
  );
  if (dirtyFiles) {
    const proceed = await confirmAction(
      "You have unsaved editor changes. Close the project anyway?",
      "Close project",
    );
    if (!proceed) return;
  }
  projectRoot = null;
  selectedPath = null;
  expandedDirs.clear();
  editorTabs = editorTabs.filter((tab) => isAppLevelTab(tab));
  activeEditorId = editorTabs[0]?.id ?? null;
  workflowLoadedRoot = null;
  clearWorkflowDesk();
  els.mediaVideo.removeAttribute("src");
  els.mediaAudio.removeAttribute("src");
  delete els.mediaVideo.dataset.path;
  delete els.mediaAudio.dataset.path;
  closeFindBar();
  await closeProjectSession();
  await renderExplorer();
  renderEditor();
  setStatus("Project closed");
}

async function saveActiveFile(): Promise<void> {
  const tab = activeEditor();
  if (isSettingsTab(tab)) {
    try {
      await persistSettings();
      refreshChatModelSelect();
      renderSettingsPanel();
      setStatus("Settings saved");
    } catch (error) {
      setStatus(`Could not save settings: ${error}`);
    }
    return;
  }
  if (isDocsTab(tab) || tab?.kind === "voice") {
    setStatus(tab?.kind === "voice" ? "Save voices with Approve / Choose in the Voice tab" : "Documentation is read-only");
    return;
  }
  if (!isTextTab(tab)) {
    setStatus(tab ? "Media tabs are view-only" : "Nothing to save");
    return;
  }
  syncTabFromEditor();
  if (!tab.path) {
    await saveActiveFileAs();
    return;
  }
  await invoke("write_text_file", { path: tab.path, contents: tab.contents });
  tab.original = tab.contents;
  tab.dirty = false;
  renderEditor();
  setStatus("Saved", tab.path);
}

async function saveActiveFileAs(): Promise<void> {
  const tab = activeEditor();
  if (isSettingsTab(tab)) {
    await saveActiveFile();
    return;
  }
  if (isDocsTab(tab) || tab?.kind === "voice") {
    setStatus(tab?.kind === "voice" ? "Save voices with Approve / Choose in the Voice tab" : "Documentation is read-only");
    return;
  }
  if (!isTextTab(tab)) {
    setStatus(tab ? "Media tabs are view-only" : "Nothing to save");
    return;
  }
  syncTabFromEditor();

  const defaultPath = tab.path
    ? tab.path
    : projectRoot
      ? joinPath(projectRoot, `${tab.name}.md`)
      : `${tab.name}.md`;

  const selected = await saveDialog({
    title: "Save As",
    defaultPath,
    filters: OPEN_FILTERS,
  });
  if (!selected) return;

  await invoke("write_text_file", { path: selected, contents: tab.contents });
  tab.path = selected;
  tab.name = basename(selected);
  tab.original = tab.contents;
  tab.dirty = false;
  selectedPath = selected;
  const parent = dirname(selected);
  if (projectRoot && normalizePath(parent).startsWith(normalizePath(projectRoot))) {
    expandedDirs.add(parent);
    await renderExplorer();
  }
  renderEditor();
  setStatus("Saved", selected);
}

function directorHistory(tab: ChatTab): Array<{ role: string; content: string }> {
  return tab.messages
    .filter((message) => message.role === "user" || message.role === "assistant")
    .map((message) => ({ role: message.role, content: message.text }));
}

function chatRoleLabel(role: ChatMessage["role"]): string {
  if (role === "user") return "You";
  if (role === "assistant") return "Director";
  return "Workbench";
}

function chatTranscriptFilename(tab: ChatTab): string {
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
  const slug =
    tab.title
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 40) || "chat";
  return `director-${slug}-${stamp}.md`;
}

function formatChatTranscript(tab: ChatTab): string {
  const settings = getSettings();
  const model = settings.selectedChatModel;
  const modelLine = model?.modelId
    ? `${model.modelId}${model.apiId ? ` (${model.apiId})` : ""}`
    : "(none selected)";
  const otherTabs = chatTabs
    .filter((item) => item.id !== tab.id)
    .map((item) => item.title);
  const busy = tab.busy || chatSendInFlight;
  const draft = els.chatInput.value.trim();
  const status = [els.statusLeft.textContent, els.statusRight.textContent]
    .map((part) => (part ?? "").trim())
    .filter(Boolean)
    .join(" — ");

  const lines: string[] = [
    `# Director chat — ${tab.title}`,
    "",
    `Captured: ${new Date().toISOString()}`,
    `Project: ${projectRoot ?? "(none open)"}`,
    `Selected path: ${selectedPath ?? "(none)"}`,
    `Chat model: ${modelLine}`,
    `Status bar: ${status || "(empty)"}`,
    `Director: ${busy ? "working" : directorStopping ? "stopping" : "idle"}`,
  ];
  if (tab.attachments.length > 0) {
    lines.push("Attachments:");
    for (const path of tab.attachments) {
      lines.push(`- ${path}`);
    }
  } else {
    lines.push("Attachments: (none)");
  }
  if (otherTabs.length > 0) {
    lines.push(`Other open chats: ${otherTabs.join(", ")}`);
  }
  lines.push("", "---", "");

  if (tab.messages.length === 0) {
    lines.push("_(No messages in this chat yet.)_", "");
  } else {
    for (const message of tab.messages) {
      lines.push(`## ${chatRoleLabel(message.role)}`, "", message.text.trim() || "_(empty)_", "");
    }
  }

  if (draft) {
    lines.push("## Draft (not sent)", "", draft, "");
  }

  return ensureTrailingNewline(lines.join("\n"));
}

function ensureTrailingNewline(text: string): string {
  return text.endsWith("\n") ? text : `${text}\n`;
}

async function copyChatTranscript(): Promise<void> {
  const tab = activeChat();
  if (!tab) {
    setStatus("No chat to copy");
    return;
  }
  const text = formatChatTranscript(tab);
  try {
    await navigator.clipboard.writeText(text);
    setStatus(
      "Chat transcript copied",
      `${tab.messages.length} message${tab.messages.length === 1 ? "" : "s"}`,
    );
  } catch (error) {
    setStatus(`Could not copy chat: ${error}`);
  }
}

async function saveChatTranscript(): Promise<void> {
  const tab = activeChat();
  if (!tab) {
    setStatus("No chat to save");
    return;
  }
  const filename = chatTranscriptFilename(tab);
  const defaultPath = projectRoot ? joinPath(projectRoot, filename) : filename;
  const selected = await saveDialog({
    title: "Save chat transcript",
    defaultPath,
    filters: [
      { name: "Markdown", extensions: ["md"] },
      { name: "Text", extensions: ["txt"] },
      { name: "All Files", extensions: ["*"] },
    ],
  });
  if (!selected) return;
  try {
    await invoke("write_text_file", {
      path: selected,
      contents: formatChatTranscript(tab),
    });
    if (projectRoot) {
      const parent = dirname(selected);
      if (normalizePath(parent).startsWith(normalizePath(projectRoot))) {
        expandedDirs.add(parent);
        await renderExplorer();
      }
    }
    setStatus("Chat transcript saved", selected);
  } catch (error) {
    setStatus(`Could not save chat: ${error}`);
  }
}

function showChatContextMenu(clientX: number, clientY: number): void {
  const selection = window.getSelection()?.toString() ?? "";
  const items: ContextMenuItem[] = [];
  if (selection.trim()) {
    items.push({
      type: "item",
      label: "Copy Selection",
      action: () => {
        void navigator.clipboard.writeText(selection).then(
          () => setStatus("Copied selection"),
          (error: unknown) => setStatus(`Could not copy: ${error}`),
        );
      },
    });
  }
  items.push(
    {
      type: "item",
      label: "Copy Chat Transcript",
      action: () => {
        void copyChatTranscript();
      },
    },
    {
      type: "item",
      label: "Save Chat Transcript…",
      action: () => {
        void saveChatTranscript();
      },
    },
  );
  showContextMenu(clientX, clientY, items);
}

function focusedEditorExcerpt(tab: EditorTab | undefined): string | null {
  if (isWorkflowTab(tab)) return workflowSelection().excerpt;
  if (!isTextTab(tab) || !tab.contents.trim()) return null;
  return tab.contents;
}

/** Let WebKit paint the Working UI before a long director_turn invoke. */
function paintSoon(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      requestAnimationFrame(() => resolve());
    });
  });
}

function setChatWorkingChrome(busy: boolean): void {
  els.chatPanel.classList.toggle("chat-busy", busy);
  if (busy) {
    els.chatPanel.setAttribute("aria-busy", "true");
  } else {
    els.chatPanel.removeAttribute("aria-busy");
  }
  els.chatInput.disabled = false;
  els.chatInput.placeholder = directorStopping
    ? "Stopping the Director…"
    : busy
      ? "Director is working — Stop or Esc to cancel"
      : "Ask the Director…";
  els.chatSendBtn.disabled = busy;
  els.chatSendBtn.classList.toggle("is-working", busy && !directorStopping);
  els.chatSendBtn.textContent = busy ? "Working…" : "Send";
  els.chatStopBtn.classList.toggle("hidden", !busy);
  els.chatStopBtn.disabled = directorStopping;
  els.chatStopBtn.textContent = directorStopping ? "Stopping…" : "Stop";
}

/** Live progress line shown in chat while a Director turn is running. */
let directorProgressMessageId: string | null = null;
/** Caps host-driven resume hops for planner yields and long model-sheet packs. */
let autoContinueDepth = 0;
const MAX_AUTO_CONTINUE = 48;
/** User asked to halt the current Director turn / auto-continue chain. */
let directorStopRequested = false;
let directorStopping = false;

async function stopDirector(): Promise<void> {
  if (directorStopping) return;
  directorStopRequested = true;
  directorStopping = true;
  autoContinueDepth = MAX_AUTO_CONTINUE;
  setChatWorkingChrome(chatSendInFlight || Boolean(activeChat()?.busy));
  setStatus("Stopping Director…");
  upsertDirectorProgressMessage("Stopping Director…");
  try {
    await invoke("director_cancel");
  } catch (error) {
    setStatus(`Stop request failed: ${error}`);
  }
}

function looksLikeStoppedError(error: unknown): boolean {
  const text = String(error);
  return /stopped by user/i.test(text);
}

function upsertDirectorProgressMessage(text: string): void {
  const tab = activeChat();
  if (!tab) return;
  const cleaned = text.trim();
  if (!cleaned) return;
  if (directorProgressMessageId) {
    const existing = tab.messages.find(
      (message) => message.id === directorProgressMessageId,
    );
    if (existing) {
      existing.text = cleaned;
      renderChat();
      return;
    }
  }
  directorProgressMessageId = uid("msg");
  tab.messages.push({
    id: directorProgressMessageId,
    role: "system",
    text: cleaned,
  });
  renderChat();
}

function clearDirectorProgressMessage(): void {
  directorProgressMessageId = null;
}

function currentDirectorContext() {
  const settings = getSettings();
  const editor = activeEditor();
  const scope = directorTurnSelection();
  const tab = activeChat();
  return {
    projectRoot,
    selectedPath,
    focusedEditorPath: isWorkflowTab(editor)
      ? workflowSelection().path
      : (editor?.path ?? null),
    focusedEditorExcerpt: focusedEditorExcerpt(editor),
    attachedPaths: [...(tab?.attachments ?? [])],
    capabilityDefaults: settings.capabilityDefaults ?? {},
    adultContentEnabled: Boolean(settings.adultContentEnabled),
    adultCapabilityDefaults: settings.adultCapabilityDefaults ?? {},
    workflowStage: scope.stage,
    workflowSceneId: scope.sceneId,
    workflowSceneStableId: scope.sceneStableId,
    workflowShotStableId: scope.shotStableId,
    workflowAssetId: scope.assetId,
    workflowLookSlotId: scope.lookSlotId,
    workflowRequirementId: scope.requirementId,
    workflowWorkArea: scope.workArea,
  };
}

async function approveDirectorAction(
  messageId: string,
  actionId: string,
): Promise<void> {
  const tab = activeChat();
  if (!tab || tab.busy || chatSendInFlight) return;
  const selected = getSettings().selectedChatModel;
  if (!selected?.modelId) {
    setStatus("Select a chat model before approving");
    return;
  }
  tab.busy = true;
  chatSendInFlight = true;
  renderChat();
  setStatus("Running approved plan…", selected.modelId);
  try {
    const result = await invoke<{
      reply: string;
      model: string;
      toolEvents: string[];
      continueSuggested?: boolean;
    }>("director_approve", {
      id: actionId,
      model: selected.modelId,
      apiId: selected.apiId,
      context: currentDirectorContext(),
    });
    const source = tab.messages.find((message) => message.id === messageId);
    if (source) {
      source.text = source.text
        .replace(/\[\[workbench_approve:[A-Za-z0-9]+\]\]/, "")
        .trim();
    }
    tab.messages.push({
      id: uid("msg"),
      role: "assistant",
      text: result.reply,
    });
    setStatus("Approved plan finished", result.model);
    if (projectRoot) {
      await renderExplorer();
      try {
        await refreshWorkflowDesk();
      } catch {
        /* desk refresh is best-effort after an approval */
      }
    }
  } catch (error) {
    tab.messages.push({
      id: uid("msg"),
      role: "system",
      text: `Approval failed: ${error}`,
    });
    setStatus(`Approval error: ${error}`);
  } finally {
    tab.busy = false;
    chatSendInFlight = false;
    renderChat();
  }
}

async function sendChatMessage(
  text: string,
  options?: { autoContinue?: boolean; resume?: boolean },
): Promise<void> {
  const tab = activeChat();
  const isResume = Boolean(options?.resume);
  if (!tab || tab.busy || chatSendInFlight) return;
  if (options?.autoContinue && directorStopRequested) {
    directorStopping = false;
    tab.messages.push({
      id: uid("msg"),
      role: "system",
      text: "Stopped. The Director will not continue this turn.",
    });
    renderChat();
    setStatus("Director stopped");
    return;
  }
  if (!isResume && !text.trim()) return;

  const userText = text.trim();
  if (!options?.autoContinue && !isResume) {
    autoContinueDepth = 0;
    directorStopRequested = false;
    directorStopping = false;
  }
  // Clear immediately so Send never looks like it failed to take the message.
  els.chatInput.value = "";

  const settings = getSettings();
  const selected = settings.selectedChatModel;
  if (!selected?.modelId) {
    if (!isResume) {
      tab.messages.push({ id: uid("msg"), role: "user", text: userText });
    }
    tab.messages.push({
      id: uid("msg"),
      role: "system",
      text: "Pick a chat model in the dropdown at the bottom of this panel first.",
    });
    renderChat();
    return;
  }

  if (!isResume) {
    tab.messages.push({ id: uid("msg"), role: "user", text: userText });
  }
  tab.busy = true;
  chatSendInFlight = true;
  if (!isResume) {
    directorProgressMessageId = null;
  }
  // Apply chrome immediately (before full rebuild) so Send never stays "Send".
  setChatWorkingChrome(true);
  renderChat();
  setStatus(
    isResume ? "Director resuming…" : "Director working…",
    selected.modelId,
  );
  upsertDirectorProgressMessage(
    isResume
      ? "Director resuming the same turn…"
      : "Director working…",
  );
  await paintSoon();

  try {
    const result = await invoke<{
      reply: string;
      model: string;
      toolEvents: string[];
      continueSuggested?: boolean;
      plannerPaused?: boolean;
      stopped?: boolean;
    }>("director_turn", {
      request: {
        model: selected.modelId,
        apiId: selected.apiId,
        messages: directorHistory(tab),
        resume: isResume,
        context: currentDirectorContext(),
      },
    });

    if (directorProgressMessageId) {
      tab.messages = tab.messages.filter(
        (message) => message.id !== directorProgressMessageId,
      );
      clearDirectorProgressMessage();
    }

    const halted = directorStopRequested || Boolean(result.stopped);

    // Planner yielded so the desk could paint — resume the same host loop.
    // Do not add a pause bubble or a fake "continue" user turn.
    if (!halted && result.plannerPaused && autoContinueDepth < MAX_AUTO_CONTINUE) {
      if (projectRoot) {
        try {
          await refreshWorkflowDesk();
        } catch {
          /* keep the turn going if the desk cannot refresh yet */
        }
      }
      autoContinueDepth += 1;
      upsertDirectorProgressMessage(
        "Director still working — resuming the same turn…",
      );
      tab.busy = false;
      chatSendInFlight = false;
      renderChat();
      await paintSoon();
      await sendChatMessage("", { autoContinue: true, resume: true });
      return;
    }

    tab.messages.push({
      id: uid("msg"),
      role: halted ? "system" : "assistant",
      text: halted
        ? result.stopped
          ? result.reply
          : "Stopped. The Director will not continue this turn."
        : result.reply,
    });
    if (halted) {
      setStatus("Director stopped");
    } else if (result.toolEvents.length > 0) {
      setStatus(
        "Director reply",
        `${result.model} · ${result.toolEvents.length} tool call(s)`,
      );
    } else {
      setStatus("Director reply", result.model);
    }
    if (projectRoot) {
      await renderExplorer();
      try {
        await refreshWorkflowDesk();
      } catch {
        /* desk refresh is best-effort after chat */
      }
    }

    // Specialist hop (model sheet / collect) with a pending action in the reply.
    const awaitingApproval = /\[\[workbench_approve:[A-Za-z0-9]+\]\]/.test(
      result.reply,
    );
    if (
      !halted &&
      result.continueSuggested &&
      !awaitingApproval &&
      autoContinueDepth < MAX_AUTO_CONTINUE
    ) {
      autoContinueDepth += 1;
      tab.busy = false;
      chatSendInFlight = false;
      renderChat();
      await paintSoon();
      await sendChatMessage("continue", { autoContinue: true });
      return;
    }
  } catch (error) {
    if (directorProgressMessageId) {
      tab.messages = tab.messages.filter(
        (message) => message.id !== directorProgressMessageId,
      );
      clearDirectorProgressMessage();
    }
    if (directorStopRequested || looksLikeStoppedError(error)) {
      tab.messages.push({
        id: uid("msg"),
        role: "system",
        text: "Stopped. The Director will not continue this turn.",
      });
      setStatus("Director stopped");
    } else {
      tab.messages.push({
        id: uid("msg"),
        role: "system",
        text: `Director turn failed: ${error}`,
      });
      setStatus(`Director error: ${error}`);
    }
  } finally {
    tab.busy = false;
    chatSendInFlight = false;
    directorStopping = false;
    renderChat();
  }
}

function undoEdit(): void {
  const tab = activeEditor();
  if (!isTextTab(tab) || tab.preview) return;
  syncTabFromEditor();
  if (tab.undoStack.length === 0) {
    setStatus("Nothing to undo", tab.path ?? tab.name);
    return;
  }
  applyingHistory = true;
  tab.redoStack.push(tab.contents);
  const previous = tab.undoStack.pop()!;
  tab.contents = previous;
  tab.dirty = tab.contents !== tab.original;
  setEditorValue(previous);
  applyingHistory = false;
  renderEditorTabs();
  setStatus(tab.dirty ? "Unsaved changes" : "Ready", tab.path ?? tab.name);
}

function redoEdit(): void {
  const tab = activeEditor();
  if (!isTextTab(tab) || tab.preview) return;
  syncTabFromEditor();
  if (tab.redoStack.length === 0) {
    setStatus("Nothing to redo", tab.path ?? tab.name);
    return;
  }
  applyingHistory = true;
  tab.undoStack.push(tab.contents);
  const next = tab.redoStack.pop()!;
  tab.contents = next;
  tab.dirty = tab.contents !== tab.original;
  setEditorValue(next);
  applyingHistory = false;
  renderEditorTabs();
  setStatus(tab.dirty ? "Unsaved changes" : "Ready", tab.path ?? tab.name);
}

async function copySelection(): Promise<void> {
  const target = document.activeElement;
  if (target === els.chatInput || target === els.findInput || target === els.replaceInput) {
    const input = target as HTMLInputElement | HTMLTextAreaElement;
    const selected = input.value.slice(input.selectionStart ?? 0, input.selectionEnd ?? 0);
    if (selected) {
      await navigator.clipboard.writeText(selected);
    }
    return;
  }

  const tab = activeEditor();
  if (!isTextTab(tab) || tab.preview) return;
  focusEditor();
  const selected = els.editor.value.slice(
    els.editor.selectionStart,
    els.editor.selectionEnd,
  );
  if (!selected) return;
  await navigator.clipboard.writeText(selected);
}

async function cutSelection(): Promise<void> {
  const active = document.activeElement;
  if (
    active === els.chatInput ||
    active === els.findInput ||
    active === els.replaceInput
  ) {
    const input = active as HTMLInputElement | HTMLTextAreaElement;
    const start = input.selectionStart ?? 0;
    const end = input.selectionEnd ?? 0;
    if (start === end) return;
    const selected = input.value.slice(start, end);
    await navigator.clipboard.writeText(selected);
    input.setRangeText("", start, end, "end");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return;
  }

  const tab = activeEditor();
  if (!isTextTab(tab) || tab.preview) return;
  focusEditor();
  const start = els.editor.selectionStart;
  const end = els.editor.selectionEnd;
  if (start === end) return;
  const selected = els.editor.value.slice(start, end);
  await navigator.clipboard.writeText(selected);
  pushUndoSnapshot(tab, els.editor.value);
  const next = els.editor.value.slice(0, start) + els.editor.value.slice(end);
  applyingHistory = true;
  setEditorValue(next, start, start);
  applyingHistory = false;
  tab.contents = next;
  tab.dirty = tab.contents !== tab.original;
  tab.selectionStart = start;
  tab.selectionEnd = start;
  renderEditorTabs();
  setStatus(tab.dirty ? "Unsaved changes" : "Ready", tab.path ?? tab.name);
}

async function pasteClipboard(): Promise<void> {
  const active = document.activeElement;
  if (
    active === els.chatInput ||
    active === els.findInput ||
    active === els.replaceInput
  ) {
    const input = active as HTMLInputElement | HTMLTextAreaElement;
    let text = "";
    try {
      text = await navigator.clipboard.readText();
    } catch {
      setStatus("Clipboard paste blocked");
      return;
    }
    const start = input.selectionStart ?? input.value.length;
    const end = input.selectionEnd ?? input.value.length;
    input.setRangeText(text, start, end, "end");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return;
  }

  const tab = activeEditor();
  if (!isTextTab(tab) || tab.preview) return;
  focusEditor();
  let text = "";
  try {
    text = await navigator.clipboard.readText();
  } catch {
    setStatus("Clipboard paste blocked — try focusing the editor and using Ctrl+V");
    return;
  }
  const start = els.editor.selectionStart;
  const end = els.editor.selectionEnd;
  pushUndoSnapshot(tab, els.editor.value);
  const next =
    els.editor.value.slice(0, start) + text + els.editor.value.slice(end);
  const caret = start + text.length;
  applyingHistory = true;
  setEditorValue(next, caret, caret);
  applyingHistory = false;
  tab.contents = next;
  tab.dirty = tab.contents !== tab.original;
  tab.selectionStart = caret;
  tab.selectionEnd = caret;
  renderEditorTabs();
  setStatus(tab.dirty ? "Unsaved changes" : "Ready", tab.path ?? tab.name);
}

function selectAllInEditor(): void {
  const active = document.activeElement;
  if (
    active === els.chatInput ||
    active === els.findInput ||
    active === els.replaceInput
  ) {
    (active as HTMLInputElement | HTMLTextAreaElement).select();
    return;
  }
  const tab = activeEditor();
  if (!isTextTab(tab) || tab.preview) return;
  focusEditor();
  els.editor.select();
  tab.selectionStart = 0;
  tab.selectionEnd = els.editor.value.length;
}

function collectMatchIndexes(text: string, query: string): number[] {
  if (!query) return [];
  const indexes: number[] = [];
  const haystack = text.toLowerCase();
  const needle = query.toLowerCase();
  let from = 0;
  while (from <= haystack.length) {
    const found = haystack.indexOf(needle, from);
    if (found === -1) break;
    indexes.push(found);
    from = found + Math.max(needle.length, 1);
  }
  return indexes;
}

function updateFindCount(): void {
  const query = els.findInput.value;
  const tab = activeEditor();
  if (!tab || !query) {
    els.findCount.textContent = "";
    return;
  }
  const matches = collectMatchIndexes(tab.contents, query);
  if (matches.length === 0) {
    els.findCount.textContent = "No results";
    return;
  }
  const labelIndex = findMatchIndex >= 0 ? findMatchIndex + 1 : 1;
  els.findCount.textContent = `${labelIndex} of ${matches.length}`;
}

function selectFindMatch(direction: 1 | -1): void {
  const tab = activeEditor();
  if (!tab || tab.preview) return;
  const query = els.findInput.value;
  if (!query) {
    els.findCount.textContent = "";
    return;
  }
  const matches = collectMatchIndexes(tab.contents, query);
  if (matches.length === 0) {
    findMatchIndex = -1;
    els.findCount.textContent = "No results";
    return;
  }

  if (findMatchIndex === -1) {
    const caret = els.editor.selectionStart;
    if (direction === 1) {
      findMatchIndex = matches.findIndex((index) => index >= caret);
      if (findMatchIndex === -1) findMatchIndex = 0;
    } else {
      findMatchIndex = -1;
      for (let i = matches.length - 1; i >= 0; i -= 1) {
        if (matches[i] < caret) {
          findMatchIndex = i;
          break;
        }
      }
      if (findMatchIndex === -1) findMatchIndex = matches.length - 1;
    }
  } else {
    findMatchIndex = (findMatchIndex + direction + matches.length) % matches.length;
  }

  const start = matches[findMatchIndex];
  const end = start + query.length;
  focusEditor();
  els.editor.setSelectionRange(start, end);
  tab.selectionStart = start;
  tab.selectionEnd = end;
  els.findCount.textContent = `${findMatchIndex + 1} of ${matches.length}`;
}

function openFindBar(withReplace: boolean): void {
  const tab = activeEditor();
  if (!isTextTab(tab)) {
    setStatus("Find is available in text tabs");
    return;
  }
  if (tab.preview) {
    tab.preview = false;
    renderEditor();
  }
  els.findBar.classList.remove("hidden");
  els.replaceField.classList.toggle("hidden", !withReplace);
  els.replaceActions.classList.toggle("hidden", !withReplace);
  const selected = els.editor.value.slice(
    els.editor.selectionStart,
    els.editor.selectionEnd,
  );
  if (selected && !selected.includes("\n")) {
    els.findInput.value = selected;
  }
  els.findInput.focus();
  els.findInput.select();
  findMatchIndex = -1;
  updateFindCount();
}

function closeFindBar(): void {
  els.findBar.classList.add("hidden");
  findMatchIndex = -1;
  els.findCount.textContent = "";
  focusEditor();
}

function replaceOne(): void {
  const tab = activeEditor();
  if (!tab || tab.preview) return;
  const query = els.findInput.value;
  if (!query) return;

  const start = els.editor.selectionStart;
  const end = els.editor.selectionEnd;
  const selected = els.editor.value.slice(start, end);
  const matchesSelection =
    selected.length > 0 && selected.toLowerCase() === query.toLowerCase();

  if (!matchesSelection) {
    selectFindMatch(1);
    return;
  }

  const replacement = els.replaceInput.value;
  pushUndoSnapshot(tab, els.editor.value);
  const next =
    els.editor.value.slice(0, start) + replacement + els.editor.value.slice(end);
  const caret = start + replacement.length;
  applyingHistory = true;
  setEditorValue(next, caret, caret);
  applyingHistory = false;
  tab.contents = next;
  tab.dirty = tab.contents !== tab.original;
  tab.selectionStart = caret;
  tab.selectionEnd = caret;
  findMatchIndex = -1;
  renderEditorTabs();
  selectFindMatch(1);
  setStatus(tab.dirty ? "Unsaved changes" : "Ready", tab.path ?? tab.name);
}

function replaceAll(): void {
  const tab = activeEditor();
  if (!tab || tab.preview) return;
  const query = els.findInput.value;
  if (!query) return;

  const matches = collectMatchIndexes(tab.contents, query);
  if (matches.length === 0) {
    els.findCount.textContent = "No results";
    return;
  }

  const replacement = els.replaceInput.value;
  pushUndoSnapshot(tab, tab.contents);
  let next = "";
  let cursor = 0;
  for (const index of matches) {
    next += tab.contents.slice(cursor, index) + replacement;
    cursor = index + query.length;
  }
  next += tab.contents.slice(cursor);
  applyingHistory = true;
  setEditorValue(next);
  applyingHistory = false;
  tab.contents = next;
  tab.dirty = tab.contents !== tab.original;
  tab.selectionStart = 0;
  tab.selectionEnd = 0;
  findMatchIndex = -1;
  renderEditorTabs();
  updateFindCount();
  setStatus(`Replaced ${matches.length} match(es)`, tab.path ?? tab.name);
}

function updatePlayPauseButton(): void {
  const media = activeMediaElement();
  const playing = Boolean(media && !media.paused && !media.ended);
  els.mediaPlayPause.textContent = playing ? "Pause" : "Play";
}

function togglePlayPause(): void {
  const media = activeMediaElement();
  if (!media) return;
  if (media.paused || media.ended) {
    const atEnd =
      media.ended ||
      (Number.isFinite(media.duration) &&
        media.duration > 0 &&
        media.currentTime >= media.duration - 0.05);
    if (atEnd) {
      try {
        media.currentTime = 0;
      } catch {
        /* seek may fail until metadata; play() still retries from start when possible */
      }
    }
    void media
      .play()
      .then(() => updatePlayPauseButton())
      .catch((error: unknown) => {
        // Common after ended when seek/range was broken: force reload then play.
        if (atEnd && media.src) {
          const src = media.src;
          const path = media.dataset.path || "";
          attachMediaSource(media, src, path || src);
          const start = () => {
            void media.play().catch((retryError: unknown) => {
              setStatus(`Playback failed: ${retryError}`);
              updatePlayPauseButton();
            });
          };
          if (media.readyState >= 2) start();
          else media.addEventListener("loadeddata", start, { once: true });
          return;
        }
        setStatus(`Playback failed: ${error}`);
        updatePlayPauseButton();
      });
  } else {
    media.pause();
  }
  updatePlayPauseButton();
}

function toggleMarkdownPreview(): void {
  const tab = activeEditor();
  if (!isTextTab(tab)) {
    setStatus("Open a markdown file to preview");
    return;
  }
  if (!tabIsMarkdown(tab)) {
    setStatus("Preview is available for .md files", tab.path ?? tab.name);
    return;
  }
  syncTabFromEditor();
  tab.preview = !tab.preview;
  renderEditor();
  if (!tab.preview) {
    focusEditor();
  }
}

function bindSplitters(): void {
  const splitters = document.querySelectorAll<HTMLElement>(".splitter");
  for (const splitter of splitters) {
    let dragging = false;
    let startX = 0;
    let startWidth = 0;

    const onMove = (event: MouseEvent) => {
      if (!dragging) return;
      const delta = event.clientX - startX;
      if (splitter.dataset.splitter === "explorer") {
        const width = Math.min(480, Math.max(180, startWidth + delta));
        document.documentElement.style.setProperty(
          "--explorer-width",
          `${width}px`,
        );
      } else {
        const width = Math.min(560, Math.max(220, startWidth - delta));
        document.documentElement.style.setProperty("--chat-width", `${width}px`);
      }
    };

    const onUp = () => {
      dragging = false;
      splitter.classList.remove("active");
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    splitter.addEventListener("mousedown", (event) => {
      dragging = true;
      startX = event.clientX;
      startWidth =
        splitter.dataset.splitter === "explorer"
          ? els.explorerPanel.getBoundingClientRect().width
          : els.chatPanel.getBoundingClientRect().width;
      splitter.classList.add("active");
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
      event.preventDefault();
    });
  }
}

let lastAction = "";
let lastActionAt = 0;

function handleMenuAction(action: string): void {
  const now = Date.now();
  if (action === lastAction && now - lastActionAt < 80) {
    return;
  }
  lastAction = action;
  lastActionAt = now;

  const recentPath = decodeOpenRecentMenuAction(action);
  if (recentPath) {
    void openProjectAt(recentPath);
    return;
  }

  switch (action) {
    case "open-project":
      void openProject();
      break;
    case "close-project":
      void closeProject();
      break;
    case "save":
      void saveActiveFile();
      break;
    case "save-as":
      void saveActiveFileAs();
      break;
    case "refresh-explorer":
      void renderExplorer();
      break;
    case "undo":
      undoEdit();
      break;
    case "redo":
      redoEdit();
      break;
    case "cut":
      if (explorerPanelFocused() || (!isTypingTarget(document.activeElement) && selectedPath)) {
        void cutExplorerEntry();
      } else {
        void cutSelection();
      }
      break;
    case "copy":
      if (explorerPanelFocused()) {
        void copyExplorerEntry();
      } else {
        void copySelection();
      }
      break;
    case "paste":
      if (explorerPanelFocused() || (explorerClipboard && !isTypingTarget(document.activeElement))) {
        void pasteExplorerClipboard();
      } else {
        void pasteClipboard();
      }
      break;
    case "select-all":
      selectAllInEditor();
      break;
    case "find":
      openFindBar(false);
      break;
    case "replace":
      openFindBar(true);
      break;
    case "toggle-markdown-preview":
      toggleMarkdownPreview();
      break;
    case "open-settings":
      showShell("settings");
      break;
    case "open-voice":
      showShell("voices");
      break;
    case "open-workflow":
      showShell("studio");
      break;
    case "toggle-chat":
      toggleChatPanel();
      break;
    case "open-docs":
      openDocsTab();
      break;
    case "copy-chat":
      void copyChatTranscript();
      break;
    case "save-chat":
      void saveChatTranscript();
      break;
    default:
      break;
  }
}

function bindUi(): void {
  for (const button of els.navRail.querySelectorAll<HTMLButtonElement>(".nav-item")) {
    button.addEventListener("click", () => {
      const surface = button.dataset.surface;
      if (surface === "home" || surface === "studio" || surface === "files" || surface === "assets" || surface === "voices") {
        showShell(surface);
      }
    });
  }
  els.navRail.addEventListener("keydown", (event) => {
    const items = [...els.navRail.querySelectorAll<HTMLButtonElement>(".nav-item")];
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    if (index < 0) return;
    const next =
      event.key === "ArrowDown" || event.key === "ArrowRight"
        ? items[(index + 1) % items.length]
        : event.key === "ArrowUp" || event.key === "ArrowLeft"
          ? items[(index - 1 + items.length) % items.length]
          : event.key === "Home"
            ? items[0]
            : event.key === "End"
              ? items[items.length - 1]
              : null;
    if (!next) return;
    event.preventDefault();
    next.focus();
  });
  els.jobsBtn.addEventListener("click", () => {
    openJobsDrawer();
    if (shellSurface !== "studio") showShell("studio");
  });
  els.shellWorkArea.addEventListener("change", () => {
    const value = els.shellWorkArea.value;
    if (value === "plan" || value === "produce" || value === "finish") {
      setStudioWorkArea(value);
      if (shellSurface !== "studio") showShell("studio");
    }
  });
  els.settingsBtn.addEventListener("click", () => showShell("settings"));
  document.querySelector("#home-open-project")?.addEventListener("click", () => {
    void openProject();
  });
  document.querySelector("#home-studio")?.addEventListener("click", () => showShell("studio"));
  document.querySelector("#home-files")?.addEventListener("click", () => showShell("files"));

  els.mediaPlayPause.addEventListener("click", () => togglePlayPause());

  for (const button of document.querySelectorAll<HTMLButtonElement>(".rate-btn")) {
    button.addEventListener("click", () => {
      const rate = Number(button.dataset.rate);
      if (PLAYBACK_RATES.includes(rate as PlaybackRate)) {
        applyPlaybackRate(rate as PlaybackRate);
      }
    });
  }

  els.mediaSeek.addEventListener("pointerdown", () => {
    mediaSeekDragging = true;
  });
  els.mediaSeek.addEventListener("pointerup", () => {
    mediaSeekDragging = false;
    seekActiveMedia(Number(els.mediaSeek.value));
  });
  els.mediaSeek.addEventListener("input", () => {
    mediaSeekDragging = true;
    const media = activeMediaElement();
    const duration = media && Number.isFinite(media.duration) ? media.duration : 0;
    els.mediaTime.textContent = `${formatTime(Number(els.mediaSeek.value))} / ${formatTime(duration)}`;
  });
  els.mediaSeek.addEventListener("change", () => {
    mediaSeekDragging = false;
    seekActiveMedia(Number(els.mediaSeek.value));
  });

  els.mediaVolume.addEventListener("input", () => {
    applyMediaVolume(Number(els.mediaVolume.value));
  });

  const onMediaTime = () => updateMediaTimeDisplay();
  const onMediaError = (event: Event) => {
    const media = event.currentTarget as HTMLMediaElement;
    const message = media.error?.message || "unknown media error";
    setStatus(`Media error: ${message}`);
  };
  els.mediaVideo.addEventListener("timeupdate", onMediaTime);
  els.mediaVideo.addEventListener("loadedmetadata", onMediaTime);
  els.mediaVideo.addEventListener("durationchange", onMediaTime);
  els.mediaVideo.addEventListener("play", updatePlayPauseButton);
  els.mediaVideo.addEventListener("pause", updatePlayPauseButton);
  els.mediaVideo.addEventListener("ended", updatePlayPauseButton);
  els.mediaVideo.addEventListener("error", onMediaError);
  els.mediaAudio.addEventListener("timeupdate", onMediaTime);
  els.mediaAudio.addEventListener("loadedmetadata", onMediaTime);
  els.mediaAudio.addEventListener("durationchange", onMediaTime);
  els.mediaAudio.addEventListener("play", updatePlayPauseButton);
  els.mediaAudio.addEventListener("pause", updatePlayPauseButton);
  els.mediaAudio.addEventListener("ended", updatePlayPauseButton);
  els.mediaAudio.addEventListener("error", onMediaError);
  applyMediaVolume(mediaVolume);
  updatePlayPauseButton();

  els.editor.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    showEditorTextContextMenu(event.clientX, event.clientY);
  });

  els.editor.addEventListener("input", () => {
    const tab = activeEditor();
    if (!isTextTab(tab) || applyingHistory) return;
    pushUndoSnapshot(tab, lastEditorSnapshot);
    tab.contents = els.editor.value;
    tab.dirty = tab.contents !== tab.original;
    tab.selectionStart = els.editor.selectionStart;
    tab.selectionEnd = els.editor.selectionEnd;
    lastEditorSnapshot = els.editor.value;
    findMatchIndex = -1;
    renderEditorTabs();
    if (!els.findBar.classList.contains("hidden")) {
      updateFindCount();
    }
    setStatus(tab.dirty ? "Unsaved changes" : "Ready", tab.path ?? tab.name);
  });

  els.editor.addEventListener("select", () => {
    const tab = activeEditor();
    if (!tab) return;
    tab.selectionStart = els.editor.selectionStart;
    tab.selectionEnd = els.editor.selectionEnd;
  });

  els.chatForm.addEventListener("submit", (event) => {
    event.preventDefault();
    if (chatSendInFlight) return;
    const text = els.chatInput.value;
    void sendChatMessage(text);
  });

  els.chatStopBtn.addEventListener("click", () => {
    void stopDirector();
  });

  els.chatInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      if (chatSendInFlight) return;
      els.chatForm.requestSubmit();
    }
  });

  els.newChatTab.addEventListener("click", () => {
    createChatTab();
    renderChat();
  });

  els.copyChatBtn.addEventListener("click", () => {
    void copyChatTranscript();
  });
  els.saveChatBtn.addEventListener("click", () => {
    void saveChatTranscript();
  });
  els.toggleChatBtn.addEventListener("click", () => toggleChatPanel());
  els.chatRail.addEventListener("click", () => setChatCollapsed(false));
  els.chatPanel.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    showChatContextMenu(event.clientX, event.clientY);
  });

  const setChatDragOver = (active: boolean) => {
    els.chatPanel.classList.toggle("drag-over", active);
  };

  els.chatPanel.addEventListener("dragenter", (event) => {
    event.preventDefault();
    setChatDragOver(true);
  });
  els.chatPanel.addEventListener("dragover", (event) => {
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
    setChatDragOver(true);
  });
  els.chatPanel.addEventListener("dragleave", (event) => {
    const related = event.relatedTarget as Node | null;
    if (related && els.chatPanel.contains(related)) return;
    setChatDragOver(false);
  });
  els.chatPanel.addEventListener("drop", (event) => {
    event.preventDefault();
    setChatDragOver(false);
    const transfer = event.dataTransfer;
    if (!transfer) return;
    const workbenchPath =
      transfer.getData(WORKBENCH_PATH_MIME) || transfer.getData("text/plain");
    if (workbenchPath && workbenchPath.startsWith("/")) {
      addPathToChat(workbenchPath.trim());
      return;
    }
    // Fallback: some environments expose a path on dropped File objects.
    for (const file of Array.from(transfer.files)) {
      const maybePath = (file as File & { path?: string }).path;
      if (maybePath) addPathToChat(maybePath);
    }
  });

  els.chatModelSelect.addEventListener("change", () => {
    setSelectedChatModelFromSelect(els.chatModelSelect.value);
    void persistSettings()
      .then(() => setStatus("Chat model updated"))
      .catch((error: unknown) => setStatus(`Could not save chat model: ${error}`));
  });

  els.newFileBtn.addEventListener("click", () => {
    if (!projectRoot) return;
    void createEntryAt(projectRoot, "file");
  });

  els.newFolderBtn.addEventListener("click", () => {
    if (!projectRoot) return;
    void createEntryAt(projectRoot, "folder");
  });

  els.saveBtn.addEventListener("click", () => {
    void saveActiveFile();
  });
  els.saveAsBtn.addEventListener("click", () => {
    void saveActiveFileAs();
  });
  els.renameFileBtn.addEventListener("click", () => {
    // Prefer explorer selection over the active editor tab.
    void beginRename(pathForFileOps());
  });
  els.deleteFileBtn.addEventListener("click", () => {
    void deleteEntry(pathForFileOps());
  });
  els.togglePreviewBtn.addEventListener("click", () => {
    toggleMarkdownPreview();
  });

  // Capture-phase: always suppress Chromium's default menu in the desk UI.
  // Specific handlers below populate our custom menu.
  document.addEventListener(
    "contextmenu",
    (event) => {
      event.preventDefault();
    },
    true,
  );

  els.fileTree.addEventListener("contextmenu", (event) => {
    if (!projectRoot) return;
    event.preventDefault();
    event.stopPropagation();
    showExplorerContextMenu(event.clientX, event.clientY, null, projectRoot);
  });

  // Drop onto empty explorer space → project root.
  els.fileTree.addEventListener("dragover", (event) => {
    if (!projectRoot) return;
    const types = event.dataTransfer?.types;
    if (!types || !Array.from(types).includes(WORKBENCH_PATH_MIME)) return;
    event.preventDefault();
    const copy = event.ctrlKey || event.metaKey || event.altKey;
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = copy ? "copy" : "move";
    }
  });
  els.fileTree.addEventListener("drop", (event) => {
    if (!projectRoot) return;
    const types = event.dataTransfer?.types;
    if (!types || !Array.from(types).includes(WORKBENCH_PATH_MIME)) return;
    event.preventDefault();
    const source =
      event.dataTransfer?.getData(WORKBENCH_PATH_MIME) ||
      event.dataTransfer?.getData("text/plain") ||
      "";
    if (!source.startsWith("/")) return;
    const copy = event.ctrlKey || event.metaKey || event.altKey;
    void transferExplorerPaths([source.trim()], projectRoot, copy ? "copy" : "cut");
  });

  els.editorTabs.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    event.stopPropagation();
    showEditorTabContextMenu(event.clientX, event.clientY);
  });

  // Left-button pointerdown dismisses; ignore the right-click that opened the menu.
  window.addEventListener(
    "pointerdown",
    (event) => {
      if (event.button !== 0) return;
      dismissContextMenu(event);
    },
    true,
  );
  window.addEventListener("resize", () => hideContextMenu());
  window.addEventListener("blur", () => dismissContextMenu());

  els.findPrev.addEventListener("click", () => selectFindMatch(-1));
  els.findNext.addEventListener("click", () => selectFindMatch(1));
  els.replaceOne.addEventListener("click", () => replaceOne());
  els.replaceAll.addEventListener("click", () => replaceAll());
  els.findClose.addEventListener("click", () => closeFindBar());

  els.findInput.addEventListener("input", () => {
    findMatchIndex = -1;
    updateFindCount();
  });

  els.findInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      selectFindMatch(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeFindBar();
    }
  });

  els.replaceInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      replaceOne();
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeFindBar();
    }
  });

  window.addEventListener("keydown", (event) => {
    const mod = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();

    if (event.key === "Escape") {
      if (!els.contextMenu.classList.contains("hidden")) {
        event.preventDefault();
        hideContextMenu();
        return;
      }
      if (pendingRename) {
        event.preventDefault();
        cancelPendingRename();
        return;
      }
      if (pendingCreate) {
        event.preventDefault();
        cancelPendingCreate();
        return;
      }
      if (!els.findBar.classList.contains("hidden")) {
        event.preventDefault();
        closeFindBar();
        return;
      }
      if (chatSendInFlight || activeChat()?.busy) {
        event.preventDefault();
        void stopDirector();
        return;
      }
    }

    if (!mod && !event.altKey && event.key === "F2") {
      const target = event.target;
      const inNameField =
        target instanceof HTMLElement &&
        (target.classList.contains("tree-name-input") ||
          Boolean(target.closest(".tree-name-input")));
      const inChatOrFind =
        target === els.chatInput ||
        target === els.findInput ||
        target === els.replaceInput;
      if (!inNameField && !inChatOrFind) {
        event.preventDefault();
        // Prefer explorer selection; fall back to the active editor file.
        void beginRename();
        return;
      }
    }

    if (
      !mod &&
      !event.altKey &&
      (event.key === "Delete" || event.key === "Del") &&
      !isTypingTarget(event.target)
    ) {
      event.preventDefault();
      void deleteEntry();
      return;
    }

    if (
      !mod &&
      !isTypingTarget(event.target) &&
      isAvTab(activeEditor()) &&
      (event.key === "<" || event.key === ">" || event.code === "Comma" || event.code === "Period")
    ) {
      if (event.key === "<" || (event.code === "Comma" && event.shiftKey)) {
        event.preventDefault();
        bumpPlaybackRate(-1);
        return;
      }
      if (event.key === ">" || (event.code === "Period" && event.shiftKey)) {
        event.preventDefault();
        bumpPlaybackRate(1);
        return;
      }
    }

    if (event.key === "F1") {
      event.preventDefault();
      handleMenuAction("open-docs");
      return;
    }

    if (!mod) return;

    if (key === "c" && event.altKey && event.shiftKey) {
      event.preventDefault();
      void copyPath(false);
      return;
    }
    if (key === "c" && event.shiftKey && !event.altKey) {
      event.preventDefault();
      handleMenuAction("copy-chat");
      return;
    }
    if (key === "c" && event.altKey) {
      event.preventDefault();
      void copyPath(true);
      return;
    }
    if (key === "e" && event.shiftKey && !event.altKey) {
      event.preventDefault();
      handleMenuAction("save-chat");
      return;
    }

    if (key === "o") {
      event.preventDefault();
      handleMenuAction("open-project");
      return;
    }
    if (key === "s" && event.shiftKey) {
      event.preventDefault();
      handleMenuAction("save-as");
      return;
    }
    if (key === "s") {
      event.preventDefault();
      handleMenuAction("save");
      return;
    }
    if (key === "f") {
      event.preventDefault();
      handleMenuAction("find");
      return;
    }
    if (key === "h" && event.altKey) {
      event.preventDefault();
      handleMenuAction("toggle-chat");
      return;
    }
    if (key === "h") {
      event.preventDefault();
      handleMenuAction("replace");
      return;
    }
    if (key === "p" && event.altKey) {
      event.preventDefault();
      handleMenuAction("open-workflow");
      return;
    }
    if (key === "p" && event.shiftKey) {
      event.preventDefault();
      handleMenuAction("toggle-markdown-preview");
      return;
    }
    if (key === "," && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      handleMenuAction("open-settings");
      return;
    }
    if (key === "r" && !event.shiftKey && !event.altKey) {
      event.preventDefault();
      handleMenuAction("refresh-explorer");
    }
    // Clipboard / undo / select-all: Chromium Edit menu roles handle these.
  });
}

async function bindMenuAndProgress(): Promise<void> {
  await bindHostEvents({
    onMenu: (id) => handleMenuAction(id),
    onDirectorProgress: (message) => {
      const text = (message || "").trim();
      if (!text) return;
      setStatus(text);
      if (chatSendInFlight) {
        upsertDirectorProgressMessage(text);
      }
    },
  });
}

function bindPreviewLinkGuard(): void {
  const onClick = (event: MouseEvent) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const anchor = target.closest("a");
    if (!(anchor instanceof HTMLAnchorElement)) return;
    const host = anchor.closest("#markdown-preview, #docs-view");
    if (!host) return;
    const href = anchor.getAttribute("href") || "";
    if (href.startsWith("#") && !href.startsWith("#/")) {
      event.preventDefault();
      const id = decodeURIComponent(href.slice(1));
      host.querySelector(`#${CSS.escape(id)}`)?.scrollIntoView();
      return;
    }
    event.preventDefault();
    void openPreviewLink(href);
  };
  document.addEventListener("click", onClick);
  document.addEventListener("auxclick", onClick);
}

async function openPreviewLink(href: string): Promise<void> {
  const openExternal = window.workbench?.openExternal;
  if (!openExternal) {
    setStatus("That link stays in the app");
    return;
  }
  try {
    const opened = await openExternal(href);
    setStatus(opened ? "Opened the link in your browser" : "Link was not opened");
  } catch (error) {
    setStatus(`Could not open link: ${error}`);
  }
}

async function boot(): Promise<void> {
  bindPreviewLinkGuard();
  bindSettingsHost({
    root: els.settingsView,
    onStatus: setStatus,
    onSettingsChanged: () => {
      refreshChatModelSelect();
      syncSettingsTabDirty();
    },
    onDraftChanged: () => {
      refreshChatModelSelect();
      syncSettingsTabDirty();
    },
  });
  bindVoiceHost({
    root: els.voiceView,
    projectRoot: () => projectRoot,
    selectedPath: () => selectedPath,
    onStatus: setStatus,
    onSaved: () => {
      void renderExplorer();
      void refreshWorkflowDesk();
    },
  });
  bindWorkflowDesk({
    root: els.workflowView,
    projectRoot: () => projectRoot,
    onOpenPath: (path) => {
      void openFile(path);
    },
    onAddToChat: (path) => addPathToChat(path),
    onOpenVoice: (characterDirectory) => {
      selectedPath = characterDirectory;
      showShell("voices");
    },
    onComposeChat: (text, attachment) => {
      if (!activeChat()) createChatTab("Director");
      if (attachment) addPathToChat(attachment);
      setChatCollapsed(false);
      renderChat();
      els.chatInput.value = text;
      els.chatInput.focus();
    },
    onSelectionChange: (selection) => {
      if (selection.path) selectedPath = selection.path;
      updateShellChrome();
      renderDirectorContext();
    },
    chatModel: () => getSettings().selectedChatModel,
    onStatus: setStatus,
    onPendingPlan: (text) => {
      const tab = activeChat() ?? createChatTab("Director");
      tab.messages.push({
        id: uid("msg"),
        role: "assistant",
        text,
      });
      setChatCollapsed(false);
      renderChat();
      setStatus("Generation ready", "Approve in chat to spend");
    },
  });
  try {
    await loadAppSettings();
  } catch (error) {
    setStatus(`Settings load failed: ${error}`);
  }
  refreshChatModelSelect();
  createChatTab("Director");
  bindSplitters();
  bindUi();
  updateExplorerActionState();
  renderChat();
  renderHome();
  paintSurface();
  renderEditor();
  await renderExplorer();
  try {
    await bindMenuAndProgress();
  } catch (error) {
    setStatus(`Host events unavailable: ${error}`);
    return;
  }
  setStatus("Ready — open a project to begin");
}

void boot();
