import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as builders from "../src/studio/modelSheetRequest";
// This suite owns saved-input and exact-media approval behavior; SB21 tests prompt editing.
vi.mock('../src/studio/generationPrompt', async () => ({...await vi.importActual('../src/studio/generationPrompt'), editGenerationPrompt: async (spec: {prompt: string}) => spec.prompt}));
import type { AssetRecord, CharacterRecord, WorkflowImportResult } from "../src/workflowTypes";
import { createSpendConfirmationHandler } from "../src/spendConfirmation";
import { readFileSync } from "node:fs";
import * as desk from "../src/workflowDesk";
import { renderCharactersStage } from "../src/studio/surfaces/charactersStage";
import { characterDrafts, getSurfaceHost, resetSurfaceSession, setNewCharacterOpen } from "../src/studio/surfaces/session";

const host = vi.hoisted(() => vi.fn<(...args: any[]) => any>(() => { throw new Error("Unexpected host call"); }));
vi.mock("../src/host", async () => ({ ...await vi.importActual("../src/host"), invoke: host }));
const character: CharacterRecord = {
  id: "char_mara", name: "Mara Vale", dir: "Characters/Mara Vale",
  profile_path: "Characters/Mara Vale/character.yaml", identity_ready: false, voice_ready: false,
  profile: { identity_prompt: "Saved identity" }, reference_asset_ids: [],
};
const image = (id: string, file: string): AssetRecord => ({
  id, path: `Characters/Mara Vale/refs/${file}`, role: "identity_reference", character_id: character.id,
});
const expected = (paths: string[] = [], brief = "  Freckles.\nKeep the scar.  ") => ({
  characterId: "char_mara", action: "run_specialist", specialist: "model_sheet",
  inputs: { character_files: ["Characters/Mara Vale/character.yaml"],
    output_path: "Characters/Mara Vale/model_sheet", character_slug: "Mara Vale",
    identity_text: brief, reference_images: paths },
});

