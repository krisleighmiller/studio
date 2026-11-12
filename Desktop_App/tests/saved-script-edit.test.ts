import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import type { WorkflowImportResult } from "../src/workflowTypes";
vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
const native = JSON.parse(readFileSync("tests/fixtures/fx02-import-result.json", "utf8")) as WorkflowImportResult;
afterEach(() => {
  desk.clearWorkflowDesk();
  for (const target of [document, window]) for (const [type, listener, options] of vi.mocked(target.addEventListener).mock.calls) target.removeEventListener(type, listener, options);
  vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.body.innerHTML = "";
});
it("Edit script opens the selected registered copy; dirty tab is retained; Files Save refreshes the same source/scene", async () => {
  vi.useFakeTimers(); vi.spyOn(document, "addEventListener"); vi.spyOn(window, "addEventListener");
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = "/fixture/project", relative = "Scripts/Final authored dialogue.md", path = `${root}/${relative}`;
  const files = new Map([[path, "Scene 1 — Platform\nMARA: Before."], [`${root}/root.md`, "Scene 1\nRoot copy."]]);
  let latest = structuredClone(native);
  const forbidden = vi.fn(() => { throw new Error("Unexpected network/provider call"); }); vi.stubGlobal("fetch", forbidden);
  let failWrite = false;
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "list_dir") return [];
    if (cmd === "workflow_import") {
      const result = structuredClone(latest);
      if (args?.editedScriptPath) {
        result.persistedPath = `${root}/Preproduction/workflow_state.json`;
        result.editedScript = {path: args.editedScriptPath as string, stateRevision: 3, sourceSha256: "a".repeat(64), affectedSceneIds: [2]};
      }
      return result;
    }
    if (cmd === "classify_file") return "text";
    if (cmd === "read_text_file") return files.get(args!.path as string) ?? "";
    if (cmd === "write_text_file") {
      if (failWrite) throw new Error("EACCES fixture");
      files.set(args!.path as string, args!.contents as string);
      if (args!.path === path) for (const scene of latest.state.scenes.filter(s => s.source_path === relative && s.source_scene_number === 1)) { scene.dialogue = [{speaker:"MARA",text:"Après — café."}]; scene.source_excerpt = args!.contents as string; }
      return;
    }
    return forbidden();
  });
  const saveDialog = vi.fn(async (): Promise<string | null> => null);
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, saveDialog, openProject: async () => root, rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main"); const settle = () => vi.advanceTimersByTimeAsync(100); await settle();
  document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await settle();
  await desk.refreshScriptSelection(relative);
  const selected = desk.workflowSelection().sceneId;
  const button = (label: string, scope: ParentNode = document) => { const b = [...scope.querySelectorAll("button")].find(b => (b.querySelector('span:not([class])')?.textContent ?? b.textContent)?.trim() === label); expect(b, label).toBeDefined(); return b!; };
  button("Script", document.querySelector(".workflow-main")!).click();
  button("Edit script", document.querySelector(".workflow-script")!).click(); await settle();
  expect(document.querySelector('[data-surface="files"]')?.getAttribute("aria-current")).toBe("page");
  expect(invoke).toHaveBeenCalledWith("read_text_file", {path});
  const editor = document.querySelector<HTMLTextAreaElement>("#editor")!;
  const changed = "Scene 1 — Platform\nMARA: Après — café.";
  editor.value = changed; editor.dispatchEvent(new Event("input", {bubbles:true}));
  document.querySelector<HTMLButtonElement>('[data-surface="studio"]')!.click();
  button("Edit script", document.querySelector(".workflow-script")!).click(); await settle();
  expect(editor.value).toBe(changed);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "read_text_file")).toHaveLength(1);
  document.dispatchEvent(new KeyboardEvent("keydown", {key:"s",ctrlKey:true,bubbles:true,cancelable:true})); await settle();
  expect(files.get(path)).toBe(changed);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(3);
  expect(desk.workflowSelection().sceneId).toBe(selected);
  document.querySelector<HTMLButtonElement>('[data-surface="studio"]')!.click();
  expect(document.querySelector(".workflow-script")?.textContent).toContain("Après — café.");
  // Repeated Save updates without import/registration commands.
  button("Edit script", document.querySelector(".workflow-script")!).click(); await settle();
  document.querySelector<HTMLButtonElement>("#save-btn")!.click(); await settle();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(4);
  editor.value = `${changed}\nMARA: Unsaved after refusal.`;
  editor.dispatchEvent(new Event("input", {bubbles:true}));
  failWrite = true; document.querySelector<HTMLButtonElement>("#save-btn")!.click(); await settle();
  expect(editor.value).toContain("Unsaved after refusal.");
  expect(files.get(path)).toBe(changed);
  expect(document.querySelector(".tab.active")?.classList.contains("dirty")).toBe(true);
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(4);
  expect(document.querySelector("#status-left")?.textContent).toContain("Could not save");
  failWrite = false; document.querySelector<HTMLButtonElement>("#save-as-btn")!.click(); await settle();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(4);
  saveDialog.mockResolvedValue(`${root}/root.md`); document.querySelector<HTMLButtonElement>("#save-as-btn")!.click(); await settle();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(4);
  saveDialog.mockResolvedValue(`${root}/Scripts/New.md`); document.querySelector<HTMLButtonElement>("#save-as-btn")!.click(); await settle();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(5);
  failWrite = true; saveDialog.mockResolvedValue(path); document.querySelector<HTMLButtonElement>("#save-as-btn")!.click(); await settle();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(5);
  expect(document.querySelector("#status-left")?.textContent).toContain("Could not save"); failWrite = false;
  // A save finishing after the same project is reopened cannot refresh its replacement session.
  let finishWrite!: () => void;
  invoke.mockImplementationOnce(async (cmd: string) => { expect(cmd).toBe("write_text_file"); await new Promise<void>(resolve => finishWrite = resolve); });
  document.querySelector<HTMLButtonElement>("#save-btn")!.click(); await settle();
  document.querySelector<HTMLButtonElement>('[data-surface="home"]')!.click();
  document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await settle();
  const afterReopen = invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import").length;
  expect(afterReopen).toBe(6);
  finishWrite(); await settle();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(afterReopen);
  expect(forbidden).not.toHaveBeenCalled();
});
