import { readFileSync, writeFileSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { getSurfaceHost } from '../src/studio/surfaces/session';
const native = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8'));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); localStorage.clear(); });
const flush = async () => { await new Promise(r => setTimeout(r, 0)); };
async function setup(change: (fixture: any) => void = () => {}) {
  let project = '/disposable/sb21/A';
  const fixture = structuredClone(native.initial);
  change(fixture);
  const calls: {cmd: string; args: any}[] = [];
  const handoffs: string[] = [];
  Object.defineProperty(window, 'workbench', {configurable: true, value: {
    invoke: async (cmd: string, args: any) => {
      calls.push({cmd, args: structuredClone(args)});
      if (cmd === 'workflow_import') return structuredClone(fixture);
      if (cmd === 'workflow_generate_script') return {script: 'Scene 1 — Returned local script'};
      if (cmd.startsWith('workflow_save_')) return structuredClone(fixture);
      if (['workflow_draft_sketches','workflow_generate_look','workflow_generate_start_frame','workflow_prepare_render'].includes(cmd)) return {...structuredClone(fixture), pendingAction: `Fresh preparation [[workbench_approve:NEW${calls.length}]]`};
      if (cmd === 'start_character_model_sheet') return {pendingAction: 'Model sheet [[workbench_approve:CHAR1]]'};
      if (cmd === 'director_cancel') return null;
      throw Error('Forbidden '+cmd);
    }, ask: async () => false,
  }});
  const root = document.createElement('main'); document.body.append(root);
  const unused = vi.fn();
  desk.bindWorkflowDesk({root, projectRoot: () => project, onOpenPath: unused, onAddToChat: unused, onOpenVoice: unused, onComposeChat: unused, onSelectionChange: unused, chatModel: () => ({apiId:'fake',modelId:'fake'}), onStatus: unused, onPendingPlan: text => handoffs.push(text)});
  await desk.loadWorkflowDesk(project); calls.length = 0;
  const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === text)!;
  const nav = (text: string) => { const b = [...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b => b.textContent?.includes(text))!; expect(b.disabled).toBe(false); b.click(); };
  const dialog = () => document.querySelector<HTMLDialogElement>('dialog[data-generation-prompt]')!;
  const edit = (text: string) => {const e = dialog().querySelector<HTMLTextAreaElement>('[name=prompt]')!; e.value=text; e.dispatchEvent(new Event('input',{bubbles:true}));};
  return {root,calls,handoffs,fixture,button,nav,dialog,edit,replace: async () => {desk.clearWorkflowDesk(); await desk.loadWorkflowDesk(project);}};
}
it('edits the actual Script action before any host call; cancel sends nothing and duplicate submit sends exact text once', async () => {
  const s = await setup(); s.nav('Script'); s.button('Create script').click();
  const notes=s.root.querySelector<HTMLTextAreaElement>('[name=notes]')!; notes.value='Original notes'; notes.dispatchEvent(new Event('input',{bubbles:true}));
  const send=s.button('Generate script from notes');send.click();send.click();await flush();
  expect(s.dialog(), 'editable prompt must precede host preparation').not.toBeNull();
  expect(s.calls).toHaveLength(0);expect(s.dialog().textContent).toContain('Original notes');
  [...s.dialog().querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === 'Cancel')!.click();await flush();expect(s.calls).toHaveLength(0);
  s.button('Generate script from notes').click();await flush();
  const edited='  Exact <script> direction\nwith spacing — keep it.  ';s.edit(edited);
  const submit=s.button('Prepare request');submit.click();submit.click();await flush();
  expect(s.calls).toEqual([{cmd:'workflow_generate_script',args:{projectRoot:'/disposable/sb21/A',title:'',notes:edited,apiId:'fake',modelId:'fake',replaceExisting:false}}]);
  expect(s.root.querySelector<HTMLTextAreaElement>('[name=notes]')!.value).toBe('Original notes');
});

