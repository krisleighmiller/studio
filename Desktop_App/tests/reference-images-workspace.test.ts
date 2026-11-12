import { readFileSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { resetSurfaceSession } from '../src/studio/surfaces/session';
import type { WorkflowImportResult } from '../src/workflowTypes';

const host = vi.hoisted(() => ({invoke: vi.fn(), openDialog: vi.fn(), ask: vi.fn(), toMediaUrlAsync: vi.fn(async () => 'data:image/png;base64,AA==')}));
vi.mock('../src/host', () => host);
afterEach(() => { desk.clearWorkflowDesk(); resetSurfaceSession(); document.body.replaceChildren(); vi.clearAllMocks(); });

it('connects each saved description to image actions and its own versions', async () => {
  const f = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8')).initial as WorkflowImportResult;
  f.state.scenes[0].look_board = {slots: [
    {id:'room', role:'interior_reference', title:'Room', prompt:'Warm room', asset_ids:['one','two']},
    {id:'key', role:'prop_reference', title:'Key', prompt:'Brass key', asset_ids:[]},
  ]};
  f.state.assets = [
    {id:'one', role:'interior_reference', scene_id:1, path:'room-one.png', status:'draft'},
    {id:'two', role:'interior_reference', scene_id:1, path:'room-two.png', status:'draft'},
  ];
  host.invoke.mockImplementation(async (cmd:string) => { if (['workflow_import','workflow_generate_look','workflow_approve_look'].includes(cmd)) return structuredClone(f); throw new Error(`Forbidden ${cmd}`); });
  host.openDialog.mockResolvedValue(null);
  const root = document.createElement('main'); document.body.append(root); const noop = vi.fn();
  desk.bindWorkflowDesk({root, projectRoot:()=>'/fixture/reference', onOpenPath:noop, onAddToChat:noop, onOpenVoice:noop, onComposeChat:noop, onSelectionChange:noop, chatModel:()=>null, onStatus:noop, onPendingPlan:noop});
  await desk.loadWorkflowDesk('/fixture/reference');
  const nav = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.includes('Reference Images'))!;
  expect(nav).toBeDefined(); nav.click();
  const sections = [...root.querySelectorAll<HTMLElement>('.workflow-reference-descriptions > section')];
  const room = sections.find(s=>s.querySelector('h3')?.textContent === 'Room')!;
  const key = sections.find(s=>s.querySelector('h3')?.textContent === 'Key')!;
  expect(room.querySelectorAll('.scene-look-alt')).toHaveLength(2);
  expect(room.querySelector('img[data-look-path$="room-one.png"]')).not.toBeNull();
  expect(key.querySelector('img[data-look-path]')).toBeNull();
  expect(key.textContent).toContain('Generate still');
  const attach = [...key.querySelectorAll('button')].find(b=>b.textContent==='Attach existing image')!;
  expect(attach).toBeDefined(); attach.click();
  await vi.waitFor(()=>expect(host.openDialog).toHaveBeenCalledOnce());
  expect(host.invoke.mock.calls.map(c=>c[0])).toEqual(['workflow_import']);
  room.querySelectorAll<HTMLButtonElement>('button.scene-look-alt')[1].click();
  const roomAfter = [...root.querySelectorAll<HTMLElement>('.workflow-reference-descriptions > section')].find(s=>s.querySelector('h3')?.textContent === 'Room')!;
  expect(roomAfter.querySelector('.scene-look-stage img')?.getAttribute('data-look-path')).toContain('room-two.png');
  [...roomAfter.querySelectorAll('button')].find(b=>b.textContent==='Approve')!.click();
  await vi.waitFor(()=>expect(host.invoke).toHaveBeenCalledWith('workflow_approve_look', expect.objectContaining({sceneId:1,slotId:'room',assetId:'two'})));
  await vi.waitFor(() => expect([...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent==='Generate still')?.disabled).toBe(false));
  const keyAfter = [...root.querySelectorAll<HTMLElement>('.workflow-reference-descriptions > section')].find(s=>s.querySelector('h3')?.textContent === 'Key')!;
  const checkbox = keyAfter.querySelector<HTMLInputElement>('input[type=checkbox]')!;
  expect(checkbox.checked).toBe(false);
  [...keyAfter.querySelectorAll('button')].find(b=>b.textContent==='Generate still')!.click();
  const dialog = document.querySelector<HTMLDialogElement>('dialog[data-generation-prompt]')!;
  expect(dialog.querySelector('textarea')?.value).toBe('Brass key');
  dialog.querySelector('form')!.dispatchEvent(new Event('submit', {bubbles:true,cancelable:true}));
  await vi.waitFor(()=>expect(host.invoke).toHaveBeenCalledWith('workflow_generate_look', expect.objectContaining({sceneId:1,slotId:'key',prompt:'Brass key',attachCharacterImage:false})));
  const currentKey = [...root.querySelectorAll<HTMLElement>('.workflow-reference-descriptions > section')].find(s=>s.querySelector('h3')?.textContent === 'Key')!;
  const description = currentKey.querySelector<HTMLTextAreaElement>('textarea')!;
  description.value = 'Silver key'; description.dispatchEvent(new Event('input', {bubbles:true}));
  expect(currentKey.querySelector('fieldset')!.disabled).toBe(true);
  expect(currentKey.textContent).toContain('Save description');
});
