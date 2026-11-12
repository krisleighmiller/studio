import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import type { WorkflowImportResult } from "../src/workflowTypes";

// Real main + workflow desk + typed ingestion seam; only IPC/disk and unrelated panels are fixtures.
vi.mock("../src/settingsPanel", () => ({
  bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }),
  loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(),
  renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false,
}));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
const imported = JSON.parse(readFileSync(resolve("tests/fixtures/sc05-import-result.json"), "utf8")) as WorkflowImportResult;

describe("SC09 actual Files and Plan bindings", () => {
  afterEach(() => {
    desk.clearWorkflowDesk();
    for (const target of [document, window]) {
      for (const [type, listener, options] of vi.mocked(target.addEventListener).mock.calls) target.removeEventListener(type, listener, options);
    }
    vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals();
    document.body.innerHTML = "";
  });

  it("uses the same typed existing-project handler from Files and Plan, and retains selection on failed registration", async () => {
    vi.useFakeTimers();
    vi.spyOn(document, "addEventListener"); vi.spyOn(window, "addEventListener");
    document.body.innerHTML = readFileSync(resolve("index.html"), "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/network command"); });
    vi.stubGlobal("fetch", forbidden); vi.stubGlobal("XMLHttpRequest", forbidden); vi.stubGlobal("WebSocket", forbidden);
    let registered = false;
    let failure = "";
    const invoke = vi.fn(async (cmd: string, _args?: Record<string, unknown>) => {
      if (cmd === "list_dir") return [{ name: "root-script.md", path: "/fixture/project/root-script.md", isDir: false }];
      if (cmd === "workflow_import") {
        const value = structuredClone(imported);
        if (!registered) value.state.scenes = [];
        return value;
      }
      if (cmd === "ingest_script_file") {
        if (failure) throw new Error(failure);
        registered = true;
        return "Scripts/imported.md";
      }
      return forbidden();
    });
    const openDialog = vi.fn(async (): Promise<string | null> => "/fixture/project/root-script.md");
    Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openDialog, openProject: async () => "/fixture/project",
      rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
      onSpendConfirmationRequired: () => () => {} } });
    await import("../src/main");
    await vi.advanceTimersByTimeAsync(100);
    document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
    await vi.advanceTimersByTimeAsync(100);
    expect(desk.workflowSelection().sceneId).toBeNull();
    const handler = vi.spyOn(desk, "handleUseAsProjectScript");
    const button = (label: string) => {
      const b = [...document.querySelectorAll("button")].find(b => b.textContent === label);
      expect(b, label).toBeDefined(); return b!;
    };
    const registerCalls = () => invoke.mock.calls.filter(([cmd]) => cmd === "ingest_script_file");
    const rootFile = [...document.querySelectorAll<HTMLElement>(".tree-item")].find(e => e.textContent?.includes("root-script.md"));
    expect(rootFile).toBeDefined();
    rootFile!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    button("Use as project script").click();
    await vi.advanceTimersByTimeAsync(100);
    expect(handler).toHaveBeenCalledWith("/fixture/project/root-script.md");
    expect(openDialog).not.toHaveBeenCalled();
    expect(desk.workflowSelection().sceneId).toBe(imported.state.scenes.find(scene => scene.source_path === "Scripts/imported.md")!.id);
    button("Use existing project file").click();
    await vi.advanceTimersByTimeAsync(100);
    // Same-module Plan calls are lexical (not interceptable by an export spy).
    // Both real bindings must reach the identical typed SC04 mode/path below.
    expect(handler).toHaveBeenCalledTimes(1);
    expect(openDialog).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "/fixture/project", multiple: false }));
    expect(registerCalls()).toEqual(Array.from({ length: 2 }, () => ["ingest_script_file", {
      projectRoot: "/fixture/project", sourcePath: "/fixture/project/root-script.md", sourceKind: "existing-project-file",
    }]));
    expect(desk.workflowSelection().sceneId).toBe(imported.state.scenes.find(scene => scene.source_path === "Scripts/imported.md")!.id);
    const selection = desk.workflowSelection();
    for (const path of ["/outside/not-project.md", "/fixture/project/Scripts/imported.md"]) {
      failure = path.startsWith("/outside") ? "Source must be inside the project" : "EACCES: denied";
      const refreshCount = invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import").length;
      openDialog.mockResolvedValue(path);
      const result = await desk.handleUseAsProjectScript(path.startsWith("/outside") ? undefined : path);
      expect(result).toEqual({ importFeedback: expect.stringContaining(failure) });
      expect(document.body.textContent).toContain("Could not use project script");
      if (path.startsWith("/outside")) {
        // Assert the mounted controller's rendered feedback, not just its return value.
        expect(document.body.textContent).toContain("For an external source, use Import script instead.");
      }
      expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_import")).toHaveLength(refreshCount);
      expect(desk.workflowSelection()).toEqual(selection);
    }
    failure = "";
    await desk.handleUseAsProjectScript("/fixture/project/Scripts/imported.md");
    expect(registerCalls().at(-1)![1]).toEqual({ projectRoot: "/fixture/project", sourcePath: "/fixture/project/Scripts/imported.md", sourceKind: "existing-project-file" });
    expect(desk.workflowSelection().sceneId).toBe(imported.state.scenes.find(scene => scene.source_path === "Scripts/imported.md")!.id);
    openDialog.mockResolvedValue(null);
    const count = invoke.mock.calls.length;
    expect(await desk.handleUseAsProjectScript()).toBe("cancelled");
    expect(invoke).toHaveBeenCalledTimes(count);
    expect(forbidden).not.toHaveBeenCalled();
  });
});
