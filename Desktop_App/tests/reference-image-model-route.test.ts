import { readFileSync } from 'node:fs';
import { expect, it, vi } from 'vitest';
import { buildSceneLook } from '../src/studio/sceneLookModel';
import { renderSceneLookView } from '../src/studio/sceneLookView';
import type { WorkflowImportResult } from '../src/workflowTypes';

it('carries the chosen configured capability alongside ordered references', () => {
  const f = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8')).initial as WorkflowImportResult;
  f.state.scenes[0].look_board = {slots: [{id:'interior', role:'interior_reference', title:'Interior', prompt:'Room', asset_ids:[]}]};
  f.state.assets = [{id:'a', path:'a.png', role:'interior_reference', status:'draft', scene_id:1}] as typeof f.state.assets;
  const model = buildSceneLook({scene:f.state.scenes[0], report:undefined, assets:f.state.assets, jobs:[], characters:[], blockers:[], focusedSlotId:null, focusedAssetId:null, directorSlotId:null, identityHint:null, busy:null, resolvePath:p=>p ?? null});
  Object.assign(model, {imageRoutes:[{id:'image.text_to_image', label:'Text to image · model-A'}, {id:'image.character_reference', label:'Character reference · model-B'}]});
  const intent = vi.fn(); const root = renderSceneLookView(model, intent, true);
  const route = root.querySelector<HTMLSelectElement>('select[aria-label="Image generation model"]');
  expect(route).not.toBeNull();
  expect(route!.textContent).toContain('model-A');
  route!.value = 'image.text_to_image';
  const refs = root.querySelector<HTMLSelectElement>('select[aria-label="Reference image"]')!;
  refs.value = 'a'; root.querySelector<HTMLButtonElement>('button[aria-label="Add reference image"]')!.click();
  [...root.querySelectorAll('button')].find(b => b.textContent === 'Generate still')!.click();
  expect(intent).toHaveBeenCalledWith(expect.objectContaining({type:'generate', imageCapability:'image.text_to_image', referenceAssetIds:['a']}));
});