it('edits look, cut and render inputs at the mounted command seam, cancelling without writes and preparing new IDs after edits', async () => {
  const s = await setup(f => {
    f.state.scenes[0].look_board = {slots:[{id:'room',role:'interior_reference',title:'Room',prompt:'Original room',asset_ids:['asset_18']}]};
    f.state.scenes[0].cuts[0].start_frame_prompt = 'Original cut still';
  });
  const cases = [
    {cmd:'workflow_generate_look',args:{sceneId:1,slotId:'room',role:'interior_reference',prompt:'Original room'},field:'prompt',save:null},
    {cmd:'workflow_draft_sketches',args:{sceneId:1},field:'brief',save:null},
    {cmd:'workflow_generate_start_frame',args:{sceneId:1,cut:1},field:'prompt',save:'workflow_save_start_frame_prompt'},
    {cmd:'workflow_prepare_render',args:{sceneId:1,cut:1},field:'prompt',save:'workflow_save_production_prompt'},
    {cmd:'workflow_prepare_render',args:{sceneId:1,cut:1,selectedPerformance:true},field:'prompt',save:'workflow_save_production_prompt'},
  ];
  const snapshots: unknown[]=[];
  for (const c of cases) {
    const start=s.calls.length;
    const cancelled=getSurfaceHost().run('Prepare fixture',c.cmd,c.args);await flush();
    expect(s.dialog()).not.toBeNull();expect(s.calls).toHaveLength(start);
    expect(s.dialog().textContent).toContain('Preproduction/Scene1/references/interior.png');
    snapshots.push({cmd:c.cmd,context:s.dialog().textContent});
    s.button('Cancel').click();expect(await cancelled).toBe(false);expect(s.calls).toHaveLength(start);
    const p=getSurfaceHost().run('Prepare fixture',c.cmd,c.args);await flush();
    expect(await getSurfaceHost().run('Duplicate fixture',c.cmd,c.args)).toBe(false);
    const text=`  Edited ${c.cmd}\n<exact> — ${start}  `;s.edit(text);
    const submit=s.button('Prepare request');submit.click();submit.click();expect(await p).toBe(true);
    const actual=s.calls.slice(start);expect(actual.map(c=>c.cmd)).toEqual(c.save?[c.save,c.cmd]:[c.cmd]);
    expect(actual[0].args[c.field]).toBe(text);
    expect(actual.every(c=>!('approvalId' in c.args)&&!('id' in c.args))).toBe(true);
    snapshots.push({cmd:c.cmd,edited:text,actual});
    const approval=document.querySelector<HTMLDialogElement>('dialog[data-operation-approval]')!;
    s.handoffs.push(approval.dataset.approvalId!);
    s.button('Deny operation').click();await flush();
  }
  const prior=s.handoffs.at(-1);
  const again=getSurfaceHost().run('Prepare edited render','workflow_prepare_render',{sceneId:1,cut:1});await flush();s.edit('Different render instructions');s.button('Prepare request').click();await again;
  const nextId=document.querySelector<HTMLDialogElement>('dialog[data-operation-approval]')!.dataset.approvalId;
  expect(nextId).not.toBe(prior);
  s.button('Deny operation').click();await flush();
  const start=s.calls.length;const stale=getSurfaceHost().run('Stale','workflow_prepare_render',{sceneId:1,cut:1});await flush();const detached=s.button('Prepare request');await s.replace();detached.click();expect(await stale).toBe(false);
  expect(s.calls.slice(start).map(c=>c.cmd)).toEqual(['workflow_import']);
  if(process.env.SB21_RECEIPT)writeFileSync(process.env.SB21_RECEIPT,JSON.stringify({snapshots,calls:s.calls,handoffs:s.handoffs,providerCalls:0},null,2));
});

it('prepares the character model sheet with edited identity and visible selected references, without approving generation', async () => {
  const s=await setup(f=>{
    f.state.characters=[{id:'char_mara',name:'Mara',dir:'Characters/Mara',profile_path:'Characters/Mara/character.yaml',profile:{identity_prompt:'Saved identity'},reference_asset_ids:['ref_mara']}];
    f.state.assets.push({id:'ref_mara',character_id:'char_mara',path:'Characters/Mara/refs/face.png',role:'identity_reference'});
  });
  const first=desk.handleCreateModelSheet('char_mara');await flush();
  expect(s.dialog()).not.toBeNull();expect(s.dialog().textContent).toContain('Characters/Mara/refs/face.png');expect(s.calls).toHaveLength(0);
  s.button('Cancel').click();await first;expect(s.calls).toHaveLength(0);
  const second=desk.handleCreateModelSheet('char_mara');await flush();s.edit('  Edited exact identity\nkeep scar.  ');
  const submit=s.button('Prepare request');submit.click();submit.click();await second;
  expect(s.calls.map(c=>c.cmd)).toEqual(['start_character_model_sheet','director_cancel']);
  expect(s.calls[0].args.request.inputs.identity_text).toBe('  Edited exact identity\nkeep scar.  ');
  expect(s.calls[0].args.request.inputs.reference_images).toEqual(['Characters/Mara/refs/face.png']);
  expect(s.calls.some(c=>c.cmd==='director_approve')).toBe(false);
});

it('refuses preparation after a failed prompt save or a session replacement during that save', async () => {
  const s=await setup();
  const bridge=(window as any).workbench, original=bridge.invoke;
  bridge.invoke=async(cmd:string,args:any)=>{const result=await original(cmd,args);if(cmd==='workflow_save_production_prompt')throw Error('Fixture prompt write refused');return result;};
  const failed=getSurfaceHost().run('Prepare render','workflow_prepare_render',{sceneId:1,cut:1});await flush();s.edit('Edited render before failed save');s.button('Prepare request').click();
  expect(await failed).toBe(false);expect(s.calls.map(c=>c.cmd)).toEqual(['workflow_save_production_prompt']);
  expect(getSurfaceHost().error).toContain('Fixture prompt write refused');
  let release!:()=>void;const gate=new Promise<void>(r=>release=r);
  bridge.invoke=async(cmd:string,args:any)=>{const result=await original(cmd,args);if(cmd==='workflow_save_production_prompt')await gate;return result;};
  const stale=getSurfaceHost().run('Prepare render','workflow_prepare_render',{sceneId:1,cut:1});await flush();s.edit('Different render for stale save');s.button('Prepare request').click();await flush();
  expect(getSurfaceHost().busy).not.toBeNull();
  await s.replace();release();expect(await stale).toBe(false);
  expect(s.calls.map(c=>c.cmd)).toEqual(['workflow_save_production_prompt','workflow_save_production_prompt','workflow_import']);
  expect(getSurfaceHost().busy).toBeNull();expect(s.handoffs).toHaveLength(0);
});
