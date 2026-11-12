import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { Window } from "happy-dom";
import { afterEach, expect, it, vi } from "vitest";

vi.mock("../src/settingsPanel", () => ({
  bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }),
  loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(),
  renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false,
}));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));

import { buildModelSheetRequest } from "../src/studio/modelSheetRequest";
import type { WorkflowImportResult } from "../src/workflowTypes";

const controls: Record<string, string[]> = {
  "src/workflowDesk.ts": ["Create script", "Import script", "Use existing project file"],
  "src/studio/surfaces/scriptDraftForm.ts": ["Title", "Notes", "Script", "Load notes", "Save script", "Cancel"],
  "src/main.ts": ["Use as project script", "Open File…"],
  "src/studio/surfaces/charactersStage.ts": ["Character name", "Description", "Appearance", "Identity prompt", "Create character", "Save profile", "Add reference image", "Remove", "Create full 12-pose sheet", "Update full 12-pose sheet"],
};

afterEach(async () => {
  (await import("../src/workflowDesk")).clearWorkflowDesk();
  for (const target of [document, window]) {
    for (const [type, listener, options] of vi.mocked(target.addEventListener).mock.calls) target.removeEventListener(type, listener, options);
  }
  vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals();
  document.body.innerHTML = ""; document.head.innerHTML = "";
  vi.resetModules();
});

