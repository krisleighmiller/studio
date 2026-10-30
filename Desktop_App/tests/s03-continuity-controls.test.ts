import { describe, expect, it } from "vitest";
import { renderStartFrameCard } from "../src/studio/surfaces/referenceStage";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { SceneRecord } from "../src/workflowTypes";
function fixture(selected = true, busy: string | null = null) {
 resetSurfaceSession();
 bindSurfaceHost({ busy, error:null, chatReady:true, state:{assets:[]}, resolvePath:(p:string)=>p, mediaUrl:async(p:string)=>p, continuityDependents:()=>[] } as unknown as SurfaceHost);
 const scene={ id:1, title:"S03", cuts:[{id:{source_scene:1,cut:1},takes:[{id:"t",path:"take.mp4",status:"selected"}],selected_take:selected?"t":null},{id:{source_scene:1,cut:2},takes:[],visual_source:{strategy:"previous_selected_take_exit_frame",predecessor:{source_scene:1,cut:1}}}] } as unknown as SceneRecord;
 const intents:unknown[]=[];const view=renderStartFrameCard(scene,scene.cuts[1],i=>intents.push(i));
 const video=view.querySelector("video")!;const time=view.querySelector<HTMLInputElement>('input[type="number"]')!;const bind=[...view.querySelectorAll("button")].find(b=>b.textContent==="Bind selected frame")!;
 return {view,video,time,bind,intents};
}
describe("S03 live continuity control state",()=>{
 it("enables binding after metadata supplies the default time without toggling another field",()=>{
  const f=fixture();expect(f.bind.disabled).toBe(true);Object.defineProperty(f.video,"duration",{value:2,configurable:true});f.video.dispatchEvent(new Event("loadedmetadata"));expect(f.time.value).toBe("1.900");expect(f.bind.disabled).toBe(false);f.bind.click();expect(f.intents).toEqual([{type:"bind-frame",cut:2,predecessorCut:1,frameTimeSeconds:1.9}]);
 });
 it("enables an explicitly edited valid time and preserves it on metadata",()=>{
  const f=fixture();f.time.value="0.5";f.time.dispatchEvent(new Event("input"));expect(f.bind.disabled).toBe(false);Object.defineProperty(f.video,"duration",{value:2,configurable:true});f.video.dispatchEvent(new Event("loadedmetadata"));expect(f.time.value).toBe("0.5");expect(f.bind.disabled).toBe(false);
 });
 it("does not enable a missing predecessor selection or a busy surface",()=>{
  for(const f of [fixture(false),fixture(true,"Binding…")]) {f.time.value="0.5";f.time.dispatchEvent(new Event("input"));Object.defineProperty(f.video,"duration",{value:2,configurable:true});f.video.dispatchEvent(new Event("loadedmetadata"));expect(f.bind.disabled).toBe(true);}
 });
});
