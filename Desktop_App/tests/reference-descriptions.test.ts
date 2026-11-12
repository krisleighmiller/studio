import { expect, it, vi } from 'vitest';
import { renderReferenceImagesStage } from '../src/studio/surfaces/referenceStage';
import { bindSurfaceHost } from '../src/studio/surfaces/session';
import type { SceneRecord } from '../src/workflowTypes';

it('edits scene descriptions without cut video or image upload and adds user-defined by default', () => {
  bindSurfaceHost({busy: false, error: null, blockerNames: () => [], chatReady: true} as never);
  const scene = {id: 1, title:'Room', cuts:[{id:{source_scene:1,cut:1}}], look_board:{slots:[{id:'p1',role:'prop_reference',title:'Key',prompt:'Brass key',approved_asset_id:'old'}]}} as unknown as SceneRecord;
  const emit = vi.fn(); const root = renderReferenceImagesStage(scene, undefined, emit);
  expect(root.querySelector('video')).toBeNull();
  expect(root.textContent).not.toContain('Start-frame prompt');
  const areas = root.querySelectorAll('textarea'); expect(areas[1].value).toBe('Brass key');
  areas[1].value='Red key';
  const buttons=[...root.querySelectorAll('button')]; buttons.find(b=>b.textContent==='Save description')!.click();
  expect(emit).toHaveBeenCalledWith({type:'save-description',slotId:'p1',description:'Red key',category:'prop_reference'});
  buttons.find(b=>b.textContent==='Delete look')!.click();
  expect(emit).toHaveBeenCalledWith({type:'delete-look-slot',slotId:'p1',title:'Key'});
  const newArea=areas[0]; newArea.value='Clock'; buttons.find(b=>b.textContent==='Add description')!.click();
  expect(emit).toHaveBeenLastCalledWith({type:'save-description',slotId:null,description:'Clock',category:'user_defined'});
});