it.each([false, true])("Documented minimal visual inputs match Create/Update (existing pack: %s)", async existingPack => {
  vi.spyOn(document, "addEventListener"); vi.spyOn(window, "addEventListener");
  const forbidden = vi.fn(() => { throw new Error("Unexpected provider/network/chat operation"); });
  vi.stubGlobal("fetch", forbidden); vi.stubGlobal("XMLHttpRequest", forbidden); vi.stubGlobal("WebSocket", forbidden);
  const fixture = JSON.parse(readFileSync(resolve("tests/fixtures/sc05-import-result.json"), "utf8")) as WorkflowImportResult;
  const character = {
    id: "identity_fixture", name: "Alice", dir: "Characters/Alice", profile_path: "Characters/Alice/character.yaml",
    identity_ready: existingPack, voice_ready: false, reference_asset_ids: [],
    profile: { description: "A patient botanist with freckles.", appearance: ["Curly black hair", "Green coat"], identity_prompt: "" },
  };
  fixture.state.characters = [character]; fixture.state.assets = [];
  const host = await import("../src/host");
  const invoke = vi.spyOn(host, "invoke").mockImplementation(async (command: string) => {
    if (command === "workflow_import") return structuredClone(fixture);
    if (command === "start_character_model_sheet") return { status: "pendingApproval", pendingAction: "Fixture approval [[workbench_approve:R3ABC]]", characterId: character.id };
    if (command === "director_cancel") return null;
    return forbidden();
  });
  const ask = vi.fn(async () => false);
  Object.defineProperty(window, "workbench", { configurable: true, writable: true, value: { ask } });
  const desk = await import("../src/workflowDesk");
  const { renderCharactersStage } = await import("../src/studio/surfaces/charactersStage");
  const root = document.createElement("div"); document.body.append(root);
  desk.bindWorkflowDesk({ root, projectRoot: () => "/fixture/project", onOpenPath: forbidden, onAddToChat: forbidden,
    onOpenVoice: forbidden, onComposeChat: forbidden, onPendingPlan: forbidden, onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
  let pending = Promise.resolve();
  const click = () => {
    const stage = renderCharactersStage(intent => { pending = desk.handleCharacterIntent(intent); });
    root.replaceChildren(stage);
    const button = [...stage.querySelectorAll("button")].find(node => node.textContent === (existingPack ? "Update full 12-pose sheet" : "Create full 12-pose sheet"));
    expect(button).toBeDefined(); button!.click();
    return pending;
  };
  await desk.loadWorkflowDesk("/fixture/project"); invoke.mockClear();
  const accepted = buildModelSheetRequest(character, character.profile.identity_prompt, []);
  expect(accepted).toMatchObject({ inputs: { identity_text: "A patient botanist with freckles.\n\nCurly black hair\n\nGreen coat" } });
  const action = click();
  const dialog = document.querySelector<HTMLDialogElement>('dialog[data-generation-prompt]');
  expect(dialog).not.toBeNull();
  expect(dialog!.querySelector<HTMLTextAreaElement>('[name="prompt"]')!.value).toBe(accepted.inputs.identity_text);
  expect(invoke).not.toHaveBeenCalled();
  dialog!.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await action;
  expect(invoke.mock.calls).toEqual([
    ["start_character_model_sheet", { projectRoot: "/fixture/project", request: accepted }], ["director_cancel"],
  ]);
  expect(ask).toHaveBeenCalledOnce(); expect(root.textContent).toMatch(/denied/i);
  expect(forbidden).not.toHaveBeenCalled();

  const guide = readFileSync(resolve("src/docs/workbench-guide.md"), "utf8").split('<h2 id="recovery-5">')[1].split('<h2 id="recovery-6">')[0];
  expect(guide).toContain("selected reference images, or both are sufficient for Create and Update");
  expect(guide).toContain("**Identity prompt** is optional");
  expect(guide).not.toContain("Identity prompt** is required");
});

it("HP01 F1 renders the current bundled guide before/after project open and reuses its tab without provider calls", async () => {
  vi.useFakeTimers();
  vi.spyOn(document, "addEventListener"); vi.spyOn(window, "addEventListener");
  document.body.innerHTML = readFileSync(resolve("index.html"), "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const forbidden = vi.fn(() => { throw new Error("Unexpected provider/network command"); });
  vi.stubGlobal("fetch", forbidden); vi.stubGlobal("XMLHttpRequest", forbidden); vi.stubGlobal("WebSocket", forbidden);
  const fixture = JSON.parse(readFileSync(resolve("tests/fixtures/sc05-import-result.json"), "utf8"));
  const invoke = vi.fn(async (command: string) => {
    if (command === "list_dir") return [];
    if (command === "workflow_import") return structuredClone(fixture);
    return forbidden();
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => "/fixture/project",
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {},
    onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  await vi.advanceTimersByTimeAsync(100);
  const openHelp = async () => {
    const before = invoke.mock.calls.length;
    const event = new KeyboardEvent("keydown", { key: "F1", bubbles: true, cancelable: true });
    document.dispatchEvent(event);
    await vi.advanceTimersByTimeAsync(100);
    expect(event.defaultPrevented).toBe(true);
    expect(invoke.mock.calls.length).toBe(before);
    const docs = document.querySelector<HTMLElement>("#docs-view")!;
    expect(docs.classList.contains("hidden")).toBe(false);
    const snapshot = new Window();
    snapshot.document.body.innerHTML = document.body.innerHTML;
    const style = snapshot.document.createElement("style");
    style.textContent = readFileSync(resolve("src/styles.css"), "utf8");
    snapshot.document.head.append(style);
    for (let node = snapshot.document.querySelector("#docs-view"); node; node = node.parentElement) {
      expect(snapshot.getComputedStyle(node).display).not.toBe("none");
    }
    expect(docs.textContent?.trim().startsWith("Studio — Current workflow guide")).toBe(true);
    for (const [file, labels] of Object.entries(controls)) {
      const source = readFileSync(resolve(file), "utf8");
      for (const label of labels) {
        expect(source, file).toContain(`"${label}"`);
        expect(docs.textContent, label).toContain(label);
      }
    }
    expect(docs.textContent).toContain("Production workflow");
    expect(docs.textContent).toContain("File editor");
    expect(docs.textContent).toContain(".md, .markdown and .txt");
    expect(docs.textContent).toContain("Generate script from notes");
    expect(docs.textContent).toContain("native confirmation");
    expect(docs.textContent).toContain("typing never calls a model");
    expect(docs.textContent).toContain("retired");
    expect(docs.textContent).not.toMatch(/Basic prompt|Build identity|### 1\. Script Assistant/);
    for (let issue = 1; issue <= 7; issue++) expect(docs.querySelector(`#recovery-${issue}`)).not.toBeNull();
    const links = [...docs.querySelectorAll<HTMLAnchorElement>("a[href]")];
    expect(links.length).toBeGreaterThanOrEqual(7);
    for (const link of links) {
      const href = link.getAttribute("href")!;
      expect(href.startsWith("#")).toBe(true);
      expect(docs.querySelector(`[id="${href.slice(1)}"]`), href).not.toBeNull();
      link.click();
    }
    return docs.innerHTML;
  };
  const first = await openHelp();
  document.querySelector<HTMLButtonElement>("#home-open-project")!.click();
  await vi.advanceTimersByTimeAsync(100);
  expect(document.querySelector(".nav-item.active")?.getAttribute("data-surface")).toBe("studio");
  expect(await openHelp()).toBe(first);
  expect(await openHelp()).toBe(first);
  expect([...document.querySelectorAll("#editor-tabs button")].filter(tab => tab.textContent?.includes("Documentation"))).toHaveLength(1);
  expect(forbidden).not.toHaveBeenCalled();
});
