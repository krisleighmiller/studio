import { afterEach, expect, it, vi } from "vitest";
import { renderVideoTakeEdit } from "../src/studio/surfaces/videoTakeEdit";
import { renderAssembleStage } from "../src/studio/surfaces/assembleStage";
import { renderProductionNoteCard } from "../src/studio/surfaces/notesStage";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { AnimaticShot, TakeRecord } from "../src/workflowTypes";

afterEach(() => { resetSurfaceSession(); document.body.replaceChildren(); });

it("previews original and source-bound window, saves only editorial intent", async () => {
  const state = {scenes: []};
  bindSurfaceHost({state,busy:null,resolvePath:(p:string)=>`/copy/${p}`,mediaUrl:async(p:string)=>`file://${p}`} as unknown as SurfaceHost);
  const shot = {id:"shot-1",cut_id:{source_scene:1,cut:1},duration_seconds:2,trim_in_seconds:0.25,trim_out_seconds:0.25} as AnimaticShot;
  const take = {id:"paid-take-1",path:"Scenes/Scene1/Cut1/source.mp4",status:"selected"} as TakeRecord;
  const emit = vi.fn();
  const view = renderVideoTakeEdit(shot,take,emit); document.body.append(view);
  const videos = view.querySelectorAll("video");
  await vi.waitFor(() => expect(videos[0].src).toContain("source.mp4"));
  expect(videos[1].src).toBe(videos[0].src);
  Object.defineProperty(videos[0],"duration",{value:2}); videos[0].dispatchEvent(new Event("loadedmetadata"));
  const save = [...view.querySelectorAll("button")].find(button => button.textContent === "Save selected-take edit")!;
  expect(save.disabled).toBe(false);
  const inputs = view.querySelectorAll<HTMLInputElement>('input[type="number"]');
  inputs[0].value = "1.9"; inputs[0].dispatchEvent(new Event("input"));
  expect(save.disabled).toBe(true);
  inputs[0].value = "0.5"; inputs[0].dispatchEvent(new Event("input"));
  expect(view.textContent).toContain("0.50s to 2.00s");
  save.click();
  expect(emit).toHaveBeenCalledWith({type:"set-shot-trim",shotId:"shot-1",trimInSeconds:0.5,trimOutSeconds:0,sourceEndSeconds:2,expectedTakeId:"paid-take-1"});
  expect(shot.trim_in_seconds).toBe(0.25);
});

it("edits a selected first take in Production Notes while the next cut is unrendered", async () => {
  const first = {id:{source_scene:1,cut:1},stable_id:"one",title:"First",selected_take:"take-1",takes:[{id:"take-1",path:"first.mp4",status:"selected"}], sketch:{}};
  const second = {id:{source_scene:1,cut:2},stable_id:"two",takes:[]};
  const scene = {id:1,cuts:[first,second],animatic:{shots:[{id:"shot-1",cut_id:first.id,duration_seconds:5}]}};
  const state = {scenes:[scene],assets:[],jobs:[]};
  bindSurfaceHost({state,busy:null,resolvePath:(p:string)=>p,mediaUrl:async(p:string)=>p} as unknown as SurfaceHost);
  const emit = vi.fn();
  const view = renderProductionNoteCard(first as never,emit); document.body.append(view);
  const editor = view.querySelector('section[aria-label="Edit selected video take take-1"]') as HTMLElement;
  expect(editor).not.toBeNull();
  const video = editor.querySelector('video')!;
  Object.defineProperty(video,"duration",{value:5}); video.dispatchEvent(new Event("loadedmetadata"));
  const fields = editor.querySelectorAll<HTMLInputElement>('input[type=number]');
  fields[1].value = "1"; fields[1].dispatchEvent(new Event("input"));
  [...editor.querySelectorAll("button")].find(b => b.textContent === "Save selected-take edit")!.click();
  expect(emit).toHaveBeenCalledWith({type:"set-shot-trim",shotId:"shot-1",trimInSeconds:0,trimOutSeconds:0,sourceEndSeconds:1,expectedTakeId:"take-1"});
});

it("blocks scene and project assembly when an edit belongs to a different selected take", () => {
  const scene = {id:1,cuts:[{id:{source_scene:1,cut:1},stable_id:"stable-1",selected_take:"new",takes:[{id:"new",path:"Scenes/Scene1/Cut1/new.mp4",status:"selected"}]}],animatic:{shots:[{id:"shot-1",cut_id:{source_scene:1,cut:1},duration_seconds:2,trim_source_take_id:"old"}]}};
  const state = {scenes:[scene],jobs:[],assets:[]};
  bindSurfaceHost({state,busy:null,error:null,blockerNames:()=>[],reviewExportPath:()=>null,resolvePath:(p:string)=>p,mediaUrl:async(p:string)=>p} as unknown as SurfaceHost);

  const view = renderAssembleStage(scene as never,vi.fn()); document.body.append(view);
  expect(view.textContent).toContain("Saved edit belongs to old, not selected take new");
  const buttons = [...view.querySelectorAll("button")];
  expect(buttons.find(button => button.textContent === "Prepare Scene 1 master")?.disabled).toBe(true);
  expect(buttons.find(button => button.textContent === "Prepare project master")?.disabled).toBe(true);
});


it("saves five seconds of content plus an additional tenth-second tail", () => {
  const state = {scenes: []};
  bindSurfaceHost({state,busy:null,resolvePath:(p:string)=>p,mediaUrl:async(p:string)=>p} as unknown as SurfaceHost);
  const shot = {id:"shot-1",cut_id:{source_scene:1,cut:1},duration_seconds:5} as AnimaticShot;
  const take = {id:"take-1",path:"take.mp4",status:"selected"} as TakeRecord;
  const emit = vi.fn(); const view = renderVideoTakeEdit(shot,take,emit); document.body.append(view);
  const video = view.querySelector("video")!; Object.defineProperty(video,"duration",{value:10.041667}); video.dispatchEvent(new Event("loadedmetadata"));
  const fields = view.querySelectorAll<HTMLInputElement>('input[type=number]');
  expect(fields.length).toBe(3);
  fields[0].value="0"; fields[1].value="5"; fields[2].value="0.1"; fields[2].dispatchEvent(new Event("input"));
  expect(view.textContent).toContain("0.00s to 5.10s (5.10s)");
  const save = [...view.querySelectorAll("button")].find(b=>b.textContent==="Save selected-take edit")!;
  expect(save.disabled).toBe(false);
  const edited = view.querySelectorAll("video")[1]; const pause = vi.spyOn(edited,"pause").mockImplementation(() => {});
  edited.currentTime = 5.09; edited.dispatchEvent(new Event("timeupdate")); expect(pause).not.toHaveBeenCalled();
  edited.currentTime = 5.1; edited.dispatchEvent(new Event("timeupdate")); expect(pause).toHaveBeenCalled();
  save.click();
  expect(emit).toHaveBeenCalledWith({type:"set-shot-trim",shotId:"shot-1",trimInSeconds:0,trimOutSeconds:0,sourceEndSeconds:5.1,expectedTakeId:"take-1"});
  fields[0].value="5"; fields[0].dispatchEvent(new Event("input")); expect(save.disabled).toBe(true);
});
