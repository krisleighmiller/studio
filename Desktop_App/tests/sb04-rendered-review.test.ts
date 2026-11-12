import { afterEach, expect, it, vi } from "vitest";
import { renderAssembleStage } from "../src/studio/surfaces/assembleStage";
import { renderAnimatic } from "../src/studio/surfaces/sequenceReview";
import { bindSurfaceHost, resetSurfaceSession } from "../src/studio/surfaces/session";
import type { SurfaceHost } from "../src/studio/surfaces/host";
import type { ProjectState, SceneRecord } from "../src/workflowTypes";
import { readFileSync } from "node:fs";

const receipt = JSON.parse(readFileSync("tests/fixtures/sb04-saved-copy.json", "utf8"));
afterEach(() => { resetSurfaceSession(); document.body.replaceChildren(); vi.restoreAllMocks(); });
it("reviews the saved three-take rendered master, not the sketch approval, including a scene without animatic", async () => {
  const scenes = receipt.saved_job.assembly_snapshot.map((shot: any, i: number) => ({
    id: i + 1, stable_id: `scene-${i+1}`, cuts: [{id:{source_scene:i+1,cut:1},stable_id:shot.shot_id,selected_take:shot.take_id,takes:[{id:shot.take_id,path:shot.path,status:"selected"}]}],
    ...(i === 1 ? { animatic: { shots: [{id:"middle",cut_id:{source_scene:2,cut:1},duration_seconds:shot.duration_seconds,transition:shot.transition}] } } : {}),
  })) as SceneRecord[];
  const state = {scenes, assets:[receipt.saved_asset], jobs:[receipt.saved_job], profile:{}} as ProjectState;
  const before = JSON.stringify(state); const emit = vi.fn(); const mediaUrl = vi.fn(async () => "http://fixture/master.mp4");
  bindSurfaceHost({state,busy:null,error:null,resolvePath:(p: string)=>`/fixture/${p}`,mediaUrl,blockerNames:()=>[],reviewExportPath:()=>null} as unknown as SurfaceHost);
  document.body.append(renderAssembleStage(scenes[2],emit));
  await Promise.resolve();
  const review = document.querySelector('[aria-label="Rendered rough-cut review"]')!;
  expect(review, "Assembly must expose a rendered review player").not.toBeNull();
  const video = review.querySelector("video")!;
  expect(video.controls).toBe(true);
  expect(video.src).toBe("http://fixture/master.mp4");
  expect(mediaUrl).toHaveBeenCalledWith("/fixture/Final/Project_master.mp4");
  expect([...review.querySelectorAll("li")].map(row=>row.dataset.takeId)).toEqual(["asset_27","asset_34","asset_45"]);
  expect(review.textContent).toContain("Scene 3: no sketch animatic");
  expect(review.textContent).toContain("Draft · not a creative approval");
  const play = vi.spyOn(video,"play").mockResolvedValue();
  const pause = vi.spyOn(video,"pause").mockImplementation(()=>undefined);
  const button = review.querySelector("button")!; button.click(); await Promise.resolve();
  expect(play).toHaveBeenCalledOnce();
  video.dispatchEvent(new Event("play")); expect(button.textContent).toBe("Pause rendered rough cut");
  button.click(); expect(pause).toHaveBeenCalledOnce();
  // The discoverable sequence seek control must reach the actual video, not planning time.
  const seek = review.querySelector<HTMLInputElement>('input[aria-label="Seek rendered rough cut"]')!;
  expect(seek).not.toBeNull();
  Object.defineProperty(video, "duration", {value:22.369});
  video.dispatchEvent(new Event("loadedmetadata"));
  expect(seek.max).toBe("22.369");
  seek.value = "13"; seek.dispatchEvent(new Event("input"));
  expect(video.currentTime).toBe(13); expect(emit).not.toHaveBeenCalled();
  expect(JSON.stringify(state)).toBe(before);
  const sketch = renderAnimatic(scenes[2],emit);
  expect(sketch.textContent).toContain("No sketch animatic for Scene 3");
  expect(sketch.querySelector("button")).toBeNull();
  scenes[0].animatic = {approved:true,shots:[],warnings:[]} as any;
  const planning = renderAnimatic(scenes[0],emit);
  expect(planning.textContent).not.toMatch(/Rough cut|rough cut/);
  expect(planning.textContent).toContain("Sketch animatic");
});
