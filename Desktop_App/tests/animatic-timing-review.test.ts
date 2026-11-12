import { afterEach, expect, it, vi } from "vitest";
import { renderAnimatic } from "../src/studio/surfaces/sequenceReview";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { ProjectState, SceneRecord } from "../src/workflowTypes";

afterEach(() => { resetSurfaceSession(); document.body.replaceChildren(); vi.restoreAllMocks(); });

function show(seed?: number, duration = 11.64) {
  const scene = {id:3, dialogue:[{speaker:"Kris",text:"Shorter line now."}], cuts:[{id:{source_scene:3,cut:1},speech_mode:"source_video_lipsync",dialogue:[{speaker:"Kris",text:"Shorter line now."}],selected_take:"take-1",selected_performance_asset_id:"wav-1",takes:[{id:"take-1",path:"Scenes/Scene3/Cut1/take.mp4",status:"selected"}]}],animatic:{id:"a",approved:false,warnings:[],shots:[{id:"shot-1",cut_id:{source_scene:3,cut:1},duration_seconds:duration,auto_duration_seed_seconds:seed}]}} as unknown as SceneRecord;
  const state = {scenes:[scene],assets:[{id:"wav-1",path:"Scenes/Scene3/Cut1/performance.wav",role:"source_audio"}]} as ProjectState;
  const emit = vi.fn();
  bindSurfaceHost({state,busy:null,error:null,resolvePath:(p:string)=>`/project/${p}`,mediaUrl:async(p:string)=>`file://${p}`,rerender:()=>undefined} as unknown as SurfaceHost);
  const section = renderAnimatic(scene,emit); document.body.append(section);
  return {section,emit,state};
}

it("explains legacy timing and bounds an explicit edit to the selected source, without changing state",async()=>{
  const {section,emit,state}=show(); const before=JSON.stringify(state);
  await vi.waitFor(()=>expect(section.querySelector('video[data-timing-source]')?.getAttribute('src')).toContain('take.mp4'));
  const video=section.querySelector<HTMLVideoElement>('video[data-timing-source]')!;
  Object.defineProperty(video,"duration",{value:10.041667}); video.dispatchEvent(new Event("loadedmetadata"));
  const audio=section.querySelector<HTMLAudioElement>('audio[data-timing-performance]')!;
  Object.defineProperty(audio,"duration",{value:5.746}); audio.dispatchEvent(new Event("loadedmetadata"));
  expect(section.textContent).toContain("11.64s");
  expect(section.textContent).toContain("10.04s");
  expect(section.textContent).toContain("5.75s");
  expect(section.textContent).toMatch(/unknown.*not.*manual/i);
  expect(section.textContent).toMatch(/script.*estimate/i);
  expect(section.textContent).toMatch(/exceeds.*source/i);
  const offer=[...section.querySelectorAll('button')].find(b=>b.textContent?.includes('Use bounded'))!;
  expect(offer).toBeDefined(); expect(JSON.stringify(state)).toBe(before); expect(emit).not.toHaveBeenCalled();
  offer.click(); expect(emit).toHaveBeenCalledWith({type:"set-duration",shotId:"shot-1",durationSeconds:10.04});
});

it("labels a fresh auto seed and a deliberate manual override distinctly",()=>{
  const auto=show(2,2); expect(auto.section.textContent).toMatch(/auto.*seed/i);
  resetSurfaceSession(); document.body.replaceChildren();
  const manual=show(undefined,2); expect(manual.section.textContent).toMatch(/unknown.*not.*manual/i);
});

it("does not offer an invalid window when trims exhaust the selected source",async()=>{
  const {section,emit,state}=show(undefined,11.64);
  state.scenes[0].animatic!.shots![0].trim_in_seconds=9.8;
  await vi.waitFor(()=>expect(section.querySelector('video[data-timing-source]')?.getAttribute('src')).toContain('take.mp4'));
  const video=section.querySelector<HTMLVideoElement>('video[data-timing-source]')!;
  Object.defineProperty(video,"duration",{value:10.041667}); video.dispatchEvent(new Event("loadedmetadata"));
  expect(section.querySelector('button[data-bounded-edit]')).toBeNull(); expect(emit).not.toHaveBeenCalled();
});

it("probes an unselected legacy take companion without calling it verified speech",async()=>{
  const {section,state}=show(); state.scenes[0].cuts[0].selected_performance_asset_id=null;
  // Re-render against the legacy state to exercise companion discovery.
  section.remove(); const legacy=renderAnimatic(state.scenes[0],vi.fn()); document.body.append(legacy);
  await vi.waitFor(()=>expect(legacy.querySelector('audio[data-timing-performance]')?.getAttribute('src')).toContain('take.wav'));
  const audio=legacy.querySelector<HTMLAudioElement>('audio[data-timing-performance]')!;
  Object.defineProperty(audio,"duration",{value:6.2807}); audio.dispatchEvent(new Event("loadedmetadata"));
  expect(legacy.textContent).toContain("6.28s"); expect(legacy.textContent).toContain("linkage unverified");
});
