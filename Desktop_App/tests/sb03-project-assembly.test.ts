import { readFileSync } from "node:fs";
import vm from "node:vm";
import ts from "typescript";
import { afterEach, expect, it, vi } from "vitest";
import { renderAssembleStage } from "../src/studio/surfaces/assembleStage";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { ProjectState, SceneRecord } from "../src/workflowTypes";

// Exercise the actual lexical controller handler, not a duplicate routing mock.
const source = readFileSync("src/workflowDesk.ts", "utf8");
const ast = ts.createSourceFile("workflowDesk.ts", source, ts.ScriptTarget.Latest, true);
const handler = ast.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === "handleAssembleIntent")!;
const compiled = ts.transpileModule(handler.getText(ast), { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText;
afterEach(() => { resetSurfaceSession(); document.body.innerHTML = ""; });
it("prepares the whole selected project without a scene filter or approval and opens its own completed draft", () => {
  const scenes = [1,2,3].map(id => ({id, stable_id:`scene-${id}`, title:`Scene ${id}`, cuts:[{id:{source_scene:id,cut:1}, stable_id:`shot-${id}`, takes:[{id:`take-${id}`,path:`Scenes/Scene${id}/Cut1/take.mp4`,status:"selected"}],selected_take:`take-${id}`}]})) as SceneRecord[];
  const state = { scenes, assets:[], jobs:[{id:"project-job",kind:"assemble",status:"completed",scope:{kind:"project",id:"project"},raw_provider_id:"project",output_path:"Final/Project_master.mp4"}], profile:{} } as unknown as ProjectState;
  const runAction = vi.fn(); const open = vi.fn();
  const sandbox = vm.createContext({runAction}); vm.runInContext(compiled,sandbox);
  bindSurfaceHost({state,busy:null,error:null,blockerNames:()=>[],reviewExportPath:()=>null,mediaUrl:async (p: string)=>`http://fixture/${p}`,resolvePath:(p: string)=>`/fixture/${p}`,hooks:{onOpenPath:open}} as unknown as SurfaceHost);
  const paint = () => {
    const view = renderAssembleStage(scenes[1],intent => sandbox.handleAssembleIntent(scenes[1],intent));
    document.body.replaceChildren(view); return view;
  };
  const button = (label:string) => [...document.querySelectorAll("button")].find(b=>b.textContent===label)!;
  paint();
  expect(button("Prepare project master"), "project assembly must be reachable alongside scene assembly").toBeDefined();
  expect(button("Prepare project master").disabled).toBe(false);
  button("Prepare project master").click();
  expect(runAction).toHaveBeenLastCalledWith("Preparing project assembly…", "workflow_prepare_assemble", {});
  button("Prepare Scene 2 master").click();
  expect(runAction).toHaveBeenLastCalledWith("Preparing assembly…", "workflow_prepare_assemble", {sceneId:2});
  button("Open project draft").click();
  expect(open).toHaveBeenCalledWith("/fixture/Final/Project_master.mp4");
  expect(runAction.mock.calls.some(call=>String(call[1]).includes("approve"))).toBe(false);
  // Another scene's missing selection disables project assembly, not this scene.
  scenes[2].cuts[0].selected_take=null; paint();
  expect(button("Prepare project master").disabled).toBe(true);
  expect(button("Prepare Scene 2 master").disabled).toBe(false);
  expect(document.body.textContent).toContain("Scene 3 Cut 1");
});
