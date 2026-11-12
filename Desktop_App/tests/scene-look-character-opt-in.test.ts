import { readFileSync } from 'node:fs';
import { expect, it, vi } from 'vitest';
import { buildSceneLook } from '../src/studio/sceneLookModel';
import { renderSceneLookView } from '../src/studio/sceneLookView';
import type { WorkflowImportResult } from '../src/workflowTypes';

it('generates without character images by default and opts in only when checked', () => {
  const f = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8')).initial as WorkflowImportResult;
  f.state.scenes[0].look_board = {slots: [{id:'interior', role:'interior_reference', title:'Interior', prompt:'Empty room', asset_ids:[]}]};
  const model = buildSceneLook({scene:f.state.scenes[0], report:undefined, assets:f.state.assets ?? [], jobs:[], characters:[], blockers:[], focusedSlotId:null, focusedAssetId:null, directorSlotId:null, identityHint:null, busy:null, resolvePath:p=>p ?? null});
  const intent = vi.fn();
  const root = renderSceneLookView(model, intent);
  const checkbox = root.querySelector<HTMLInputElement>('input[type=checkbox]');
  expect(checkbox).not.toBeNull();
  expect(checkbox!.checked).toBe(false);
  const generate = [...root.querySelectorAll('button')].find(b => b.textContent === 'Generate still')!;
  generate.click();
  expect(intent).toHaveBeenLastCalledWith(expect.objectContaining({type:'generate', attachCharacterImage:false}));
  checkbox!.checked = true;
  generate.click();
  expect(intent).toHaveBeenLastCalledWith(expect.objectContaining({type:'generate', attachCharacterImage:true}));
});

 it('sends references in the order the user adds them, across scenes', () => {
  const f = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8')).initial as WorkflowImportResult;
  f.state.scenes[0].look_board = {slots: [{id:'interior', role:'interior_reference', title:'Interior', prompt:'Empty room', asset_ids:[]}]};
  f.state.assets = [
    {id:'a', path:'a.png', role:'interior_reference', status:'draft', scene_id:1},
    {id:'b', path:'b.png', role:'interior_reference', status:'draft', scene_id:2},
  ] as typeof f.state.assets;
  const model = buildSceneLook({scene:f.state.scenes[0], report:undefined, assets:f.state.assets, jobs:[], characters:[], blockers:[], focusedSlotId:null, focusedAssetId:null, directorSlotId:null, identityHint:null, busy:null, resolvePath:p=>p ?? null});
  const intent = vi.fn(); const root = renderSceneLookView(model, intent);
  const picker = root.querySelector<HTMLSelectElement>('select[aria-label="Reference image"]')!;
  picker.value = 'b'; root.querySelector<HTMLButtonElement>('button[aria-label="Add reference image"]')!.click();
  picker.value = 'a'; root.querySelector<HTMLButtonElement>('button[aria-label="Add reference image"]')!.click();
  [...root.querySelectorAll('button')].find(b => b.textContent === 'Generate still')!.click();
  expect(intent).toHaveBeenLastCalledWith(expect.objectContaining({referenceAssetIds:['b','a']}));
});
