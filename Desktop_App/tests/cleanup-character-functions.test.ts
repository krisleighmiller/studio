import { afterEach, describe, expect, it, vi } from "vitest";
import { renderCharactersStage } from "../src/studio/surfaces/charactersStage";
import { bindSurfaceHost, characterDrafts, resetSurfaceSession, setNewCharacterOpen } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { CharacterRecord, ProjectState, WorkflowImportResult } from "../src/workflowTypes";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";
import { readFileSync, mkdtempSync, mkdirSync, rmSync, existsSync } from "node:fs";
import { spawn, execFileSync } from "node:child_process";
import { createInterface } from "node:readline";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;

const character = (id: string): CharacterRecord => ({ id, name: id, dir: `Characters/${id}`, identity_ready: false, voice_ready: false, profile: {}, reference_asset_ids: [`${id}-photo`], asset_ids: [`${id}-photo`] });
const click = (root: HTMLElement, label: string) => {
  const button = [...root.querySelectorAll("button")].find(button => button.textContent === label);
  expect(button, label).toBeDefined();
  button!.click();
};

describe("CR04 reference effects", () => {
  afterEach(() => { desk.clearWorkflowDesk(); resetSurfaceSession(); vi.unstubAllGlobals(); document.body.innerHTML = ""; });
  async function setup() {
    const persisted = structuredClone(fixture);
    persisted.state.characters = [character("alice"), character("bob")];
    persisted.state.assets = persisted.state.characters.map(person => ({ id: `${person.id}-photo`, path: `${person.dir}/photo.png`, character_id: person.id, role: "identity_reference" })) as ProjectState["assets"];
    const forbidden = vi.fn(() => { throw new Error("Unexpected provider call"); });
    const picker = vi.fn(async (): Promise<string | null> => "/external/portrait.png");
    const ask = vi.fn(async () => true);
    const trashed = vi.fn();
    const filesChanged = vi.fn();
    const files = new Set<string>();
    const failures = { command: "", preexisting: false };
    const invoke = vi.fn(async (cmd: string, args: Record<string, unknown> = {}): Promise<unknown> => {
      if (cmd === failures.command) throw new Error(`Fixture ${cmd} failure`);
      if (cmd === "workflow_save_character") {
        const existing = persisted.state.characters.find(person => person.id === args.characterId);
        if (!existing) persisted.state.characters.push({ ...character("new-person"), name: String(args.name), dir: "Characters/New person", reference_asset_ids: [], asset_ids: [] });
        return structuredClone(persisted);
      }
      if (cmd === "ingest_character_reference_image") {
        const path = `${args.characterDir}/refs/portrait.png`;
        files.add(path);
        return { path, created: !failures.preexisting };
      }
      if (cmd === "delete_path") { files.delete(String(args.path).replace("/fixture/project/", "")); return null; }
      if (cmd === "workflow_save_character_reference_selection") {
        const person = persisted.state.characters.find(person => person.id === args.characterId)!;
        person.reference_asset_ids = [...args.assetIds as string[]];
        return structuredClone(person);
      }
      if (cmd === "workflow_delete_character_reference") {
        const person = persisted.state.characters.find(person => person.id === args.characterId)!;
        const asset = persisted.state.assets?.find(asset => asset.id === args.assetId)!;
        files.delete(asset.path);
        persisted.state.assets = persisted.state.assets?.filter(asset => asset.id !== args.assetId);
        person.asset_ids = person.asset_ids?.filter(id => id !== args.assetId);
        person.reference_asset_ids = person.reference_asset_ids?.filter(id => id !== args.assetId);
        // Exact native delete receipt: no report/summary or importer side effects.
        return { state: structuredClone(persisted.state), note: "Moved uploaded character reference to recoverable project trash." };
      }
      if (cmd === "workflow_import") {
        persisted.state.assets = persisted.state.assets.filter(asset => !asset.path.includes("/refs/") || files.has(asset.path));
        for (const path of files) {
          if (!persisted.state.assets.some(asset => asset.path === path)) {
            const person = persisted.state.characters.find(person => path.startsWith(`${person.dir}/`))!;
            persisted.state.assets.push({ id: `${person.id}-uploaded`, path, character_id: person.id, role: "identity_reference", notes: "uploaded reference image" } as NonNullable<ProjectState["assets"]>[number]);
            person.asset_ids?.push(`${person.id}-uploaded`);
          }
        }
        return structuredClone(persisted);
      }
      return forbidden();
    });
    Object.defineProperty(window, "workbench", { configurable: true, value: { invoke, openDialog: picker, ask } });
    vi.stubGlobal("fetch", forbidden); vi.stubGlobal("WebSocket", forbidden); vi.stubGlobal("XMLHttpRequest", forbidden);
    let projectRoot = "/fixture/project";
    const status = vi.fn();
    const deskRoot = document.createElement("div");
    desk.bindWorkflowDesk({ root: deskRoot, projectRoot: () => projectRoot, onOpenPath: forbidden, onPathTrashed: trashed,
      onProjectFilesChanged: filesChanged, onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden,
      onSelectionChange: vi.fn(), chatModel: () => null, onStatus: status });
    await desk.loadWorkflowDesk("/fixture/project");
    invoke.mockClear();
    let pending: Promise<void> = Promise.resolve();
    const render = () => { const root = renderCharactersStage(intent => { pending = desk.handleCharacterIntent(intent); }); document.body.replaceChildren(root); return root; };
    return { persisted, forbidden, picker, ask, trashed, filesChanged, invoke, render, files, failures, status, deskRoot,
      setRoot: (root: string) => { projectRoot = root; }, settle: () => pending };
  }

  it("character_reference_saved_picker_opens", async () => {
    const s = await setup();
    s.picker.mockResolvedValueOnce(null);

    click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Add reference image");
    await s.settle();

    expect(s.picker).toHaveBeenCalledOnce();
    expect(s.picker).toHaveBeenCalledWith({
      title: "Choose character reference image",
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }],
    });
    expect(s.invoke).not.toHaveBeenCalled();
  });

  it.each(["picker", "reselection", "profile", "acquisition", "import", "association", "cleanup-delete", "cleanup-import"].flatMap(boundary =>
    [false, true].map(reject => ({ boundary, reject }))))("R2 discards $boundary (reject=$reject) after replacement without touching Bob", async ({ boundary, reject }) => {
    const s = await setup();
    let release!: () => void;
    let reached!: () => void;
    const held = new Promise<void>(resolve => { reached = resolve; });
    const gate = new Promise<void>(resolve => { release = resolve; });
    const delay = async <T,>(value: T): Promise<T> => { reached(); await gate; if (reject) throw new Error("Late Alice failure"); return value; };
    const original = s.invoke.getMockImplementation()!;
    const replacement = structuredClone(fixture);
    replacement.state.characters = [character("bob")];
    replacement.state.assets = [];
    replacement.note = "Bob project feedback";
    let imports = 0;
    let switched = false;
    const afterSwitch: string[] = [];
    s.invoke.mockImplementation(async (cmd, args = {}) => {
      if (args.projectRoot === "/fixture/B") {
        if (cmd === "workflow_import") return structuredClone(replacement);
        throw new Error(`Unexpected B mutation ${cmd}`);
      }
      if (switched) { afterSwitch.push(cmd); throw new Error("Wrong open project"); }
      if (cmd === "workflow_import") imports++;
      if (boundary.startsWith("cleanup-") && cmd === "workflow_save_character_reference_selection") throw new Error("Association fixture failure");
      const value = await original(cmd, args);
      const target = boundary === "profile" ? "workflow_save_character"
        : boundary === "acquisition" ? "ingest_character_reference_image"
        : boundary === "association" ? "workflow_save_character_reference_selection"
        : boundary === "cleanup-delete" ? "delete_path" : "workflow_import";
      if (cmd === target && !["picker", "reselection"].includes(boundary)
        && (boundary !== "cleanup-import" || imports === 2)) return delay(value);
      return value;
    });
    if (["picker", "reselection"].includes(boundary)) s.picker.mockImplementationOnce(() => delay("/external/portrait.png"));
    if (["profile", "reselection"].includes(boundary)) {
      setNewCharacterOpen(true);
      characterDrafts.set("__new__", { name: "Alice", profile: {}, references: [{ id: "ref", path: "/external/portrait.png" }],
        ...(boundary === "reselection" ? { referenceReselectionPaths: ["/external/portrait.png"] } : {}) });
      click(s.render(), "Create character");
    } else click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Add reference image");
    const aliceOperation = s.settle();
    await held;
    desk.clearWorkflowDesk();
    s.setRoot("/fixture/B"); switched = true;
    await desk.loadWorkflowDesk("/fixture/B");
    setNewCharacterOpen(true);
    characterDrafts.set("__new__", { name: "Bob draft", profile: { description: "Keep Bob" } });
    // A real B-owned operation stays busy while the old A response settles.
    let releaseBob!: (value: null) => void;
    s.picker.mockImplementationOnce(() => new Promise(resolve => { releaseBob = resolve; }));
    click(s.render().querySelector<HTMLElement>('[data-character-key="bob"]')!, "Add reference image");
    const bobOperation = s.settle();
    const before = { state: structuredClone(getSurfaceHost().state), selection: desk.workflowSelection(),
      drafts: structuredClone([...characterDrafts]), busy: getSurfaceHost().busy, error: getSurfaceHost().error, status: s.status.mock.calls.slice(), feedback: s.deskRoot.textContent };
    expect(before.busy).toBe("Updating character references…");
    release(); await aliceOperation;
    expect(getSurfaceHost().state).toEqual(before.state);
    expect(desk.workflowSelection()).toEqual(before.selection);
    expect([...characterDrafts]).toEqual(before.drafts);
    expect(getSurfaceHost().busy).toBe(before.busy);
    expect(getSurfaceHost().error).toBe(before.error);
    expect(s.status.mock.calls).toEqual(before.status);
    expect(s.deskRoot.textContent).toBe(before.feedback);
    expect(afterSwitch).toEqual([]);
    expect(s.render().querySelector('[data-character-key="alice"]')).toBeNull();
    // Created A copies are retained, not deleted using B's session. A can rediscover
    // or reuse these inputs on reopen; no false association/cleanup is reported.
    if (["acquisition", "import", "association"].includes(boundary)) expect(s.files.size).toBe(1);
    releaseBob(null); await bobOperation;
    if (!reject && ["acquisition", "import"].includes(boundary)) {
      // Reopening A uses the ordinary importer; reselecting the original input
      // reuses the confined copy and associates it without duplicate files.
      s.invoke.mockImplementation(original);
      desk.clearWorkflowDesk(); s.setRoot("/fixture/project");
      await desk.loadWorkflowDesk("/fixture/project");
      expect(getSurfaceHost().state?.assets?.some(asset => asset.path === "Characters/alice/refs/portrait.png")).toBe(true);
      expect(getSurfaceHost().state?.characters[0].reference_asset_ids).toEqual(["alice-photo"]);
      s.failures.preexisting = true;
      s.invoke.mockClear();
      click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Add reference image");
      await s.settle();
      expect(s.files.size).toBe(1);
      expect(getSurfaceHost().state?.characters[0].reference_asset_ids).toEqual(["alice-photo", "alice-uploaded"]);
      expect(s.invoke.mock.calls.some(([cmd]) => cmd === "delete_path")).toBe(false);
    }
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("character_reference_new_picker_opens", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    const newCharacter = s.render().querySelector<HTMLElement>('[data-character-key="__new__"]');
    expect(newCharacter).toBeDefined();

    click(newCharacter!, "Add reference image");
    await s.settle();

    expect(s.picker).toHaveBeenCalledOnce();
    expect(s.picker).toHaveBeenCalledWith({
      title: "Choose character reference image",
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }],
    });
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("creates the profile before acquiring staged inputs and retains the saved selection on reload", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    characterDrafts.set("__new__", { name: "New person", profile: { description: "Keep this" }, references: [{ id: "staged", path: "/external/portrait.png" }] });
    click(s.render(), "Create character");
    await vi.waitFor(() => expect(characterDrafts.has("__new__")).toBe(false));
    expect(s.invoke.mock.calls.map(([cmd]) => cmd)).toEqual(["workflow_save_character", "ingest_character_reference_image", "workflow_import", "workflow_save_character_reference_selection"]);
    expect(s.invoke.mock.calls[1][1]?.characterDir).toBe("Characters/New person");
    expect(s.persisted.state.characters.find(person => person.id === "new-person")?.reference_asset_ids).toEqual(["new-person-uploaded"]);
    await desk.loadWorkflowDesk("/fixture/project");
    expect(s.render().querySelector('[data-character-key="new-person"]')?.textContent).toContain("portrait.png");
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it.each(["workflow_save_character", "ingest_character_reference_image", "workflow_save_character_reference_selection"])("preserves staged inputs and removes only its new unassociated copy on %s failure, then allows retry", async command => {
    const s = await setup();
    setNewCharacterOpen(true);
    const draft = { name: "New person", profile: { description: "Keep this" }, references: [{ id: "staged", path: "/external/portrait.png" }] };
    characterDrafts.set("__new__", draft);
    s.failures.command = command;
    click(s.render(), "Create character");
    await vi.waitFor(() => expect(getSurfaceHost().error).toContain(`Fixture ${command} failure`));
    expect(characterDrafts.get("__new__")?.references).toEqual(draft.references);
    expect(characterDrafts.get("__new__")?.profile.description).toBe("Keep this");
    expect(s.files.size).toBe(0);
    expect(s.persisted.state.characters.every(person => !person.reference_asset_ids?.some(id => id.endsWith("-uploaded")))).toBe(true);
    if (command === "workflow_save_character") expect(s.invoke.mock.calls).toHaveLength(1);
    s.failures.command = "";
    click(s.render(), "Create character");
    await vi.waitFor(() => expect(characterDrafts.has("__new__")).toBe(false));
    expect(s.persisted.state.characters.filter(person => person.name === "New person")).toHaveLength(1);
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("CR04 R1 reselects the original consumed input before retry and preserves a cancelled draft", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    characterDrafts.set("__new__", { name: "New person", profile: { description: "Keep this" }, references: [{ id: "staged", path: "/external/portrait.png" }] });
    s.failures.command = "workflow_save_character_reference_selection";
    click(s.render(), "Create character");
    await s.settle();
    expect(getSurfaceHost().error).toContain("choose the same reference image again");
    const before = structuredClone(characterDrafts.get("__new__"));
    s.failures.command = "";
    s.invoke.mockClear();
    s.picker.mockResolvedValueOnce(null);
    click(s.render(), "Create character");
    await s.settle();
    expect(s.invoke).not.toHaveBeenCalled();
    expect(characterDrafts.get("__new__")).toEqual(before);
    expect(getSurfaceHost().error).toContain("cancelled");
    s.picker.mockResolvedValueOnce("/external/different.png");
    click(s.render(), "Create character");
    await s.settle();
    expect(s.invoke).not.toHaveBeenCalled();
    expect(characterDrafts.get("__new__")).toEqual(before);
    expect(getSurfaceHost().error).toContain("same reference image");
    click(s.render(), "Create character");
    await s.settle();
    expect(characterDrafts.has("__new__")).toBe(false);
    expect(s.persisted.state.characters.filter(person => person.name === "New person")).toHaveLength(1);
  });

  it.each([false, true])("preserves saved selections on association failure (preexisting=%s)", async preexisting => {
    const s = await setup();
    const before = structuredClone(s.persisted.state.characters);
    s.failures.preexisting = preexisting;
    if (preexisting) s.files.add("Characters/alice/refs/portrait.png");
    s.failures.command = "workflow_save_character_reference_selection";
    click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Add reference image");
    await vi.waitFor(() => expect(getSurfaceHost().error).toContain("Fixture workflow_save_character_reference_selection failure"));
    expect(s.persisted.state.characters.map(person => person.reference_asset_ids)).toEqual(before.map(person => person.reference_asset_ids));
    expect(s.persisted.state.characters[1]).toEqual(before[1]);
    expect(s.files.size).toBe(preexisting ? 1 : 0);
    expect(s.invoke.mock.calls.filter(([cmd]) => cmd === "delete_path")).toHaveLength(preexisting ? 0 : 1);
    expect(s.render().querySelector('[data-character-key="alice"]')?.querySelectorAll("figure")).toHaveLength(1);
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("character_reference_saved_picker_cancel", async () => {
    const s = await setup();
    const beforeIds = structuredClone(s.persisted.state.characters[0].reference_asset_ids);
    const beforeGallery = s.render()
      .querySelector<HTMLElement>('[data-character-key="alice"] .workflow-character-reference-gallery')!
      .innerHTML;
    s.picker.mockResolvedValueOnce(null);

    click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Add reference image");
    await s.settle();

    expect(s.picker).toHaveBeenCalledOnce();
    expect(s.invoke).not.toHaveBeenCalled();
    expect(s.persisted.state.characters[0].reference_asset_ids).toEqual(beforeIds);
    expect(getSurfaceHost().state?.characters[0].reference_asset_ids).toEqual(beforeIds);
    expect(s.render()
      .querySelector<HTMLElement>('[data-character-key="alice"] .workflow-character-reference-gallery')!
      .innerHTML).toBe(beforeGallery);
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("preserves saved references on delete/copy failure", async () => {
    const s = await setup();
    const before = structuredClone(s.persisted.state.characters);
    s.failures.command = "ingest_character_reference_image";
    click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Add reference image");
    await vi.waitFor(() => expect(getSurfaceHost().error).toContain("ingest_character_reference_image failure"));
    expect(s.files.size).toBe(0);
    s.failures.command = "workflow_delete_character_reference";
    click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Delete");
    await vi.waitFor(() => expect(getSurfaceHost().error).toContain("workflow_delete_character_reference failure"));
    expect(s.persisted.state.characters).toEqual(before);
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("character_reference_new_picker_cancel", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    characterDrafts.set("__new__", {
      name: "New person",
      profile: { description: "Keep this exact draft", identity_prompt: "Portrait continuity" },
      references: [{ id: "staged", path: "/external/already-staged.png" }],
    });
    const before = JSON.stringify(characterDrafts.get("__new__"));
    s.picker.mockResolvedValueOnce(null);

    click(s.render().querySelector<HTMLElement>('[data-character-key="__new__"]')!, "Add reference image");
    await s.settle();

    expect(s.picker).toHaveBeenCalledOnce();
    expect(JSON.stringify(characterDrafts.get("__new__"))).toBe(before);
    expect(s.invoke).not.toHaveBeenCalled();
  });

  it("character_reference_cancelled_draft_late_picker", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    characterDrafts.set("__new__", { name: "Cancelled person", profile: { description: "Must stay cancelled" } });
    const bobBefore = structuredClone(s.persisted.state.characters[1]);
    let resolvePicker!: (path: string) => void;
    s.picker.mockImplementationOnce(() => new Promise(resolve => { resolvePicker = resolve; }));
    const root = s.render();

    click(root.querySelector<HTMLElement>('[data-character-key="__new__"]')!, "Add reference image");
    click(root, "Cancel new character");
    expect(characterDrafts.has("__new__")).toBe(false);

    setNewCharacterOpen(true);
    const replacement = { name: "Replacement person", profile: { description: "Must remain untouched" } };
    characterDrafts.set("__new__", structuredClone(replacement));
    resolvePicker("/external/late.png");
    await s.settle();

    expect(characterDrafts.get("__new__")).toEqual(replacement);
    expect(characterDrafts.get("__new__")?.references).toBeUndefined();
    expect(s.persisted.state.characters[1]).toEqual(bobBefore);
    expect(getSurfaceHost().state?.characters[1]).toEqual(bobBefore);
    expect(s.files.size).toBe(0);
    expect(s.invoke).not.toHaveBeenCalled();
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("character_reference_draft_remove_one", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    characterDrafts.set("__new__", {
      name: "New person",
      profile: {},
      references: [
        { id: "staged-a", path: "/external/a.png" },
        { id: "staged-b", path: "/external/b.png" },
      ],
    });

    const root = s.render();
    click(root.querySelector<HTMLElement>('[data-character-key="__new__"]')!, "Remove");
    await s.settle();

    expect(characterDrafts.get("__new__")?.references?.map(ref => ref.id)).toEqual(["staged-b"]);
    expect(s.invoke).not.toHaveBeenCalled();
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("deletes a saved uploaded reference through returned state, preview cleanup and Files refresh", async () => {
    const s = await setup();
    const bob = structuredClone(s.persisted.state.characters[1]);
    click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Add reference image");
    await vi.waitFor(() => expect(s.persisted.state.characters[0].reference_asset_ids).toEqual(["alice-photo", "alice-uploaded"]));
    expect(s.invoke.mock.calls.map(([cmd]) => cmd)).toEqual(["ingest_character_reference_image", "workflow_import", "workflow_save_character_reference_selection"]);
    expect(s.invoke.mock.calls[0][1]).toEqual({ projectRoot: "/fixture/project", characterDir: "Characters/alice", sourcePath: "/external/portrait.png", receipt: true });
    let section = s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!;
    expect(section.querySelectorAll("figure")).toHaveLength(2);
    s.filesChanged.mockClear();
    click(section.querySelector<HTMLElement>('[data-asset-id="alice-uploaded"]')!, "Delete");
    await vi.waitFor(() => expect(s.persisted.state.characters[0].reference_asset_ids).toEqual(["alice-photo"]));
    await desk.loadWorkflowDesk("/fixture/project");
    section = s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!;
    expect(section.querySelectorAll("figure")).toHaveLength(1);
    expect(s.files.size).toBe(0);
    expect(s.persisted.state.assets?.some(asset => asset.id === "alice-uploaded")).toBe(false);
    expect(s.trashed).toHaveBeenCalledWith("/fixture/project/Characters/alice/refs/portrait.png");
    expect(s.filesChanged).toHaveBeenCalledOnce();
    expect(s.render().querySelector(".workflow-character-model-sheet-gallery")).toBeNull();
    expect(s.persisted.state.characters[1]).toEqual(bob);
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("keeps mounted Characters and surviving B after the native state-only delete receipt and stage rerender", async () => {
    const s = await setup();
    document.body.replaceChildren(s.deskRoot);
    const stageButton = (label: string) => [...s.deskRoot.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")]
      .find(button => button.querySelector("span:not(.studio-surface-dot)")?.textContent?.trim() === label)!;
    stageButton("Characters").click();
    const assertSurvivor = () => {
      expect(s.deskRoot.querySelector('[data-asset-id="alice-photo"]')).toBeNull();
      expect(s.deskRoot.querySelector('[data-asset-id="bob-photo"]')).not.toBeNull();
      expect(stageButton("Characters")).toBeDefined();
      expect(s.deskRoot.textContent).toContain("Add reference image");
      expect(s.deskRoot.textContent).not.toContain("Open a project folder");
    };
    click(s.deskRoot.querySelector<HTMLElement>('[data-asset-id="alice-photo"]')!, "Delete");
    await vi.waitFor(() => expect(s.invoke).toHaveBeenCalledWith("workflow_delete_character_reference", expect.any(Object)));
    await vi.waitFor(() => expect(getSurfaceHost().busy).toBeNull());
    assertSurvivor();
    const other = [...s.deskRoot.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")]
      .find(button => !button.disabled && button !== stageButton("Characters"))!;
    expect(other).toBeDefined(); other.click();
    stageButton("Characters").click();
    assertSurvivor();
    expect(s.invoke.mock.calls.map(([cmd]) => cmd)).toEqual(["workflow_delete_character_reference"]);
    expect(s.invoke.mock.calls[0][1]).toEqual({ projectRoot: "/fixture/project", characterId: "alice", assetId: "alice-photo" });
    expect(s.trashed).toHaveBeenCalledWith("/fixture/project/Characters/alice/photo.png");
    expect(s.filesChanged).toHaveBeenCalledOnce();
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("cancels saved reference deletion before the host command and leaves it retryable", async () => {
    const s = await setup();
    s.ask.mockResolvedValueOnce(false);
    click(s.render().querySelector<HTMLElement>('[data-character-key="alice"]')!, "Delete");
    await s.settle();
    expect(s.invoke).not.toHaveBeenCalled();
    expect(s.persisted.state.characters[0].reference_asset_ids).toEqual(["alice-photo"]);
    expect(s.render().querySelector('[data-character-key="alice"] button[aria-label^="Delete saved reference"]')).not.toBeNull();
  });

  it("character_reference_draft_stage_visible", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    let root = s.render();
    click(root.querySelector<HTMLElement>('[data-character-key="__new__"]')!, "Add reference image");
    await s.settle();

    root = s.render();
    const staged = root.querySelector<HTMLElement>('[data-character-key="__new__"] figure[data-asset-id="/external/portrait.png"]');
    expect(staged?.querySelector("figcaption")?.textContent).toBe("portrait.png");
    expect(staged?.querySelector<HTMLImageElement>("img")?.alt).toBe("Reference image · portrait.png");
    expect(staged?.querySelector<HTMLImageElement>("img")?.dataset.lookPath).toBe("/external/portrait.png");
    expect(s.invoke).not.toHaveBeenCalled();
  });

  it("stages a native choice without copying and cancels a new draft without an orphan", async () => {
    const s = await setup();
    setNewCharacterOpen(true);
    let root = s.render();
    const name = root.querySelector<HTMLInputElement>("input")!;
    name.value = "New person"; name.dispatchEvent(new Event("input"));
    click(root.querySelector<HTMLElement>('[data-character-key="__new__"]')!, "Add reference image");
    await vi.waitFor(() => expect(characterDrafts.get("__new__")?.references?.map(ref => ref.path)).toEqual(["/external/portrait.png"]));
    expect(characterDrafts.get("__new__")?.name).toBe("New person");
    expect(s.invoke).not.toHaveBeenCalled();
    root = s.render();
    expect(root.querySelector('[data-character-key="__new__"]')?.textContent).toContain("portrait.png");
    click(root, "Cancel new character");
    expect(characterDrafts.has("__new__")).toBe(false);
    expect(s.invoke).not.toHaveBeenCalled(); expect(s.forbidden).not.toHaveBeenCalled();
  });
});

describe("CR04 R1 production host lifecycle", () => {
  it("recovers consumed external grants through the controller picker after native association failure", async ({ onTestFinished }) => {
    // Same ordinary-host protocol/fixture as external-review/reference-retry-probe.py.
    const hostPath = resolve(process.env.WORKBENCH_HOST_PATH ?? "host/target/debug/workbench-host");
    if (!existsSync(hostPath)) throw new Error(`Native workbench host not found at ${hostPath}; build it or set WORKBENCH_HOST_PATH`);
    const base = mkdtempSync(join(tmpdir(), "cr04-r1-"));
    onTestFinished(() => rmSync(base, { recursive: true, force: true }));
    const root = `${base}/project`;
    mkdirSync(root); mkdirSync(`${base}/settings`);
    const source = `${base}/external.png`;
    execFileSync("ffmpeg", ["-v", "error", "-f", "lavfi", "-i", "color=red:s=16x16", "-frames:v", "1", "-threads", "1", source]);
    const bytes = readFileSync(source);
    const host = spawn(hostPath, [], { env: { ...process.env, WORKBENCH_SETTINGS_DIR: `${base}/settings` }, stdio: ["pipe", "pipe", "pipe"] });
    let stderr = ""; host.stderr.on("data", chunk => { stderr += chunk; });
    const pending = new Map<string, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
    const lines = createInterface({ input: host.stdout });
    lines.on("line", line => {
      const response = JSON.parse(line);
      const waiter = pending.get(response.id);
      if (!waiter) throw new Error(`Unexpected native event: ${line}`);
      pending.delete(response.id);
      if (response.ok) waiter.resolve(response.result); else waiter.reject(new Error(response.error));
    });
    const allowed = new Set(["open_project", "close_project", "workflow_import", "workflow_save_character", "grant_path", "ingest_character_reference_image", "workflow_save_character_reference_selection", "delete_path"]);
    let serial = 0;
    const native = (cmd: string, args: Record<string, unknown> = {}): Promise<any> => {
      if (!allowed.has(cmd)) throw new Error(`Unexpected command ${cmd}`);
      const id = String(++serial);
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        host.stdin.write(`${JSON.stringify({ id, cmd, args })}\n`);
      });
    };
    let fail = true;
    let cancelled = false;
    const picker = vi.fn(async () => {
      if (cancelled) return null;
      // Electron's native dialog grants this exact selected path before returning it.
      await native("grant_path", { path: source, intent: "read" });
      return source;
    });
    const invoke = vi.fn(async (cmd: string, args: Record<string, unknown> = {}) => {
      const result = await native(cmd, fail && cmd === "workflow_save_character_reference_selection" ? { ...args, assetIds: ["missing-fixture-id"] } : args);
      return result;
    });
    const forbidden = () => { throw new Error("Unexpected provider/network call"); };
    try {
      await native("open_project", { path: root });
      Object.defineProperty(window, "workbench", { configurable: true, value: { invoke, openDialog: picker } });
      vi.stubGlobal("fetch", forbidden); vi.stubGlobal("WebSocket", forbidden); vi.stubGlobal("XMLHttpRequest", forbidden);
      desk.bindWorkflowDesk({ root: document.createElement("div"), projectRoot: () => root, onOpenPath: forbidden, onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden, onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
      await desk.loadWorkflowDesk(root);
      setNewCharacterOpen(true);
      characterDrafts.set("__new__", { name: "Alice", profile: { description: "Keep this", identity_prompt: "Alice fixture" } });
      let operation = Promise.resolve();
      const render = () => renderCharactersStage(intent => { operation = desk.handleCharacterIntent(intent); });
      click(render().querySelector<HTMLElement>('[data-character-key="__new__"]')!, "Add reference image");
      await operation;
      click(render(), "Create character"); await operation;
      const saved = getSurfaceHost().state!.characters[0];
      const args = { projectRoot: root, characterDir: saved.dir, sourcePath: source, receipt: true };
      expect(getSurfaceHost().error).toBeTruthy();
      expect(existsSync(`${root}/${saved.dir}/refs/external.png`)).toBe(false);
      await expect(native("ingest_character_reference_image", args)).rejects.toThrow("one-time grant");
      const draft = structuredClone(characterDrafts.get("__new__"));
      fail = false; cancelled = true; invoke.mockClear();
      click(render(), "Create character"); await operation;
      expect(invoke).not.toHaveBeenCalled();
      expect(characterDrafts.get("__new__")).toEqual(draft);
      expect(getSurfaceHost().error).toContain("cancelled");
      cancelled = false;
      click(render(), "Create character"); await operation;
      expect(characterDrafts.has("__new__")).toBe(false);
      const reloaded = await native("workflow_import", { projectRoot: root, persist: true }) as WorkflowImportResult;
      expect(reloaded.state.characters).toHaveLength(1);
      expect(reloaded.state.characters[0].id).toBe(saved.id);
      expect(reloaded.state.characters[0].profile?.description).toBe("Keep this");
      expect(reloaded.state.characters[0].reference_asset_ids).toHaveLength(1);
      expect(readFileSync(`${root}/${saved.dir}/refs/external.png`)).toEqual(bytes);
      expect(readFileSync(source)).toEqual(bytes);
      expect(picker).toHaveBeenCalledTimes(3);
      await expect(native("ingest_character_reference_image", args)).rejects.toThrow("one-time grant");
      await native("close_project");
    } finally {
      host.kill(); lines.close();
      desk.clearWorkflowDesk(); resetSurfaceSession(); vi.unstubAllGlobals();
      if (stderr) console.log(stderr);
    }
  }, 20000);
});

describe("CR03 reference controls", () => {
  afterEach(() => { resetSurfaceSession(); vi.unstubAllGlobals(); document.body.innerHTML = ""; });

  it("character_reference_new_control_visible", () => {
    const forbidden = vi.fn(() => { throw new Error("Unexpected effect"); });
    bindSurfaceHost({
      state: { characters: [], assets: [] },
      busy: null,
      error: null,
      resolvePath: (path: string) => path,
      run: forbidden,
      pickFile: forbidden,
      requireChat: forbidden,
    } as unknown as SurfaceHost);
    setNewCharacterOpen(true);

    const root = renderCharactersStage(forbidden);
    const newCard = root.querySelector<HTMLElement>('[data-character-key="__new__"]');
    const controls = [...(newCard?.querySelectorAll<HTMLButtonElement>("button") ?? [])]
      .filter(button => button.textContent === "Add reference image");

    expect(newCard).not.toBeNull();
    expect(controls).toHaveLength(1);
    expect(controls[0].disabled).toBe(false);
    expect(controls[0].hidden).toBe(false);
    expect(forbidden).not.toHaveBeenCalled();
  });

  it("character_reference_saved_control_visible", () => {
    const forbidden = vi.fn(() => { throw new Error("Unexpected effect"); });
    bindSurfaceHost({
      state: { characters: [character("alice")], assets: [] },
      busy: null,
      error: null,
      resolvePath: (path: string) => path,
      run: forbidden,
      pickFile: forbidden,
      requireChat: forbidden,
    } as unknown as SurfaceHost);

    const root = renderCharactersStage(forbidden);
    const savedCard = root.querySelector<HTMLElement>('[data-character-key="alice"]');
    const controls = [...(savedCard?.querySelectorAll<HTMLButtonElement>("button") ?? [])]
      .filter(button => button.textContent === "Add reference image");

    expect(savedCard).not.toBeNull();
    expect(controls).toHaveLength(1);
    expect(controls[0].disabled).toBe(false);
    expect(controls[0].hidden).toBe(false);
    expect(forbidden).not.toHaveBeenCalled();
  });

  it("renders separate new/saved reference controls and emits exact selections without invoking effects", async () => {
    const forbidden = vi.fn(() => { throw new Error("Unexpected host/provider operation"); });
    vi.stubGlobal("fetch", forbidden);
    vi.stubGlobal("XMLHttpRequest", forbidden);
    vi.stubGlobal("WebSocket", forbidden);
    Object.defineProperty(window, "workbench", { configurable: true, value: { invoke: forbidden, openDialog: forbidden } });
    const people = [character("alice"), character("bob")];
    const state = { characters: people, assets: people.map(person => ({ id: `${person.id}-photo`, path: `${person.dir}/photo.png`, role: "identity_reference", notes: "Uploaded photo" })) } as ProjectState;
    const mediaUrl = vi.fn(async (path: string) => `fixture:${path}`);
    bindSurfaceHost({ state, busy: null, error: null, resolvePath: (path: string) => path, mediaUrl, rerender: vi.fn(), run: forbidden, pickFile: forbidden, requireChat: forbidden, hooks: { onOpenPath: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden, onAddToChat: forbidden } } as unknown as SurfaceHost);
    setNewCharacterOpen(true);
    const emit = vi.fn();
    const render = () => { const root = renderCharactersStage(emit); document.body.replaceChildren(root); return root; };
    let root = render();
    const sections = () => [...root.querySelectorAll<HTMLElement>(".workflow-character-references")];
    expect(sections()).toHaveLength(3);
    expect(sections().map(section => section.dataset.characterKey)).toEqual(["__new__", "alice", "bob"]);
    const name = root.querySelector<HTMLInputElement>("input")!;
    name.value = "New person";
    name.dispatchEvent(new Event("input"));
    click(sections()[0], "Add reference image");
    click(sections()[1], "Add reference image");
    click(sections()[2], "Delete");
    expect(emit.mock.calls).toEqual([
      [{ type: "add-reference", characterKey: "__new__", assetIds: [] }],
      [{ type: "add-reference", characterKey: "alice", assetIds: ["alice-photo"] }],
      [{ type: "remove-reference", characterKey: "bob", assetId: "bob-photo", assetIds: [] }],
    ]);
    expect(characterDrafts.get("__new__")?.name).toBe("New person");
    expect(people[1].reference_asset_ids).toEqual(["bob-photo"]);
    root = render();
    expect(root.querySelector<HTMLInputElement>("input")!.value).toBe("New person");
    expect(sections()[1].textContent).toContain("photo.png");
    expect(sections()[1].querySelector("img")?.alt).toBe("Reference image · photo.png");
    expect(sections()[1].querySelector("img")?.dataset.lookPath).toBe("Characters/alice/photo.png");
    expect(root.querySelector(".workflow-character-model-sheet-gallery")).toBeNull();
    expect(sections()[1].querySelector("figcaption")?.textContent).not.toMatch(/generated|model sheet|identity ready/i);
    await Promise.resolve();
    expect(sections()[1].querySelector("img")?.src).toBe("fixture:Characters/alice/photo.png");
    expect(forbidden).not.toHaveBeenCalled();
  });

  it("rerenders staged draft references in order, preserves other selections and disables busy actions", () => {
    const forbidden = vi.fn(() => { throw new Error("Unexpected effect"); });
    const host = { state: { characters: [], assets: [] }, busy: null, error: null, resolvePath: (path: string) => path, mediaUrl: async (path: string) => `fixture:${path}`, run: forbidden, pickFile: forbidden, requireChat: forbidden } as unknown as SurfaceHost;
    bindSurfaceHost(host);
    setNewCharacterOpen(true);
    const references = [{ id: "staged-a", path: "/external/a.png", name: "Portrait A" }, { id: "staged-b", path: "/external/b.jpg" }];
    characterDrafts.set("__new__", { name: "Draft", profile: {}, references });
    const emit = vi.fn();
    let root = renderCharactersStage(emit);
    let section = root.querySelector<HTMLElement>(".workflow-character-references")!;
    expect([...section.querySelectorAll("figcaption")].map(node => node.textContent)).toEqual(["Portrait A", "b.jpg"]);
    click(section, "Remove");
    expect(emit.mock.calls).toEqual([[{ type: "remove-reference", characterKey: "__new__", assetId: "staged-a", assetIds: ["staged-b"] }]]);
    expect(characterDrafts.get("__new__")?.references).toEqual(references);
    characterDrafts.get("__new__")!.references = references.slice(1);
    root = renderCharactersStage(emit);
    section = root.querySelector<HTMLElement>(".workflow-character-references")!;
    expect([...section.querySelectorAll("figcaption")].map(node => node.textContent)).toEqual(["b.jpg"]);
    click(section, "Add reference image");
    expect(emit.mock.calls[1]).toEqual([{ type: "add-reference", characterKey: "__new__", assetIds: ["staged-b"] }]);
    host.busy = "Saving";
    section = renderCharactersStage(emit).querySelector<HTMLElement>(".workflow-character-references")!;
    click(section, "Add reference image");
    click(section, "Remove");
    expect([...section.querySelectorAll("button")].every(button => button.disabled)).toBe(true);
    expect(emit).toHaveBeenCalledTimes(2);
    expect(forbidden).not.toHaveBeenCalled();
  });
});