describe("MS04 direct model-sheet button", () => {
  afterEach(() => { desk.clearWorkflowDesk(); resetSurfaceSession(); vi.restoreAllMocks(); host.mockReset(); vi.unstubAllGlobals(); document.body.innerHTML = ""; });
  async function setup() {
    const persisted = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
    persisted.state.characters = [structuredClone(character)]; persisted.state.assets = [];
    const forbidden = vi.fn(() => { throw new Error("Unexpected chat/provider call"); });
    vi.stubGlobal("fetch", forbidden);
    const ask = vi.fn(async (_message: string, _options?: unknown) => false);
    const status = vi.fn();
    Object.defineProperty(window, "workbench", { configurable: true, value: { ask } });
    host.mockImplementation(async (cmd: string) => {
      if (cmd === "workflow_import") return structuredClone(persisted);
      if (cmd === "start_character_model_sheet") return { status: "pendingApproval", pendingAction: "Model sheet [[workbench_approve:ABC123]]", characterId: character.id };
      if (cmd === "director_cancel") return null;
      throw new Error(`Unexpected host command ${cmd}`);
    });
    const root = document.createElement("div"); document.body.append(root);
    desk.bindWorkflowDesk({ root, projectRoot: () => "/fixture/project", onOpenPath: forbidden, onAddToChat: forbidden,
      onOpenVoice: forbidden, onComposeChat: forbidden, onPendingPlan: forbidden, onSelectionChange: vi.fn(), chatModel: () => null, onStatus: status });
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    let pending = Promise.resolve();
    const render = () => { const stage = renderCharactersStage(intent => { pending = desk.handleCharacterIntent(intent); }); root.replaceChildren(stage); return stage; };
    const click = () => {
      const button = [...render().querySelectorAll("button")].find(b => /^(Create|Update) full 12-pose sheet$/.test(b.textContent ?? ""));
      expect(button).toBeDefined(); button!.click();
    };
    return { persisted, forbidden, ask, status, render, click, settle: () => pending, root };
  }
  it("clicks the saved text-only action through MS01 then MS03 and truthfully denies without chat", async () => {
    const s = await setup(); const build = vi.spyOn(builders, "buildModelSheetRequest");
    s.click(); await s.settle();
    expect(build).toHaveBeenCalledOnce();
    expect(host.mock.calls[0]).toEqual(["start_character_model_sheet", { projectRoot: "/fixture/project", request: expected([], "Saved identity") }]);
    expect(build.mock.invocationCallOrder[0]).toBeLessThan(host.mock.invocationCallOrder[0]);
    expect(s.ask).toHaveBeenCalledOnce();
    expect(s.root.textContent).toMatch(/denied|not authorized/i);
    expect(host.mock.calls.map(([cmd]) => cmd)).toEqual(["start_character_model_sheet", "director_cancel"]);
    expect(s.render().textContent).toContain("Create full 12-pose sheet"); expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("offers explicit whole-pack consent without changing the default per-shot path", async () => {
    const s = await setup();
    const fullButton = () => [...s.render().querySelectorAll("button")].find(button => button.textContent === "Generate entire 12-pose sheet (one approval)");
    expect(fullButton()).toBeDefined();
    fullButton()!.click(); await s.settle();
    expect(s.ask.mock.calls[0][0]).toMatch(/up to 12.*image|12.*image/i);
    expect(host).not.toHaveBeenCalled();
    s.ask.mockResolvedValueOnce(true);
    host.mockImplementation(async (cmd: string) => {
      if (cmd === "start_character_model_sheet") return { pendingAction: "Model sheet [[workbench_approve:ABC123]]" };
      if (cmd === "director_approve") return { reply: "Exact media authorization required", toolEvents: ["approve:awaiting-spend-authorization"] };
      throw new Error(`Unexpected ${cmd}`);
    });
    fullButton()!.click(); await s.settle();
    expect(s.ask).toHaveBeenCalledTimes(2);
    expect(host.mock.calls.map(([cmd]) => cmd)).toEqual(["start_character_model_sheet", "director_approve"]);
  });
  it("accepts only one exact model-sheet media request per pose under whole-pack consent", async () => {
    const s = await setup();
    s.ask.mockResolvedValueOnce(true);
    host.mockImplementation(async (cmd: string) => {
      if (cmd === "start_character_model_sheet") return { pendingAction: "Model sheet [[workbench_approve:ABC123]]" };
      if (cmd === "director_approve") return { reply: "Exact media authorization required", toolEvents: ["approve:awaiting-spend-authorization"] };
      throw new Error(`Unexpected ${cmd}`);
    });
    const button = [...s.render().querySelectorAll("button")].find(b => b.textContent === "Generate entire 12-pose sheet (one approval)")!;
    button.click(); await s.settle();
    const owner = [...desk.modelSheetMediaOwners.values()][0];
    expect(owner).toBeDefined();
    const args = (shot: string) => ({ save_path: `/fixture/project/Characters/Mara Vale/model_sheet/mara_${shot}.png`, model: "configured-model" });
    expect(owner.authorizeMedia({ operation: "atlas_generate_image", args: args("closeup_front") })).toBe(true);
    expect(owner.authorizeMedia({ operation: "atlas_generate_image", args: args("closeup_front") })).toBe(false);
    expect(owner.authorizeMedia({ operation: "atlas_generate_video", args: args("closeup_left") })).toBe(false);
    expect(owner.authorizeMedia({ operation: "atlas_generate_image", args: { ...args("closeup_left"), save_path: "/fixture/project/Characters/Other/model_sheet/other_closeup_left.png" } })).toBe(false);
    expect(owner.authorizeMedia({ operation: "atlas_generate_image", args: args("closeup_left") })).toBe(true);
    expect(s.ask).toHaveBeenCalledOnce();
  });
  it("whole-pack mode advances across two exact media calls without repeating consent", async () => {
    const s = await setup();
    s.ask.mockResolvedValueOnce(true);
    let imageCalls = 0;
    const base = host.getMockImplementation()!;
    host.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "director_approve") return { reply: "Exact media authorization required", toolEvents: ["approve:awaiting-spend-authorization"] };
      if (cmd === "confirm_unbounded_spend") return `token-${imageCalls + 1}`;
      if (cmd === "media_generate_confirmed") {
        imageCalls++;
        const shot = image(`saved-${imageCalls}`, `saved-${imageCalls}.png`);
        s.persisted.state.assets!.push(shot);
        s.persisted.state.characters[0].asset_ids = [...(s.persisted.state.characters[0].asset_ids ?? []), shot.id];
        return { mediaResult: "saved", continuation: { reply: imageCalls === 1
          ? "First plate saved. [[workbench_approve:REST123]]" : "Model sheet pack complete (2 saved)." } };
      }
      return base(cmd, args);
    });
    const button = [...s.render().querySelectorAll("button")].find(b => b.textContent === "Generate entire 12-pose sheet (one approval)")!;
    button.click(); await s.settle();
    const ownerId = host.mock.calls.find(([cmd]) => cmd === "director_approve")![1].progressId as string;
    const handler = createSpendConfirmationHandler({ invoke: host, ask: s.ask, projectRoot: () => "/fixture/project",
      ownerIsCurrent: id => Boolean(id && desk.modelSheetMediaOwners.get(id)?.isCurrent()),
      authorizeWorkflowMedia: ({ ownerRequestId, operation, args }) => desk.modelSheetMediaOwners.get(ownerRequestId!)?.authorizeMedia({ operation, args }),
      status: s.status,
      onSubmitted: async (result, _current, id) => { await desk.modelSheetMediaOwners.get(id!)?.settle({ success: true, result }); },
    });
    for (const [shot, hash] of [["closeup_front", "a"], ["closeup_left", "b"]] as const) {
      await handler({ projectRoot: "/fixture/project", provider: "fake", category: "media", operation: "atlas_generate_image",
        model: "fake-model", ownerRequestId: ownerId, workflowContinuationBound: true,
        quote: { currency: "USD", requestHash: hash.repeat(64), ledgerRevision: `fake-${hash}`, disclosure: "Unknown total" },
        args: { model: "fake-model", save_path: `/fixture/project/Characters/Mara Vale/model_sheet/mara_${shot}.png` } });
    }
    expect(s.ask).toHaveBeenCalledOnce();
    expect(host.mock.calls.filter(([cmd]) => cmd === "media_generate_confirmed").map(([, args]) => args.token)).toEqual(["token-1", "token-2"]);
    expect(s.root.textContent).toContain("Model sheet completed");
  });
  it.each(["appearance", "description", "references", "update"])("accepts saved minimal visual input: %s without identity completion", async input => {
    const s = await setup();
    const saved = s.persisted.state.characters[0];
    saved.profile = input === "appearance" ? { appearance: ["Silver bob, blue coat"] }
      : input === "description" ? { description: "Freckled face, red curls" } : {};
    saved.identity_ready = false;
    if (input === "update") saved.model_sheet_manifest_path = `${saved.dir}/model_sheet/model_sheet_manifest.yaml`;
    if (input === "references" || input === "update") {
      const ref = image("visual", "visual.png");
      saved.reference_asset_ids = [ref.id]; s.persisted.state.assets = [ref];
      if (input === "update") saved.asset_ids = [ref.id];
    }
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    s.click(); await s.settle();
    expect(s.ask).toHaveBeenCalledOnce();
    const req = host.mock.calls[0][1].request;
    expect(req.inputs.identity_text).toBe(input === "appearance" ? "Silver bob, blue coat" : input === "description" ? "Freckled face, red curls" : "");
    expect(req.inputs.reference_images).toEqual(input === "references" || input === "update" ? [image("visual", "visual.png").path] : []);
    expect(host.mock.calls.map(([cmd]) => cmd)).toEqual(["start_character_model_sheet", "director_cancel"]);
    expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("requests visual input, not identity completion, when neither text nor selected images exist", async () => {
    const s = await setup(); s.persisted.state.characters[0].profile = { personality: ["Brave"], continuity_notes: ["Episode one"] };
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    s.click(); await s.settle();
    expect(s.root.textContent).toMatch(/visual Description or Appearance, or select a reference image/i);
    expect(host).not.toHaveBeenCalled(); expect(s.ask).not.toHaveBeenCalled();
  });
  it.each(["new", "description", "appearance", "identity_prompt", "references"])("requires saving %s edits before even constructing a request", async field => {
    const s = await setup(); const build = vi.spyOn(builders, "buildModelSheetRequest");
    if (field === "new") { setNewCharacterOpen(true); characterDrafts.set("__new__", { name: "Draft", profile: {} }); }
    else {
      s.render(); const draft = characterDrafts.get(character.id)!;
      if (field === "references") draft.references = [{ id: "staged", path: "/external/new.png" }];
      else if (field === "appearance") draft.profile.appearance = ["changed"];
      else if (field === "identity_prompt") draft.profile.identity_prompt = "Unsaved identity";
      else draft.profile.description = "Unsaved description";
    }
    s.click(); await s.settle();
    expect(build).not.toHaveBeenCalled(); expect(host).not.toHaveBeenCalled();
    expect(s.root.textContent).toMatch(/save required/i); expect(s.ask).not.toHaveBeenCalled(); expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("passes only the current saved image selection and exposes pending approval before consent", async () => {
    const s = await setup(); const a = image("a", "new.png"), b = image("b", "old.png");
    s.persisted.state.characters[0].reference_asset_ids = ["a"];
    s.persisted.state.assets = [b, a];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    let resolve!: (accepted: boolean) => void;
    s.ask.mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    s.click(); await vi.waitFor(() => expect(s.ask).toHaveBeenCalledOnce());
    expect(s.root.textContent).toMatch(/pending approval/i);
    expect(host.mock.calls[0][1].request).toEqual(expected([a.path], "Saved identity"));
    expect(host.mock.calls).toHaveLength(1);
    resolve(false); await s.settle(); expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("rejects stale saved references and MS03 errors visibly without chat or a phantom gallery", async () => {
    const s = await setup(); s.persisted.state.characters[0].reference_asset_ids = ["missing"];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    s.click(); await s.settle(); expect(s.root.textContent).toMatch(/missing or ambiguous/i); expect(host).not.toHaveBeenCalled();
    s.persisted.state.characters[0].reference_asset_ids = [];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    host.mockRejectedValueOnce(new Error("Fixture MS03 error"));
    s.click(); await s.settle(); expect(s.root.textContent).toContain("Fixture MS03 error");
    expect(s.render().querySelector(".workflow-character-model-sheet-gallery")).toBeNull(); expect(s.forbidden).not.toHaveBeenCalled();
  });
  it.each(["completed", "denied", "provider-error", "submitted", "old-output"])("settles actual exact-media confirmation with %s feedback and fake-only gallery refresh", async scenario => {
    const s = await setup();
    const old = { ...image("old", "old.png"), notes: "prior plate" };
    if (scenario === "old-output") {
      s.persisted.state.characters[0].identity_ready = true;
      s.persisted.state.characters[0].asset_ids = [old.id]; s.persisted.state.assets = [old];
      await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
      expect(s.render().textContent).toContain("Update full 12-pose sheet");
    }
    s.ask.mockResolvedValue(true);
    const base = host.getMockImplementation()!;
    host.mockImplementation(async (cmd: string, args: Record<string, any>) => {
      if (cmd === "director_approve") return { reply: "Exact media authorization required", toolEvents: ["approve:awaiting-spend-authorization"] };
      if (cmd === "confirm_unbounded_spend") return "fake-consent-token";
      if (cmd === "media_generate_confirmed") {
        if (scenario === "provider-error") throw new Error("Fixture provider-failed");
        if (scenario === "completed") {
          const output = { ...image("fake-output", "plate.png"), notes: "fake accepted plate" };
          s.persisted.state.assets!.push(output); s.persisted.state.characters[0].asset_ids = [output.id];
          s.persisted.state.characters[0].identity_ready = true;
          return { mediaResult: `Saved to ${output.path}`, continuation: { reply: `Model sheet shot saved to ${output.path}.`, workflowContinuation: true, continueSuggested: false, toolEvents: [] } };
        }
        return { mediaResult: '{"saved":null,"status":"processing"}', continuation: { reply: "Model sheet prediction submitted; continuation collects the same prediction without generation.", workflowContinuation: true, continueSuggested: false, toolEvents: [] } };
      }
      return base(cmd, args);
    });
    s.click(); await s.settle();
    expect(s.root.textContent).toMatch(/pending generation/i);
    const approval = host.mock.calls.find(([cmd]) => cmd === "director_approve")!;
    expect(approval[1].id).toBe("ABC123");
    const ownerId = approval[1].progressId as string;
    const exactAsk = vi.fn(async () => scenario !== "denied");
    const handler = createSpendConfirmationHandler({ invoke: host, ask: exactAsk, projectRoot: () => "/fixture/project",
      ownerIsCurrent: id => Boolean(id && desk.modelSheetMediaOwners.get(id)?.isCurrent()), status: s.status,
      onSubmitted: async (result, _current, id) => { await desk.modelSheetMediaOwners.get(id!)?.settle({ success: true, result }); },
      onSettled: async (outcome, _current, id) => { if (!outcome.success) await desk.modelSheetMediaOwners.get(id!)?.settle(outcome); },
    });
    await handler({ quote: { currency: "USD", requestHash: "a".repeat(64), ledgerRevision: "fake-revision", disclosure: "Fake fixture only" },
      provider: "fake", category: "media", operation: "atlas_generate_image", model: "fake-model", projectRoot: "/fixture/project",
      args: { model: "fake-model" }, ownerRequestId: ownerId, workflowContinuationBound: true });
    expect(exactAsk).toHaveBeenCalledOnce();
    const dispatches = host.mock.calls.filter(([cmd]) => cmd === "media_generate_confirmed");
    expect(dispatches).toHaveLength(scenario === "denied" ? 0 : 1);
    if (dispatches.length) expect(dispatches[0][1].token).toBe("fake-consent-token");
    if (scenario === "completed") {
      expect(s.root.textContent).toContain("Model sheet completed");
      expect(s.render().querySelector(".workflow-character-model-sheet-gallery")?.textContent).toContain("fake accepted plate");
      expect(s.render().textContent).toContain("Update full 12-pose sheet");
    } else if (scenario === "denied" || scenario === "provider-error") {
      expect(s.root.textContent).toMatch(/not authorized|error/i);
      expect(s.render().querySelector(".workflow-character-model-sheet-gallery")).toBeNull();
    } else { expect(s.root.textContent).toContain("pending output"); expect(s.root.textContent).not.toContain("Model sheet completed"); }
    expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("reports an already-complete saved pack as completed without a new provider dispatch", async () => {
    const s = await setup(); const output = { ...image("existing", "saved.png"), notes: "saved plate" };
    s.persisted.state.characters[0].asset_ids = [output.id]; s.persisted.state.characters[0].identity_ready = true; s.persisted.state.assets = [output];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear(); s.ask.mockResolvedValue(true);
    const base = host.getMockImplementation()!;
    host.mockImplementation(async (cmd: string, args: Record<string, unknown>) => cmd === "director_approve"
      ? { reply: "Model sheet pack complete (8 saved).", toolEvents: [], workflowContinuation: true } : base(cmd, args));
    s.click(); await s.settle();
    expect(s.root.textContent).toContain("Model sheet completed");
    expect(host.mock.calls.map(([cmd]) => cmd)).toEqual(["start_character_model_sheet", "director_approve", "workflow_import"]);
    expect(s.render().querySelector(".workflow-character-model-sheet-gallery")?.textContent).toContain("saved plate");
    expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("refreshes the text-only first plate then requires separate rest approval without chat", async () => {
    const s = await setup(); s.ask.mockResolvedValueOnce(true).mockResolvedValueOnce(false);
    const base = host.getMockImplementation()!;
    host.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "director_approve") return { reply: "Exact media authorization required", toolEvents: ["approve:awaiting-spend-authorization"] };
      return base(cmd, args);
    });
    s.click(); await s.settle();
    const id = host.mock.calls.find(([cmd]) => cmd === "director_approve")![1].progressId;
    const output = { ...image("first", "first.png"), notes: "first identity plate" };
    s.persisted.state.assets = [output]; s.persisted.state.characters[0].asset_ids = [output.id];
    await desk.modelSheetMediaOwners.get(id)!.settle({ success: true, result: { mediaResult: "Saved first plate", continuation: {
      reply: "Identity plate saved. Review it before approving the remaining shots. [[workbench_approve:REST123]]", workflowContinuation: true, continueSuggested: false,
    } } });
    expect(s.ask).toHaveBeenCalledTimes(2); expect(s.ask.mock.calls[1][0]).toContain("Review it");
    expect(host.mock.calls.filter(([cmd]) => cmd === "director_approve")).toHaveLength(1);
    expect(s.root.textContent).toMatch(/denied/i); expect(s.root.textContent).not.toContain("Model sheet completed");
    expect(s.render().querySelector(".workflow-character-model-sheet-gallery")?.textContent).toContain("first identity plate");
    expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("does not call approve or publish after the project is cleared while consent is open", async () => {
    const s = await setup(); let resolve!: (accepted: boolean) => void;
    s.ask.mockImplementationOnce(() => new Promise(r => { resolve = r; }));
    s.click(); await vi.waitFor(() => expect(s.ask).toHaveBeenCalledOnce());
    desk.clearWorkflowDesk(); resolve(true); await s.settle();
    expect(host.mock.calls.map(([cmd]) => cmd)).toEqual(["start_character_model_sheet"]);
    expect(desk.modelSheetMediaOwners.size).toBe(0); expect(s.forbidden).not.toHaveBeenCalled();
  });
  it("model_sheet_delete_control_role keeps generated Delete distinct from uploaded-reference Remove", async () => {
    const s = await setup();
    const generated = { ...image("generated", "standing_front.png"), role: "body_reference" as const, notes: "standing_front" };
    const uploaded = { ...image("uploaded", "costume_reference.png"), notes: "uploaded reference image" };
    s.persisted.state.characters[0].asset_ids = [generated.id, uploaded.id];
    s.persisted.state.characters[0].reference_asset_ids = [uploaded.id];
    s.persisted.state.assets = [generated, uploaded];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();

    const emit = vi.fn();
    const stage = renderCharactersStage(emit);
    const generatedCard = stage.querySelector(`.workflow-character-model-sheet-gallery figure`);
    const referenceCard = stage.querySelector(`.workflow-character-reference-gallery figure`);
    expect(generatedCard?.querySelector("button")?.textContent).toBe("Delete");
    expect(generatedCard?.querySelector("button")?.getAttribute("aria-label")).toBe("Delete model-sheet photo standing_front");
    expect(referenceCard?.querySelector("button")?.textContent).toBe("Delete");
    expect(referenceCard?.querySelector("button")?.getAttribute("aria-label")).toBe("Delete saved reference image costume_reference.png from project");

    referenceCard?.querySelector("button")?.click();
    expect(emit).toHaveBeenCalledOnce();
    expect(emit).toHaveBeenCalledWith({
      type: "remove-reference", characterKey: character.id, assetId: uploaded.id, assetIds: [],
    });
    expect(emit).not.toHaveBeenCalledWith(expect.objectContaining({ type: "delete-model-sheet-photo" }));
    expect(host).not.toHaveBeenCalled();
  });

  it("model_sheet_delete_selected_identity emits only the clicked photo's exact owner and asset IDs", async () => {
    const s = await setup();
    const photoA = { ...image("photo-a", "standing_front.png"), role: "body_reference" as const, notes: "standing_front" };
    const photoB = { ...image("photo-b", "standing_left.png"), role: "body_reference" as const, notes: "standing_left" };
    s.persisted.state.characters[0].asset_ids = [photoA.id, photoB.id];
    s.persisted.state.assets = [photoA, photoB];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();

    const emit = vi.fn();
    const stage = renderCharactersStage(emit);
    const deleteA = stage.querySelector<HTMLButtonElement>(
      'button[aria-label="Delete model-sheet photo standing_front"]',
    );
    expect(deleteA).not.toBeNull();
    deleteA!.click();

    expect(emit).toHaveBeenCalledOnce();
    expect(emit).toHaveBeenCalledWith({
      type: "delete-model-sheet-photo", characterKey: character.id, assetId: photoA.id,
    });
    expect(emit).not.toHaveBeenCalledWith(expect.objectContaining({ assetId: photoB.id }));
    expect(host).not.toHaveBeenCalled();
  });

  it("model_sheet_delete_cancel_no_command sends no delete command and leaves the current UI unchanged", async () => {
    const s = await setup();
    const unwanted = { ...image("unwanted", "standing_front.png"), role: "body_reference" as const, notes: "standing_front" };
    s.persisted.state.characters[0].asset_ids = [unwanted.id];
    s.persisted.state.assets = [unwanted];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    s.ask.mockResolvedValueOnce(false);

    const stage = s.render();
    const beforeStage = s.root.firstElementChild;
    const beforeText = s.root.textContent;
    const beforeState = structuredClone(s.persisted);
    const deleteButton = stage.querySelector<HTMLButtonElement>(
      'button[aria-label="Delete model-sheet photo standing_front"]',
    );
    const photoCard = deleteButton?.closest("figure");
    expect(deleteButton).not.toBeNull();
    expect(photoCard).not.toBeNull();
    deleteButton!.click(); await s.settle();

    expect(s.ask).toHaveBeenCalledOnce();
    expect(host).not.toHaveBeenCalled();
    expect(s.root.firstElementChild).toBe(beforeStage);
    expect(s.root.textContent).toBe(beforeText);
    expect(s.persisted).toEqual(beforeState);
    expect(photoCard!.isConnected).toBe(true);
    expect(deleteButton!.isConnected).toBe(true);
    expect(deleteButton!.disabled).toBe(false);
    expect(s.forbidden).not.toHaveBeenCalled();
  });

  it("model_sheet_delete_gallery_immediate removes the target figure and Delete button after success without navigation", async () => {
    const s = await setup();
    const unwanted = { ...image("unwanted", "standing_front.png"), role: "body_reference" as const, notes: "standing_front" };
    s.persisted.state.characters[0].asset_ids = [unwanted.id];
    s.persisted.state.assets = [unwanted];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    host.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd !== "workflow_delete_model_sheet_photo") throw new Error(`Unexpected host command ${cmd}`);
      expect(args).toEqual({ projectRoot: "/fixture/project", characterId: character.id, assetId: unwanted.id });
      s.persisted.state.characters[0].asset_ids = [];
      s.persisted.state.assets = [];
      return structuredClone(s.persisted);
    });
    s.ask.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    const first = [...s.render().querySelectorAll("button")].find(button => button.getAttribute("aria-label") === "Delete model-sheet photo standing_front");
    expect(first).toBeDefined(); first!.click(); await s.settle();
    expect(host).not.toHaveBeenCalled();
    const second = [...s.render().querySelectorAll("button")].find(button => button.getAttribute("aria-label") === "Delete model-sheet photo standing_front");
    const targetFigure = second?.closest("figure");
    expect(second).toBeDefined();
    expect(targetFigure).not.toBeNull();
    second!.click(); await s.settle();
    expect(host).toHaveBeenCalledOnce();
    expect(targetFigure!.isConnected).toBe(false);
    expect(second!.isConnected).toBe(false);
    expect(s.root.querySelector('button[aria-label="Delete model-sheet photo standing_front"]')).toBeNull();
    expect(s.root.querySelector(".workflow-character-model-sheet-gallery figure")).toBeNull();
  });

  it("model_sheet_delete_gallery_persists keeps the target absent after leaving and returning to Characters", async () => {
    const s = await setup();
    const unwanted = { ...image("unwanted", "standing_front.png"), role: "body_reference" as const, notes: "standing_front" };
    s.persisted.state.characters[0].asset_ids = [unwanted.id];
    s.persisted.state.assets = [unwanted];
    await desk.loadWorkflowDesk("/fixture/project"); host.mockClear();
    host.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd !== "workflow_delete_model_sheet_photo") throw new Error(`Unexpected host command ${cmd}`);
      expect(args).toEqual({ projectRoot: "/fixture/project", characterId: character.id, assetId: unwanted.id });
      s.persisted.state.characters[0].asset_ids = [];
      s.persisted.state.assets = [];
      return structuredClone(s.persisted);
    });
    s.ask.mockResolvedValueOnce(true);

    const deleteButton = s.render().querySelector<HTMLButtonElement>(
      'button[aria-label="Delete model-sheet photo standing_front"]',
    );
    expect(deleteButton).not.toBeNull();
    deleteButton!.click(); await s.settle();
    expect(host).toHaveBeenCalledOnce();

    const filesSurface = document.createElement("section");
    filesSurface.dataset.surface = "files";
    s.root.replaceChildren(filesSurface);
    expect(s.root.querySelector('[data-surface="files"]')).not.toBeNull();

    const returnedCharacters = s.render();
    expect(returnedCharacters.querySelector('button[aria-label="Delete model-sheet photo standing_front"]')).toBeNull();
    expect(returnedCharacters.querySelector(".workflow-character-model-sheet-gallery figure")).toBeNull();
  });
});

describe("MS01 typed model-sheet request", () => {
  const provider = vi.fn(() => { throw new Error("Unexpected provider call"); });
  beforeEach(() => { provider.mockClear(); vi.stubGlobal("fetch", provider); });
  afterEach(() => { expect(host).not.toHaveBeenCalled(); expect(provider).not.toHaveBeenCalled(); vi.unstubAllGlobals(); });
  it("uses another saved character's exact profile and output paths", () => {
    const saved = { ...character, id: "char_elin", name: "Elin", dir: "Characters/Elin", profile_path: "Characters/Elin/character.yaml" };
    expect(builders.buildModelSheetRequest(saved, "Silver hair", [])).toEqual({
      characterId: "char_elin", action: "run_specialist", specialist: "model_sheet",
      inputs: { character_files: ["Characters/Elin/character.yaml"], output_path: "Characters/Elin/model_sheet",
        character_slug: "Elin", identity_text: "Silver hair", reference_images: [] },
    });
  });
  it("resolves only saved selections in order and uses changed saved references/brief", () => {
    const a = image("a", "face.png"), b = image("b", "body.jpg"), unused = image("unused", "unused.png");
    const saved = { ...character, reference_asset_ids: ["b", "a"] };
    const before = JSON.stringify({ saved, inventory: [a, unused, b] });
    const first = builders.buildModelSheetRequest(saved, "  Freckles.\nKeep the scar.  ", [a, unused, b]);
    expect(first).toEqual(expected([b.path, a.path]));
    expect(JSON.stringify({ saved, inventory: [a, unused, b] })).toBe(before);
    const second = builders.buildModelSheetRequest({ ...saved, reference_asset_ids: ["a"] }, "New brief", [unused, b, a]);
    expect(second).toEqual(expected([a.path], "New brief"));
    expect(builders.buildModelSheetRequest(character, "New brief", [a, b])).toEqual(expected([], "New brief"));
    if (!("validationError" in first)) { first.inputs.reference_images.push("changed"); }
    expect(builders.buildModelSheetRequest(saved, "  Freckles.\nKeep the scar.  ", [a, b])).toEqual(expected([b.path, a.path]));
  });
  it.each([
    ["missing", null, "brief", [], "character"],
    ["unsaved ID", { ...character, id: "" }, "brief", [], "character"],
    ["missing name", { ...character, name: " " }, "brief", [], "character"],
    ["missing profile", { ...character, profile_path: null }, "brief", [], "profile_path"],
    ["outside profile", { ...character, profile_path: "Characters/Other/character.yaml" }, "brief", [], "profile_path"],
    ["traversal directory", { ...character, dir: "Characters/../Other" }, "brief", [], "dir"],
    ["absolute directory", { ...character, dir: "/Characters/Mara Vale" }, "brief", [], "dir"],
    ["no visual input", { ...character, profile: {} }, " \n ", [], "visualInput"],
    ["missing selected image", { ...character, reference_asset_ids: ["gone"] }, "brief", [], "references"],
    ["duplicate selection", { ...character, reference_asset_ids: ["a", "a"] }, "brief", [image("a", "face.png")], "references"],
    ["ambiguous image", { ...character, reference_asset_ids: ["a"] }, "brief", [image("a", "face.png"), image("a", "other.png")], "references"],
    ["wrong owner", { ...character, reference_asset_ids: ["a"] }, "brief", [{ ...image("a", "face.png"), character_id: "other" }], "references"],
    ["wrong role", { ...character, reference_asset_ids: ["a"] }, "brief", [{ ...image("a", "face.png"), role: "video_take" }], "references"],
    ["outside image", { ...character, reference_asset_ids: ["a"] }, "brief", [{ ...image("a", "face.png"), path: "Characters/Mara Vale2/face.png" }], "references"],
    ["traversal image", { ...character, reference_asset_ids: ["a"] }, "brief", [image("a", "../face.png")], "references"],
    ["wrong extension", { ...character, reference_asset_ids: ["a"] }, "brief", [image("a", "file.txt")], "references"],
    ["backslash image", { ...character, reference_asset_ids: ["a"] }, "brief", [image("a", "sub\\face.png")], "references"],
    ["empty selected ID", { ...character, reference_asset_ids: [""] }, "brief", [], "references"],
  ] as const)("returns a field-specific error: %s", (_label, saved, brief, inventory, field) => {
    const result = builders.buildModelSheetRequest(saved as CharacterRecord | null, brief, inventory);
    expect(result).toMatchObject({ validationError: { field, message: expect.any(String) } });
    if ("validationError" in result) expect(result.validationError.message.trim()).not.toBe("");
  });
  it("includes optional existing visual wording and appearance without mutating or sending biography fields", () => {
    const saved = { ...character, profile: { description: "Freckles", appearance: ["Blue coat"], identity_prompt: "Keep scar",
      personality: ["PRIVATE PERSONALITY"], continuity_notes: ["PRIVATE CONTINUITY"] } };
    const before = JSON.stringify(saved);
    const req = builders.buildModelSheetRequest(saved, saved.profile.identity_prompt, []);
    expect(req).toMatchObject({ inputs: { identity_text: "Keep scar\n\nFreckles\n\nBlue coat" } });
    expect(JSON.stringify(req)).not.toContain("PRIVATE"); expect(JSON.stringify(saved)).toBe(before);
  });
  it("builds the complete text-only request without host, chat or provider calls", () => {
    const fetch = vi.fn(() => { throw new Error("Unexpected provider call"); });
    vi.stubGlobal("fetch", fetch);
    const chat = vi.fn(() => { throw new Error("Unexpected chat call"); });
    const saved = { ...character, onComposeChat: chat };
    expect(builders.buildModelSheetRequest(saved, "  Freckles.\nKeep the scar.  ", [])).toEqual(expected());
    expect(fetch).not.toHaveBeenCalled(); expect(chat).not.toHaveBeenCalled();
  });
});
