import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => { desk.clearWorkflowDesk(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.body.innerHTML = ""; });

it("Files deletion refreshes Studio after the file moves to trash", async () => {
  vi.useFakeTimers();
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = "/fixture/project", path = `${root}/Scripts/story.md`;
  const initial = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  const removed = structuredClone(initial);
  removed.state.scripts = [];
  for (const scene of removed.state.scenes) scene.source_missing = true;
  removed.report.scenes = [];
  let deleted = false;
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "list_dir") return [];
    if (cmd === "workflow_import") return structuredClone(deleted ? removed : initial);
    if (cmd === "classify_file") return "text";
    if (cmd === "read_text_file") return "Scene 1 — Story";
    if (cmd === "delete_path") { deleted = true; return null; }
    throw new Error(`Unexpected ${cmd}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => root, openDialog: async () => path,
    ask: async () => true, rememberRecentProject: async () => {}, onMenu: () => () => {},
    onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  const flush = () => vi.advanceTimersByTimeAsync(150);
  await flush();
  document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await flush();
  document.querySelector<HTMLButtonElement>("#open-file-btn")!.click(); await flush();
  expect(desk.workflowSelection().sceneId).not.toBeNull();
  document.querySelector<HTMLButtonElement>("#delete-file-btn")!.click(); await flush();
  expect(invoke.mock.calls.some(([cmd]) => cmd === "delete_path")).toBe(true);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import").length).toBeGreaterThan(1);
  expect(desk.workflowSelection().sceneId).toBeNull();
});
