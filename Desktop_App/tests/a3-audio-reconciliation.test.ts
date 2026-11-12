import { afterEach, expect, it, vi } from 'vitest';
import { renderPerformanceAudio } from '../src/studio/surfaces/performanceAudio';
import { bindSurfaceHost, resetSurfaceSession } from '../src/studio/surfaces/session';
import type { SurfaceHost } from '../src/studio/surfaces/host';
import type { CutRecord, ProjectState } from '../src/workflowTypes';

afterEach(() => { resetSurfaceSession(); document.body.replaceChildren(); });

it('shows selected derivative duration delta and requires new full-dialogue review for selection and preparation', async () => {
  const cut = {id:{source_scene:1,cut:1},selected_performance_asset_id:'new-wav',dialogue:[{speaker:'Actor',text:'All words here.'}],speech_mode:'source_video_lipsync'} as CutRecord;
  const state = {assets:[
    {id:'old-wav',path:'Scenes/Scene1/Cut1/old.wav',role:'source_audio'},
    {id:'new-wav',path:'Scenes/Scene1/Cut1/trimmed.wav',role:'source_audio',parent_asset_id:'old-wav'},
  ],scenes:[{id:1,cuts:[cut],animatic:{shots:[{id:'shot-1',cut_id:cut.id,duration_seconds:4}]}}]} as ProjectState;
  const emit = vi.fn();
  bindSurfaceHost({state,busy:null,error:null,resolvePath:(p:string)=>`/copy/${p}`,mediaUrl:async(p:string)=>`file://${p}`} as unknown as SurfaceHost);
  const section = renderPerformanceAudio(cut,emit); document.body.append(section);
  expect(section.textContent).toContain('Actor: “All words here.”');
  const selected = section.querySelector<HTMLAudioElement>('audio[data-performance-path="Scenes/Scene1/Cut1/trimmed.wav"]')!;
  await vi.waitFor(() => expect(selected.src).toContain('trimmed.wav'));
  Object.defineProperty(selected,'duration',{value:5.75});selected.dispatchEvent(new Event('loadedmetadata'));
  expect(section.textContent).toContain('WAV longer by 1.75s');
  expect(section.textContent).toContain('no automatic retiming or transcript verification');
  const prepare = [...section.querySelectorAll('button')].find(b => b.textContent === 'Prepare video only with selected WAV')!;
  expect(prepare.disabled).toBe(true);
  const selectedReview = selected.closest('figure')!.querySelector<HTMLInputElement>('input[type=checkbox]')!;
  selectedReview.click(); expect(prepare.disabled).toBe(false); prepare.click();
  expect(emit).toHaveBeenCalledWith({type:'prepare-performance',cut:1,reviewedAssetId:'new-wav'});
  const old = section.querySelector<HTMLButtonElement>('button[data-select-performance="old-wav"]')!;
  expect(old.disabled).toBe(true);
  old.closest('figure')!.querySelector<HTMLInputElement>('input[type=checkbox]')!.click();old.click();
  expect(emit).toHaveBeenCalledWith({type:'select-performance',cut:1,assetId:'old-wav',reviewedAssetId:'old-wav'});
  expect(state.scenes[0].cuts[0].selected_performance_asset_id).toBe('new-wav');
});
