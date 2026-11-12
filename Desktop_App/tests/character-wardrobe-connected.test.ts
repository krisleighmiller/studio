import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { renderCharactersStage } from "../src/studio/surfaces/charactersStage";
import { resetSurfaceSession } from "../src/studio/surfaces/session";
import type { WorkflowImportResult } from "../src/workflowTypes";
import { buildSceneLook } from "../src/studio/sceneLookModel";
import { renderSceneLookView } from "../src/studio/sceneLookView";
const host = vi.hoisted(() => ({ invoke: vi.fn(), openDialog: vi.fn(), ask: vi.fn(), toMediaUrlAsync: vi.fn(async () => "data:image/png;base64,") }));
vi.mock("../src/host", () => host);
afterEach(() => { desk.clearWorkflowDesk(); resetSurfaceSession(); vi.clearAllMocks(); document.body.replaceChildren(); });
it("routes the chosen character and wardrobe through the real desk to preparation only", async () => {
  const f = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
  f.state.characters = [{id:"alice", name:"Alice", dir:"Characters/Alice", identity_ready:true, voice_ready:false, profile:{}}];
  const scene = f.state.scenes[0];
  scene.look_board = { slots: [{ id:"wardrobe-slot", role:"wardrobe_reference", title:"Coat", approved_asset_id:"w", asset_ids:["w"] }] };
  f.state.assets = [{ id:"w", path:"coat.png", role:"wardrobe_reference", status:"approved", scene_id:scene.id }];
  host.invoke.mockImplementation(async (command: string) => {
    if (command === "workflow_import" || command === "workflow_generate_wardrobe_plate") return structuredClone(f);
    throw Error(`unexpected command ${command}`);
  });
  const unused = vi.fn();
  const root = document.createElement("main"); document.body.append(root);
  desk.bindWorkflowDesk({root, projectRoot: () => "/fixture/project", onOpenPath: unused, onAddToChat: unused, onOpenVoice: unused, onComposeChat: unused, onSelectionChange: unused, chatModel: () => null, onStatus: unused, onPendingPlan: unused});
  await desk.loadWorkflowDesk("/fixture/project");
  const card = renderCharactersStage(intent => { void desk.handleCharacterIntent(intent); });
  const select = card.querySelector<HTMLSelectElement>('[data-wardrobe-character] select[aria-label="Approved wardrobe for Alice"]')!;
  select.value = `${scene.id}|w`; select.dispatchEvent(new Event("change"));
  [...card.querySelectorAll("button")].find(b => b.textContent === "Build wardrobe plates")!.click();
  await vi.waitFor(() => expect(host.invoke).toHaveBeenCalledWith("workflow_generate_wardrobe_plate", { projectRoot:"/fixture/project", sceneId:scene.id, characterId:"alice", wardrobeAssetId:"w" }));
  expect(host.invoke.mock.calls.map(call => call[0])).toEqual(["workflow_import", "workflow_generate_wardrobe_plate"]);
});

it("does not expose the old plate builder or gallery in Scene Look", () => {
  const f = JSON.parse(readFileSync("tests/fixtures/sc05-import-result.json", "utf8")) as WorkflowImportResult;
  const scene = f.state.scenes[0];
  scene.look_board = {slots: [{id:"wardrobe", role:"wardrobe_reference", title:"Coat", approved_asset_id:"w", asset_ids:["w"]}]};
  f.state.assets = [{id:"w", role:"wardrobe_reference", status:"approved", path:"coat.png", scene_id:scene.id}, {id:"plate", role:"wardrobe_plate", status:"approved", path:"plate.png", scene_id:scene.id, character_id:"alice"}];
  const model = buildSceneLook({scene, report:undefined, assets:f.state.assets, jobs:[], characters:[], blockers:[], focusedSlotId:"wardrobe", focusedAssetId:null, directorSlotId:null, identityHint:null, busy:null, resolvePath:p => p ?? null});
  const view = renderSceneLookView(model, vi.fn());
  expect(view.textContent).not.toContain("Build wardrobe plates");
  expect(view.textContent).not.toContain("Plates (1)");
});
