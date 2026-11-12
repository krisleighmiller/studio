import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { Window } from "happy-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../src/settingsPanel", () => ({
  bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }),
  loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(),
  renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false,
}));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));

const imported = JSON.parse(readFileSync(resolve("tests/fixtures/sc05-import-result.json"), "utf8"));
const el = <T extends HTMLElement = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const stylesheet = readFileSync(resolve("src/styles.css"), "utf8");
const visible = (selector: string) => {
  // happy-dom caches descendant selector matches across ancestor class changes.
  // Compute on a fresh snapshot of the real bound DOM with the unchanged app CSS.
  const snapshot = new Window();
  snapshot.document.body.innerHTML = document.body.innerHTML.replace(/<script\b[^>]*>[\s\S]*?<\/script>/g, "");
  const style = snapshot.document.createElement("style");
  style.textContent = stylesheet;
  snapshot.document.head.append(style);
  const element = snapshot.document.querySelector(selector);
  expect(element, selector).not.toBeNull();
  for (let node = element; node; node = node.parentElement) {
    const computed = snapshot.getComputedStyle(node);
    if (computed.display === "none" || computed.visibility === "hidden") return false;
  }
  return true;
};

describe("MD01 real center-panel mode painting", () => {
  afterEach(async () => {
    (await import("../src/workflowDesk")).clearWorkflowDesk();
    for (const target of [document, window]) {
      for (const [type, listener, options] of vi.mocked(target.addEventListener).mock.calls) target.removeEventListener(type, listener, options);
    }
    vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals();
    document.body.innerHTML = ""; document.head.innerHTML = "";
    vi.resetModules();
  });

  it.each([null, "", "Original local text"])("synchronizes visible heading/ARIA/controls and preserves file state (%j)", async (initialContents) => {
    vi.useFakeTimers();
    vi.spyOn(document, "addEventListener"); vi.spyOn(window, "addEventListener");
    document.body.innerHTML = readFileSync(resolve("index.html"), "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
    const style = document.createElement("style");
    style.textContent = stylesheet;
    document.head.append(style);
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider/network command"); });
    vi.stubGlobal("fetch", forbidden); vi.stubGlobal("XMLHttpRequest", forbidden); vi.stubGlobal("WebSocket", forbidden);
    const invoke = vi.fn(async (cmd: string) => {
      if (cmd === "list_dir") return [];
      if (cmd === "workflow_import") return structuredClone(imported);
      if (cmd === "classify_file") return "text";
      if (cmd === "read_text_file") return initialContents ?? "Picker text";
      return forbidden();
    });
    const openDialog = vi.fn(async () => "/fixture/project/local.txt");
    Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openDialog, openProject: async () => "/fixture/project",
      rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
      onSpendConfirmationRequired: () => () => {} } });
    await import("../src/main");
    await vi.advanceTimersByTimeAsync(100);
    el("#home-open-project").click();
    await vi.advanceTimersByTimeAsync(100);
    const switchMode = async (mode: string) => {
      el(`.nav-item[data-surface="${mode}"]`).click();
      await vi.advanceTimersByTimeAsync(100);
    };
    const assertMode = (mode: "studio" | "files") => {
      const label = mode === "studio" ? "Production workflow" : "File editor";
      expect(el("#center-panel-heading")?.textContent).toBe(label);
      expect(visible("#center-panel-heading")).toBe(true);
      expect(el("#editor-panel").getAttribute("aria-label")).toBe(label);
      expect(visible("#workflow-view")).toBe(mode === "studio");
      expect(visible("#editor-tabs")).toBe(mode === "files");
      expect(visible(".explorer-panel")).toBe(mode === "files");
      if (mode === "studio") {
        expect(visible("#editor-toolbar")).toBe(false);
        expect(visible("#editor")).toBe(false);
        expect(visible("#editor-empty")).toBe(false);
      }
    };
    const openFile = async () => {
      el("#editor-tabs").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
      const button = [...document.querySelectorAll<HTMLButtonElement>("button")].find(b => b.textContent === "Open File…")!;
      expect(button.disabled).toBe(false);
      button.click();
      await vi.advanceTimersByTimeAsync(100);
    };
    await switchMode("studio");
    assertMode("studio");
    await switchMode("files");
    assertMode("files");
    if (initialContents !== null) {
      await openFile();
      expect(el<HTMLTextAreaElement>("#editor").value).toBe(initialContents);
      if (initialContents) {
        el<HTMLTextAreaElement>("#editor").value = "Unsaved local edit";
        el("#editor").dispatchEvent(new Event("input", { bubbles: true }));
        el<HTMLTextAreaElement>("#editor").setSelectionRange(2, 9);
      }
    }
    const editor = el<HTMLTextAreaElement>("#editor");
    const before = { value: editor.value, start: editor.selectionStart, end: editor.selectionEnd,
      tab: document.querySelector("#editor-tabs .active")?.textContent ?? null };
    await switchMode("studio");
    assertMode("studio");
    await switchMode("files");
    assertMode("files");
    expect({ value: editor.value, start: editor.selectionStart, end: editor.selectionEnd,
      tab: document.querySelector("#editor-tabs .active")?.textContent ?? null }).toEqual(before);
    expect(visible("#editor-empty")).toBe(initialContents === null);
    expect(visible("#editor")).toBe(initialContents !== null);
    expect(visible("#editor-toolbar")).toBe(initialContents !== null);
    await openFile();
    expect(openDialog).toHaveBeenLastCalledWith(expect.objectContaining({ title: "Open File", multiple: false, directory: false, defaultPath: "/fixture/project" }));
    expect(invoke).toHaveBeenCalledWith("read_text_file", { path: "/fixture/project/local.txt" });
    expect(visible("#editor")).toBe(true);
    expect(forbidden).not.toHaveBeenCalled();
  });
});
