import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost, characterDrafts, setNewCharacterOpen, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { WorkflowImportResult, CharacterRecord } from "../src/workflowTypes";

const fixture = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
const traces: unknown[] = [];
afterEach(() => { desk.clearWorkflowDesk(); resetSurfaceSession(); vi.unstubAllGlobals(); document.body.replaceChildren(); });

// Real desk/controller/renderers; only the async host transport is injected.
// A gate has a bounded wait and is always released: no provider/native submission.
it.each(["workflow_save_character", "ingest_character_reference_image", "workflow_import", "workflow_save_character_reference_selection"].flatMap(boundary =>
  [false, true].map(fail => ({ boundary, fail }))))("settles delayed $boundary (failure=$fail) without duplicate create/save", async ({ boundary, fail }) => {
  const persisted = structuredClone(fixture);
  persisted.state.characters = [];
  persisted.state.assets = [];
  const person: CharacterRecord = { id: "sb06-alice", name: "SB06 Alice", dir: "Characters/SB06 Alice", profile: { description: "Keep this character draft" }, identity_ready: false, voice_ready: false, asset_ids: [], reference_asset_ids: [] };
  let release!: () => void;
  let reached!: () => void;
  const gate = new Promise<void>(r => { release = r; });
  const held = new Promise<void>(r => { reached = r; });
  let armed = false;
  const calls: string[] = [];
  const forbidden = vi.fn(() => { throw new Error("Unexpected network/provider operation"); });
  const invoke = vi.fn(async (cmd: string, args: Record<string, unknown> = {}): Promise<unknown> => {
    if (armed) calls.push(cmd);
    if (armed && cmd === boundary) {
      reached(); await gate;
      if (fail) throw new Error(`SB06 ${boundary} failed: fixture storage unavailable; retry after checking the project folder.`);
    }
    if (cmd === "workflow_save_character") {
      persisted.state.characters = [structuredClone(person)];
      return structuredClone(persisted);
    }
    if (cmd === "ingest_character_reference_image") return { path: `${person.dir}/refs/alice.png`, created: false };
    if (cmd === "workflow_import") {
      if (armed) persisted.state.assets = [{ id: "sb06-ref", path: `${person.dir}/refs/alice.png`, character_id: person.id, role: "identity_reference", notes: "uploaded reference image" }];
      return structuredClone(persisted);
    }
    if (cmd === "workflow_save_character_reference_selection") {
      persisted.state.characters[0].reference_asset_ids = args.assetIds as string[];
      return structuredClone(persisted.state.characters[0]);
    }
    return forbidden();
  });
  Object.defineProperty(window, "workbench", { configurable: true, value: { invoke } });
  vi.stubGlobal("fetch", forbidden); vi.stubGlobal("WebSocket", forbidden); vi.stubGlobal("XMLHttpRequest", forbidden);
  const root = document.createElement("main"); document.body.append(root);
  desk.bindWorkflowDesk({ root, projectRoot: () => "/fixture/sb06", onOpenPath: forbidden, onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden, onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn() });
  await desk.loadWorkflowDesk("/fixture/sb06");
  setNewCharacterOpen(true);
  characterDrafts.set("__new__", { name: person.name, profile: structuredClone(person.profile!), references: [{ id: "external", path: "/external/alice.png" }] });
  armed = true;
  const intent = { type: "save" as const, characterId: null, name: person.name, profile: structuredClone(person.profile!) };
  const charactersTab = [...root.querySelectorAll<HTMLButtonElement>(".studio-surface-nav button")].find(b => b.querySelector("span:not(.studio-surface-dot)")?.textContent === "Characters")!;
  expect(charactersTab.disabled).toBe(false);
  charactersTab.click();
  [...root.querySelectorAll("button")].find(b => b.textContent === "Create character")!.click();
  try {
    await held;
    expect(getSurfaceHost().busy).toBeTruthy();
    expect(root.querySelector(".workflow-banner")?.textContent).toMatch(/Creating character|Updating character references/);
    const create = [...root.querySelectorAll("button")].find(b => b.textContent === "Create character")!;
    expect(create.disabled).toBe(true);
    create.click();
    await desk.handleCharacterIntent(intent); // Duplicate even through direct dispatch must be ignored.
    expect(calls.filter(c => c === boundary)).toHaveLength(1);
    release(); await vi.waitFor(() => expect(getSurfaceHost().busy).toBeNull());
    expect(getSurfaceHost().busy).toBeNull();
    if (fail) {
      expect(root.textContent).toContain(`SB06 ${boundary} failed`);
      expect(root.textContent).toContain("retry after checking the project folder");
      expect(characterDrafts.get("__new__")?.profile.description).toBe("Keep this character draft");
    } else {
      expect(root.textContent).toContain("Character saved.");
      expect(characterDrafts.has("__new__")).toBe(false);
      expect(getSurfaceHost().state?.characters[0].reference_asset_ids).toEqual(["sb06-ref"]);
    }
    expect(calls.filter(c => c === "workflow_save_character")).toHaveLength(1);
    expect(forbidden).not.toHaveBeenCalled();
    traces.push({ boundary, injected_failure: fail, calls, busy_after_settlement: getSurfaceHost().busy, error: getSurfaceHost().error, visible_feedback: root.querySelector(".workflow-desk")?.textContent?.slice(0, 500) ?? root.textContent?.slice(0, 500) });
    mkdirSync("evidence/sb06", { recursive: true });
    writeFileSync("evidence/sb06/settlement-traces.json", JSON.stringify(traces, null, 2));
  } finally { release(); await vi.waitFor(() => expect(getSurfaceHost().busy).toBeNull()); }
}, 5000);
