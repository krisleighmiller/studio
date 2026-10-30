import { afterEach, describe, expect, it } from "vitest";
import { buildFinishModel } from "../src/studio/finishModel";
import { renderFinishView } from "../src/studio/finishView";
import type { SceneRecord, ProjectState, AssemblyShotSnapshot } from "../src/workflowTypes";

const snap = (i: number): AssemblyShotSnapshot => ({ shot_id: `shot-${i}`, cut_label: i, take_id: `take-${i}`, path: `Scenes/Scene1/Cut${i}/take.mp4`, start_seconds: 0.25, duration_seconds: 1.5, transition: i === 2 ? "dissolve" : "cut" });
function fixture(): SceneRecord {
 const shots = [snap(2), snap(1)];
 return { id: 1, stable_id: "scene-1", title: "Parity", dialogue: [], cuts: [1,2].map(i => ({ id: {source_scene:1,cut:i}, stable_id:`shot-${i}`, takes:[{id:`take-${i}`,path:`Scenes/Scene1/Cut${i}/take.mp4`,status:"used_in_assembly"}],selected_take:`take-${i}`,dialogue:[] })), animatic: {id:"anim",approved:true,shots:[2,1].map(i => ({id:`anim-${i}`,cut_id:{source_scene:1,cut:i},duration_seconds:2,trim_in_seconds:0.25,trim_out_seconds:0.25,transition:i===2?"dissolve":"cut"}))}, post: { versions:[{id:"v1",label:"Assembly 1",stage:"picture_lock",assembly_path:"Final/master.mp4",shots:shots.map(({shot_id,cut_label,take_id,path})=>({shot_id,cut_label,take_id,path})),assembly_snapshot:shots}],picture_locked_id:"v1",editorial_path:"host_ffmpeg",audio_mix:"approved",vfx:"approved",compositing:"approved",captions:"approved",mastering:"approved",qc:"approved",last_check:{deliverable_id:"review",version_id:"v1",passed:true,resolution_ok:true,aspect_ok:true,missing_shots:[],messages:["Actual media checked."]}} } as SceneRecord;
}
function mount(scene: SceneRecord) {
 const state = {scenes:[scene],assets:[],requirements:[],jobs:[],characters:[],settings:{}} as unknown as ProjectState;
 const model=buildFinishModel({scene,state,jobs:[],busyReason:null,deliverables:[{id:"review",name:"Review"}]});
 const view=renderFinishView(model,()=>{});document.body.replaceChildren(view);return {model,view,qc: model.items.find(i=>i.id==="qc")!};
}
afterEach(()=>document.body.replaceChildren());
describe("S07 mounted Finish never certifies stale or incomplete assembly",()=>{
 it("shows unchanged native ordered snapshot Complete",()=>{const {model,view,qc}=mount(fixture());expect(qc.state).toBe("complete");expect(model.headline).toBe("Complete");expect(view.querySelector('.studio-finish-scope')?.textContent).toContain("Complete");});
 const mutations: Array<[string,(s:SceneRecord)=>void]> = [
 ["changed selected take id with same path",s=>{s.cuts[0].takes[0].id="changed-id";s.cuts[0].selected_take="changed-id";}],
 ["changed selected path with same id",s=>{s.cuts[0].takes[0].path="Scenes/Scene1/Cut1/new.mp4";}],
 ["stable identity mismatch despite same cut label",s=>{s.cuts[0].stable_id="new-shot";}],
 ["missing assembly path",s=>{delete s.post!.versions[0].assembly_path;}],
 ["missing checked version",s=>{s.post!.versions=[];}],
 ["missing version id on historical passed check",s=>{s.post!.last_check!.version_id=null;}],
 ["empty scene and version",s=>{s.cuts=[];s.post!.versions[0].shots=[];s.post!.versions[0].assembly_snapshot=[];}],
 ["duplicate version claims",s=>{s.post!.versions[0].shots=[s.post!.versions[0].shots[0],s.post!.versions[0].shots[0]];}],
 ["extra version shot",s=>{s.post!.versions[0].shots.push({...s.post!.versions[0].shots[0],shot_id:"extra"});}],
 ["missing timing provenance",s=>{delete s.post!.versions[0].assembly_snapshot;}],
 ["changed animatic order",s=>{s.animatic!.shots!.reverse();}],
 ["changed animatic duration",s=>{s.animatic!.shots![0].duration_seconds=1.75;}],
 ["changed head trim",s=>{s.animatic!.shots![0].trim_in_seconds=0.1;}],
 ["changed transition",s=>{s.animatic!.shots![0].transition="fade_through_black";}],
 ["newer unchecked version without lock",s=>{s.post!.picture_locked_id=null;s.post!.versions.push({...s.post!.versions[0],id:"v2",label:"New unchecked"});}],
 ];
 for(const [name,change] of mutations)it(name,()=>{const s=fixture();change(s);const {model,view,qc}=mount(s);expect(qc.state).not.toBe("complete");expect(model.headline).not.toBe("Complete");expect(view.querySelector('.studio-finish-scope')?.textContent).not.toContain("· Complete");expect(view.querySelector('.studio-finish-list')?.textContent).toContain(qc.stateLabel);});
 it("renders failed actual-media QC Blocked with actionable finding",()=>{const s=fixture();s.post!.last_check!.passed=false;s.post!.last_check!.media_findings=[{id:"decodes",severity:"error",check:"Decode",message:"Artifact missing"}];const {view,model}=mount(s);expect(model.headline).toBe("Blocked");expect(view.textContent).toContain("Artifact missing");});
});
