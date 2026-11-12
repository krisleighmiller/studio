import { readFileSync } from 'node:fs';
import { expect, it, vi } from 'vitest';
import { buildSceneLook } from '../src/studio/sceneLookModel';
import { renderSceneLookView } from '../src/studio/sceneLookView';
import type { WorkflowImportResult } from '../src/workflowTypes';

it('keeps a reference unselected until explicitly chosen', () => {
  const f = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8')).initial as WorkflowImportResult;
  const scene = f.state.scenes[0];
  scene.look_board = { slots: [ { id: 'a', role: 'interior_reference', title: 'First', prompt: 'A', asset_ids: ['chosen'] }, { id: 'b', role: 'interior_reference', title: 'Second', prompt: 'B', asset_ids: [] } ] };
  const assets = f.state.assets ?? [];
  assets.push({ id:'chosen', role:'interior_reference', scene_id: scene.id, path:'Scenes/1/references/chosen.png', status:'draft' } as any);
  const model = buildSceneLook({scene, report:undefined, assets, jobs:[], characters:[], blockers:[], focusedSlotId:'b', focusedAssetId:null, directorSlotId:null, identityHint:null, busy:null, resolvePath:p=>p ?? null});
  const intent = vi.fn();
  const root = renderSceneLookView(model, intent, true);
  const select = root.querySelector<HTMLSelectElement>('select[aria-label="Reference image"]')!;
  expect(select.value).toBe('');
  const generate = [...root.querySelectorAll('button')].find(b => b.textContent === 'Generate still')!;
  generate.click();
  expect(intent).toHaveBeenLastCalledWith(expect.objectContaining({type:'generate', referenceAssetIds:[]}));
  select.value = 'chosen';
  root.querySelector<HTMLButtonElement>('button[aria-label="Add reference image"]')!.click();
  generate.click();
  expect(intent).toHaveBeenLastCalledWith(expect.objectContaining({type:'generate', referenceAssetIds:['chosen']}));
});
