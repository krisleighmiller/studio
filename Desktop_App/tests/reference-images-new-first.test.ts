import { expect, it, vi } from 'vitest';
import { renderReferenceImagesStage } from '../src/studio/surfaces/referenceStage';
import { bindSurfaceHost } from '../src/studio/surfaces/session';
import type { SceneRecord } from '../src/workflowTypes';

it('shows user-defined creation first even when suggestions exist', () => {
  bindSurfaceHost({busy:false,error:null,blockerNames:()=>[],chatReady:true} as never);
  const scene = {id:1, look_board:{slots:[{id:'suggested',role:'interior_reference',title:'Suggested room',prompt:'Room'}]}} as SceneRecord;
  const emit = vi.fn(); const root = renderReferenceImagesStage(scene, undefined, emit);
  const sections = root.querySelectorAll('section');
  expect(sections[0].querySelector('h3')?.textContent).toBe('New reference image');
  expect(sections[0].querySelector('select')?.value).toBe('user_defined');
  expect(emit).not.toHaveBeenCalled();
});
