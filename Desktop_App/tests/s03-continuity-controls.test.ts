import { describe, expect, it } from "vitest";
import { renderStartFrameCard } from "../src/studio/surfaces/referenceStage";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { SceneRecord } from "../src/workflowTypes";
function fixture(selected = true, busy: string | null = null, trimSource?: string) {
 resetSurfaceSession();
 bindSurfaceHost({ busy, error:null, chatReady:true, state:{assets:[]}, resolvePath:(p:string)=>p, mediaUrl:async(p:string)=>p, continuityDependents:()=>[] } as unknown as SurfaceHost);
 const scene={ id:1, title:"S03", cuts:[{id:{source_scene:1,cut:1},takes:[{id:"t",path:"take.mp4",status:"selected"}],selected_take:selected?"t":null},{id:{source_scene:1,cut:2},takes:[],visual_source:{strategy:"previous_selected_take_exit_frame",predecessor:{source_scene:1,cut:1}}}] } as unknown as SceneRecord;
 if (trimSource) scene.animatic = {shots:[{id:"shot1",cut_id:{source_scene:1,cut:1},duration_seconds:5,trim_in_seconds:0,trim_out_seconds:1,trim_source_take_id:trimSource}]} as SceneRecord["animatic"];
 const intents:unknown[]=[];const view=renderStartFrameCard(scene,scene.cuts[1],i=>intents.push(i));
 const video=view.querySelector("video")!;const time=view.querySelector<HTMLInputElement>('input[type="number"]')!;const bind=[...view.querySelectorAll("button")].find(b=>b.textContent==="Bind selected frame")!;
 return {view,video,time,bind,intents};
}
describe("S03 live continuity control state",()=>{
 it("defaults to the edited ending only for its selected source take",()=>{
  const f=fixture(true,null,"t"); Object.defineProperty(f.video,"duration",{value:5}); f.video.dispatchEvent(new Event("loadedmetadata")); expect(f.time.value).toBe("3.900"); f.bind.click(); expect(f.intents).toEqual([{type:"bind-frame",cut:2,predecessorCut:1,predecessorScene:1,frameTimeSeconds:3.9}]);
  const other=fixture(true,null,"old"); Object.defineProperty(other.video,"duration",{value:5}); other.video.dispatchEvent(new Event("loadedmetadata")); expect(other.time.value).toBe("4.900");
 });
 it("enables binding after metadata supplies the default time without toggling another field",()=>{
  const f=fixture();expect(f.bind.disabled).toBe(true);Object.defineProperty(f.video,"duration",{value:2,configurable:true});f.video.dispatchEvent(new Event("loadedmetadata"));expect(f.time.value).toBe("1.900");expect(f.bind.disabled).toBe(false);f.bind.click();expect(f.intents).toEqual([{type:"bind-frame",cut:2,predecessorCut:1,predecessorScene:1,frameTimeSeconds:1.9}]);
 });
 it("enables an explicitly edited valid time and preserves it on metadata",()=>{
  const f=fixture();f.time.value="0.5";f.time.dispatchEvent(new Event("input"));expect(f.bind.disabled).toBe(false);Object.defineProperty(f.video,"duration",{value:2,configurable:true});f.video.dispatchEvent(new Event("loadedmetadata"));expect(f.time.value).toBe("0.5");expect(f.bind.disabled).toBe(false);
 });
 it("does not enable a missing predecessor selection or a busy surface",()=>{
  for(const f of [fixture(false),fixture(true,"Binding…")]) {f.time.value="0.5";f.time.dispatchEvent(new Event("input"));Object.defineProperty(f.video,"duration",{value:2,configurable:true});f.video.dispatchEvent(new Event("loadedmetadata"));expect(f.bind.disabled).toBe(true);}
 });
});

describe("previous scene final take in Cut 1",()=>{
 it("offers only the preceding scene final cut and permits planning before selection",()=>{
  resetSurfaceSession();
  const previous={id:1,title:"Previous",cuts:[
   {id:{source_scene:1,cut:1},takes:[],selected_take:null},
   {id:{source_scene:1,cut:2},takes:[{id:"final",path:"final.mp4",status:"selected"}],selected_take:null},
  ]};
  const next={id:2,title:"Next",cuts:[{id:{source_scene:2,cut:1},takes:[]}]};
  bindSurfaceHost({busy:null,error:null,chatReady:true,state:{scenes:[previous,next],assets:[]},resolvePath:(p:string)=>p,mediaUrl:async(p:string)=>p,continuityDependents:()=>[]} as unknown as SurfaceHost);
  const intents:unknown[]=[];
  const view=renderStartFrameCard(next as unknown as SceneRecord,next.cuts[0] as SceneRecord["cuts"][number],i=>intents.push(i));
  const select=[...view.querySelectorAll("select")].find(el=>el.options[0]?.value==="1:2")!;
  expect([...select.options].map(o=>o.value)).toEqual(["1:2"]);
  const strategy=[...view.querySelectorAll("select")].find(el=>[...el.options].some(o=>o.value==="previous_selected_take_exit_frame"))!;
  strategy.value="previous_selected_take_exit_frame";strategy.dispatchEvent(new Event("change"));
  const save=[...view.querySelectorAll("button")].find(b=>b.textContent==="Save source strategy")!;
  const bind=[...view.querySelectorAll("button")].find(b=>b.textContent==="Bind selected frame")!;
  expect(bind.disabled).toBe(true);
  save.click();expect(intents).toEqual([{type:"set-source",cut:1,strategy:"previous_selected_take_exit_frame",predecessorCut:2,predecessorScene:1,assetId:null}]);
 });
});
