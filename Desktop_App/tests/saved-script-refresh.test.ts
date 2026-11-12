import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import * as desk from "../src/workflowDesk";
import { getSurfaceHost } from "../src/studio/surfaces/session";
import { renderSceneIntentView, clearIntentDraft } from "../src/studio/sceneIntentView";
import { buildIntentModel } from "../src/studio/surfaces/surfaceModels";
import type { WorkflowImportResult } from "../src/workflowTypes";
const native = JSON.parse(readFileSync("tests/fixtures/fx02-import-result.json", "utf8")) as WorkflowImportResult;
const root = "/fixture/project", relative = "Scripts/Final authored dialogue.md", path = `${root}/${relative}`;
afterEach(() => { desk.clearWorkflowDesk(); document.body.innerHTML=""; vi.restoreAllMocks(); vi.unstubAllGlobals(); });
async function setup() {
  const container = document.createElement("div"); document.body.append(container);
  const stateMount = document.createElement("section");
  stateMount.id = "studio-state-mount";
  document.body.append(stateMount);
  const invoke = vi.fn(async (_cmd: string, args?: {editedScriptPath?: string}): Promise<WorkflowImportResult> => {
    const result = structuredClone(native);
    if (args?.editedScriptPath) {
      result.persistedPath = `${root}/Preproduction/workflow_state.json`;
      result.editedScript = {path: args.editedScriptPath, stateRevision: 3, sourceSha256: "a".repeat(64), affectedSceneIds: [2]};
    }
    return result;
  });
  vi.stubGlobal("window", {workbench:{invoke}});
  const status = vi.fn();
  desk.bindWorkflowDesk({root:container,projectRoot:()=>root,onOpenPath:vi.fn(),onAddToChat:vi.fn(),onOpenVoice:vi.fn(),onComposeChat:vi.fn(),onSelectionChange:vi.fn(),chatModel:()=>null,onStatus:status});
  await desk.loadWorkflowDesk(root); await desk.refreshScriptSelection(relative); invoke.mockClear(); status.mockClear();
  return {container,invoke,status};
}
it("refresh hides missing script scenes and selects a remaining active scene", async () => {
  const {container,invoke} = await setup();
  const missing = structuredClone(native);
  missing.state.scripts = missing.state.scripts?.filter(script => script.path !== relative);
  for (const scene of missing.state.scenes) if (scene.source_path === relative) scene.source_missing = true;
  invoke.mockResolvedValueOnce(missing);
  await desk.refreshWorkflowDesk();
  expect(getSurfaceHost().state?.scenes.every(scene => !scene.source_missing)).toBe(true);
  expect(container.textContent).not.toContain("Final authored dialogue");
  expect(desk.workflowSelection().sceneId).toBe(1);
});
it("removing the last script returns to Start your script and restoring it recovers scenes", async () => {
  const {container,invoke} = await setup();
  const missing = structuredClone(native);
  missing.state.scripts = [];
  for (const scene of missing.state.scenes) scene.source_missing = true;
  missing.report.scenes = [];
  invoke.mockResolvedValueOnce(missing);
  await desk.refreshWorkflowDesk();
  expect(getSurfaceHost().state?.scenes).toHaveLength(0);
  expect(desk.workflowSelection().sceneId).toBeNull();
  expect(container.textContent).toContain("Start your script");
  invoke.mockResolvedValueOnce(structuredClone(native));
  await desk.refreshWorkflowDesk();
  expect(getSurfaceHost().state?.scenes).toHaveLength(native.state.scenes.length);
});
it("refresh retains source+scene identity, active Script and in-progress new script draft", async () => {
  const {container,invoke} = await setup();
  const click = (label: string) => { const b = [...container.querySelectorAll("button")].find(b=>(b.querySelector('span:not([class])')?.textContent??b.textContent)?.trim()===label); expect(b).toBeDefined(); b!.click(); };
  click("Script"); const before = desk.workflowSelection(); click("Create script");
  const field = container.querySelector<HTMLTextAreaElement>('[name="script"]')!; field.value="Scene 8\nUnfinished new draft"; field.dispatchEvent(new Event("input",{bubbles:true}));
  await desk.refreshSavedScript(path,()=>true);
  expect(container.querySelector<HTMLTextAreaElement>('[name="script"]')?.value).toBe("Scene 8\nUnfinished new draft");
  expect(desk.workflowSelection()).toEqual(before);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("workflow_import",{projectRoot:root,persist:true,editedScriptPath:relative});
  expect(container.textContent).toContain("workflow revision 3");
  expect(container.textContent).toContain("affected scenes: 2");
  expect(container.textContent).toContain("Review current project edit receipt for affected work.");
});
it("shows current source and review/invalidation receipt after save and project reopen without manual refresh",async()=>{
  const {container,invoke} = await setup();
  const changed = structuredClone(native);
  const scene = changed.state.scenes.find(scene=>scene.source_path===relative)!;
  scene.cuts = [{id:{source_scene:scene.id,cut:1},stable_id:"edited-cut",package_needs_review:true,
    production_note_path:"Scenes/Scene2/Cut1/production.md"}];
  changed.state.state_revision = 4;
  changed.state.change_impacts = [
    {revision:2,source:"script:old.md",dependent:"shot:old",disposition:"needs_review"},
    {revision:4,source:`script:${relative}`,dependent:"shot:edited-cut",disposition:"needs_review"},
    {revision:4,source:"upstream_edit",dependent:"master:old-master",disposition:"invalidated"},
    {revision:4,source:"upstream_edit",dependent:"job:prepared",disposition:"invalidated"},
  ];
  changed.state.jobs = [{id:"old-master",kind:"assemble",status:"completed"},{id:"prepared",kind:"render",status:"cancelled"}];
  changed.state.versions = [{id:"v1",asset_id:"paid",version:1,path:"Media/paid.mp4",status:"approved"}];
  changed.persistedPath = `${root}/Preproduction/workflow_state.json`;
  changed.editedScript = {path:relative,stateRevision:4,sourceSha256:"b".repeat(64),affectedSceneIds:[scene.id]};
  invoke.mockResolvedValueOnce(changed);
  await desk.refreshSavedScript(path,()=>true);
  const check = () => {
    const receipt = document.querySelector('[aria-label="Current project edit receipt"]')!;
    expect(receipt.textContent).toContain("Current project state · revision 4");
    expect(receipt.textContent).toContain(`script ${relative}; plan Scenes/Scene2/Cut1/production.md`);
    expect(receipt.textContent).toContain("Latest edit revision 4: 1 review edges, 2 invalidated approvals/jobs");
    expect(receipt.textContent).toContain("master:old-master: invalidated (historical only)");
    expect(receipt.textContent).toContain("1 earlier edit edges, 2 completed/cancelled jobs, 1 saved media versions");
    expect(receipt.textContent).toContain("paid · v1 · Media/paid.mp4 · recorded approved");
    expect(receipt.textContent).not.toContain("shot:old: review needed");
  };
  check();
  desk.clearWorkflowDesk();
  invoke.mockResolvedValueOnce(changed);
  await desk.loadWorkflowDesk(root);
  check();
  expect(invoke).toHaveBeenLastCalledWith("workflow_import",{projectRoot:root,persist:true});
});
it("saving a script does not discard an unsaved Scene Intent working copy", async () => {
  await setup(); clearIntentDraft();
  const scene = getSurfaceHost().state!.scenes.find(s => s.source_path === relative)!;
  const render = () => renderSceneIntentView(scene, buildIntentModel({scene,busy:null,error:null,chatReady:false,blocked:[]}),vi.fn());
  const input = render().querySelector<HTMLTextAreaElement>("textarea")!;
  input.value = "Unrelated unsaved purpose"; input.dispatchEvent(new Event("input",{bubbles:true}));
  await desk.refreshSavedScript(path,()=>true);
  expect(render().querySelector<HTMLTextAreaElement>("textarea")?.value).toBe("Unrelated unsaved purpose");
  clearIntentDraft();
});
it.each(["/outside/Script.md", `${root}/root.md`, `${root}/scripts/lowercase.md`, `${root}/Scripts/movie.pdf`, `${root}/Scripts/Nested/N.md`,`${root}/Scripts/../root.md`])("generic save %s does not import",async generic=>{
  const {invoke} = await setup(); expect(await desk.refreshSavedScript(generic,()=>true)).toBe(false); expect(invoke).not.toHaveBeenCalled();
});
it.each(["clear", "same-root-reopen"])("pending refresh cannot publish after %s",async mode=>{
  const {invoke,status} = await setup();
  let resolve!: (value:WorkflowImportResult)=>void;
  invoke.mockImplementationOnce(()=>new Promise(r=>resolve=r));
  const pending = desk.refreshSavedScript(path,()=>true);
  desk.clearWorkflowDesk();
  if(mode==="same-root-reopen") await desk.loadWorkflowDesk(root);
  const before=structuredClone(getSurfaceHost().state); status.mockClear();
  const stale=structuredClone(native); stale.state.project_name="Stale result"; resolve(stale); await pending;
  expect(getSurfaceHost().state).toEqual(before); expect(status).not.toHaveBeenCalled();
});
it("missing native persistence receipt refuses to publish imported-looking scenes",async()=>{
  const {container,invoke,status} = await setup();
  const before = structuredClone(getSurfaceHost().state);
  const unconfirmed = structuredClone(native);
  unconfirmed.state.scenes.find(scene=>scene.source_path===relative)!.source_excerpt = "Unconfirmed change";
  invoke.mockResolvedValueOnce(unconfirmed);
  await desk.refreshSavedScript(path,()=>true);
  expect(getSurfaceHost().state).toEqual(before);
  expect(container.textContent).toContain("Saved on disk, but Studio was not updated");
  expect(status).toHaveBeenCalledWith("Saved on disk; Studio refresh failed",expect.stringContaining("did not confirm a persisted revision"));
});
it("refresh failure distinguishes disk save from Studio's retained old version",async()=>{
  const {container,invoke,status} = await setup();
  invoke.mockRejectedValueOnce(new Error("No scenes were found. Studio still shows the last imported version."));
  const before=structuredClone(getSurfaceHost().state);
  await desk.refreshSavedScript(path,()=>true);
  expect(getSurfaceHost().state).toEqual(before);
  expect(container.textContent).toContain("Saved on disk, but Studio was not updated");
  expect(status).toHaveBeenCalledWith("Saved on disk; Studio refresh failed", expect.stringContaining("last imported version"));
});
