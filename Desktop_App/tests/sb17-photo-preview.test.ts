import { readFileSync, writeFileSync } from "node:fs";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

let desk: typeof import("../src/workflowDesk");

vi.mock("../src/settingsPanel", () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({ selectedChatModel: null }), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock("../src/voicePanel", () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
beforeEach(async () => {
  vi.resetModules();
  vi.spyOn(document, "addEventListener");
  vi.spyOn(window, "addEventListener");
  desk = await import("../src/workflowDesk");
});
afterEach(() => {
  desk.clearWorkflowDesk();
  for (const target of [document, window]) for (const [type, listener, options] of vi.mocked(target.addEventListener).mock.calls) target.removeEventListener(type, listener, options);
  vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); document.body.innerHTML = "";
});

it("model_sheet_delete_matching_preview_only closes the target preview and keeps the survivor preview open", async () => {
  vi.useFakeTimers();
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = "/fixture/project";
  const state = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  const char = { id: "alice", name: "Alice", dir: "Characters/Alice", profile_path: "Characters/Alice/character.yaml", identity_ready: true, voice_ready: false, profile: { identity_prompt: "Alice" }, asset_ids: ["a", "b"], reference_asset_ids: [] };
  const photo = (id: string) => ({ id, path: `Characters/Alice/model_sheet/${id}.png`, role: "body_reference", status: "present", character_id: "alice", notes: id });
  state.state.characters = [char]; state.state.assets = [photo("a"), photo("b")];
  state.state.scenes = []; state.report.scenes = [];
  let fail = true;
  const invoke = vi.fn(async (cmd: string, args: any) => {
    if (cmd === "list_dir") return [];
    if (cmd === "classify_file") return "image";
    if (cmd === "workflow_import") return structuredClone(state);
    if (cmd === "workflow_delete_model_sheet_photo" || cmd === "delete_path") {
      if (fail) throw new Error("Fixture trash refused");
      const id = cmd === "delete_path" ? "b" : args.assetId;
      state.state.assets = state.state.assets.filter((a: any) => a.id !== id);
      char.asset_ids = char.asset_ids.filter(a => a !== id);
      return cmd === "delete_path" ? null : structuredClone(state);
    }
    throw new Error(`Unexpected ${cmd}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, toMediaUrl: (path: string) => `file://${path}`, openProject: async () => root, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  const flush = () => vi.advanceTimersByTimeAsync(150);
  await flush(); document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await flush();
  desk.setStudioWorkArea("plan");
  [...document.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")].find(b => b.querySelector("span:not(.studio-surface-dot)")?.textContent === "Characters")!.click();
  const gallery = () => document.querySelector(".workflow-character-model-sheet-gallery")!;
  const open = async (id: string) => { gallery().querySelector<HTMLImageElement>(`img[data-look-path$="/${id}.png"]`)!.click(); await flush(); };
  const tabs = () => [...document.querySelectorAll("#editor-tabs .tab")].map(e => e.textContent);
  const remove = async (id: string) => { document.querySelector<HTMLButtonElement>(`button[aria-label="Delete model-sheet photo ${id}"]`)!.click(); await flush(); };
  await open("a"); await open("b");
  expect(tabs().join(" ")).toContain("a.png"); expect(tabs().join(" ")).toContain("b.png");
  await remove("a");
  expect(tabs().join(" ")).toContain("a.png"); expect(gallery().querySelector('img[data-look-path$="/a.png"]')).not.toBeNull();
  fail = false; await remove("a");
  expect(gallery().querySelector('img[data-look-path$="/a.png"]')).toBeNull();
  expect(tabs().join(" ")).not.toContain("a.png"); expect(tabs().join(" ")).toContain("b.png");
  expect(document.querySelector<HTMLImageElement>("#media-image")!.src).toContain("b.png");
  // Detached preview's ordinary Files delete uses the same post-success policy.
  fail = true; document.querySelector<HTMLButtonElement>("#delete-file-btn")!.click(); await flush();
  expect(tabs().join(" ")).toContain("b.png");
  expect(document.querySelector<HTMLImageElement>("#media-image")!.src).toContain("b.png");
  fail = false; document.querySelector<HTMLButtonElement>("#delete-file-btn")!.click(); await flush();
  expect(tabs().join(" ")).not.toContain("b.png");
  expect(document.querySelector(".workflow-character-model-sheet-gallery")).toBeNull();
  if (process.env.SB17_RECEIPT) writeFileSync(process.env.SB17_RECEIPT, JSON.stringify({ galleryFailureRetained: true, gallerySuccessRemovedOnlyTarget: true, survivorPreview: "b.png", detachedFailureRetained: true, detachedSuccessRemoved: true, commands: invoke.mock.calls.map(([cmd]) => cmd) }, null, 2));
});

it("model_sheet_delete_failure_retains_ui keeps the gallery figure and matching preview visible after refusal", async () => {
  vi.useFakeTimers();
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = "/fixture/project";
  const state = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  const character = { id: "alice", name: "Alice", dir: "Characters/Alice", profile_path: "Characters/Alice/character.yaml", identity_ready: true, voice_ready: false, profile: { identity_prompt: "Alice" }, asset_ids: ["refused-photo"], reference_asset_ids: [] };
  const photo = { id: "refused-photo", path: "Characters/Alice/model_sheet/refused-photo.png", role: "body_reference", status: "present", character_id: "alice", notes: "refused-photo" };
  state.state.characters = [character]; state.state.assets = [photo];
  state.state.scenes = []; state.report.scenes = [];
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "list_dir") return [];
    if (cmd === "classify_file") return "image";
    if (cmd === "workflow_import") return structuredClone(state);
    if (cmd === "workflow_delete_model_sheet_photo") throw new Error("Fixture host refused delete");
    throw new Error(`Unexpected ${cmd}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, toMediaUrl: (path: string) => `file://${path}`, openProject: async () => root, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  const flush = () => vi.advanceTimersByTimeAsync(150);
  await flush(); document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await flush();
  desk.setStudioWorkArea("plan");
  [...document.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")].find(b => b.querySelector("span:not(.studio-surface-dot)")?.textContent === "Characters")!.click();
  const gallery = () => document.querySelector(".workflow-character-model-sheet-gallery")!;
  const targetImage = gallery().querySelector<HTMLImageElement>('img[data-look-path$="/refused-photo.png"]')!;
  targetImage.click(); await flush();
  expect(document.querySelector<HTMLImageElement>("#media-image")!.src).toContain("refused-photo.png");

  const targetFigure = targetImage.closest("figure")!;
  const deleteButton = targetFigure.querySelector<HTMLButtonElement>('button[aria-label="Delete model-sheet photo refused-photo"]')!;
  deleteButton.click(); await flush();

  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_delete_model_sheet_photo")).toHaveLength(1);
  expect(gallery().querySelector('img[data-look-path$="/refused-photo.png"]')).not.toBeNull();
  expect([...document.querySelectorAll("#editor-tabs .tab")].map(tab => tab.textContent).join(" ")).toContain("refused-photo.png");
  expect(document.querySelector<HTMLImageElement>("#media-image")!.src).toContain("refused-photo.png");
});

it("model_sheet_delete_failure_retryable shows a delete error, clears busy state, and allows retry", async () => {
  vi.useFakeTimers();
  document.body.innerHTML = readFileSync("index.html", "utf8").match(/<body>([\s\S]*)<\/body>/)![1];
  const root = "/fixture/project";
  const state = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  const character = { id: "alice", name: "Alice", dir: "Characters/Alice", profile_path: "Characters/Alice/character.yaml", identity_ready: true, voice_ready: false, profile: { identity_prompt: "Alice" }, asset_ids: ["retry-photo"], reference_asset_ids: [] };
  const photo = { id: "retry-photo", path: "Characters/Alice/model_sheet/retry-photo.png", role: "body_reference", status: "present", character_id: "alice", notes: "retry-photo" };
  state.state.characters = [character]; state.state.assets = [photo];
  state.state.scenes = []; state.report.scenes = [];
  let attempts = 0;
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "list_dir") return [];
    if (cmd === "classify_file") return "image";
    if (cmd === "workflow_import") return structuredClone(state);
    if (cmd === "workflow_delete_model_sheet_photo") {
      attempts += 1;
      if (attempts === 1) throw new Error("Fixture host refused delete");
      state.state.assets = [];
      character.asset_ids = [];
      return structuredClone(state);
    }
    throw new Error(`Unexpected ${cmd}`);
  });
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, toMediaUrl: (path: string) => `file://${path}`, openProject: async () => root, ask: async () => true,
    rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  await import("../src/main");
  const flush = () => vi.advanceTimersByTimeAsync(150);
  await flush(); document.querySelector<HTMLButtonElement>("#home-open-project")!.click(); await flush();
  desk.setStudioWorkArea("plan");
  [...document.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")].find(b => b.querySelector("span:not(.studio-surface-dot)")?.textContent === "Characters")!.click();
  const deleteButton = () => document.querySelector<HTMLButtonElement>('button[aria-label="Delete model-sheet photo retry-photo"]');

  deleteButton()!.click(); await flush();

  expect(document.querySelector(".workflow-desk")?.classList.contains("busy")).toBe(false);
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Could not delete model-sheet photo: Fixture host refused delete");
  expect(deleteButton()).not.toBeNull();
  expect(deleteButton()!.disabled).toBe(false);

  deleteButton()!.click(); await flush();
  expect(invoke.mock.calls.filter(([cmd]) => cmd === "workflow_delete_model_sheet_photo")).toHaveLength(2);
  expect(deleteButton()).toBeNull();
});

it("model_sheet_delete_reopened_session_ignored keeps the reopened gallery and preview when an old result settles", async () => {
  const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8"));
  fixture.state.characters = [{ id: "alice", name: "Alice", dir: "Characters/Alice", asset_ids: ["a"], identity_ready: true, voice_ready: false }];
  fixture.state.assets = [{ id: "a", path: "Characters/Alice/model_sheet/a.png", role: "body_reference", character_id: "alice", notes: "a" }];
  let finish!: (result: unknown) => void;
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === "workflow_import") return structuredClone(fixture);
    if (cmd === "workflow_delete_model_sheet_photo") return new Promise(resolve => { finish = resolve; });
    throw new Error(cmd);
  });
  Object.assign(window, { workbench: { invoke, ask: async () => true } });
  const root = document.createElement("div"); document.body.append(root);
  const openPreviews = new Set<string>();
  const trashed = vi.fn((path: string) => openPreviews.delete(path));
  desk.bindWorkflowDesk({ root, projectRoot: () => "/fixture/project", onOpenPath: path => openPreviews.add(path), onPathTrashed: trashed,
    onAddToChat: vi.fn(), onOpenVoice: vi.fn(), onComposeChat: vi.fn(), onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
  const showCharacters = () => {
    desk.setStudioWorkArea("plan");
    [...root.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")]
      .find(button => button.querySelector("span:not(.studio-surface-dot)")?.textContent === "Characters")!.click();
  };
  const target = () => root.querySelector<HTMLImageElement>('img[data-look-path$="/a.png"]');

  await desk.loadWorkflowDesk("/fixture/project");
  showCharacters();
  target()!.click();
  expect(openPreviews).toEqual(new Set(["/fixture/project/Characters/Alice/model_sheet/a.png"]));
  const pending = desk.handleCharacterIntent({ type: "delete-model-sheet-photo", characterKey: "alice", assetId: "a" });
  await vi.waitFor(() => expect(finish).toBeDefined());

  desk.clearWorkflowDesk();
  await desk.loadWorkflowDesk("/fixture/project");
  showCharacters();
  const reopenedPhoto = target();
  expect(reopenedPhoto).not.toBeNull();

  finish({ ...fixture, state: { ...fixture.state, assets: [], characters: [] } });
  await pending;

  expect(target()).toBe(reopenedPhoto);
  expect(openPreviews).toEqual(new Set(["/fixture/project/Characters/Alice/model_sheet/a.png"]));
  expect(trashed).not.toHaveBeenCalled();
});
