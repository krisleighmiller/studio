import {readFileSync} from 'node:fs';
import {afterEach,expect,it,vi} from 'vitest';
import * as desk from '../src/workflowDesk';
import {resetSurfaceSession} from '../src/studio/surfaces/session';
import type {WorkflowImportResult} from '../src/workflowTypes';
const host=vi.hoisted(()=>({invoke:vi.fn(),openDialog:vi.fn(),ask:vi.fn(),toMediaUrlAsync:vi.fn(async()=> 'data:image/png;base64,')}));
vi.mock('../src/host',()=>host);
const baseline=JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json','utf8')).initial as WorkflowImportResult;
function fixture(){ const f=structuredClone(baseline); f.state.scenes[0].look_board={slots:[]};const nav=f.report.scenes[0].navigation!.find(n=>n.id==='scene_look')!;nav.available=true;nav.disabled_reason=null;return f; }
function button(label:string){return [...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===label)!;}
async function mount(f:WorkflowImportResult){host.invoke.mockImplementation(async(cmd:string)=>{if(cmd==='workflow_import')return structuredClone(f);throw Error(cmd);});const root=document.createElement('main');document.body.append(root);const noop=vi.fn();desk.bindWorkflowDesk({root,projectRoot:()=>'/fixture/sb14',onOpenPath:noop,onAddToChat:noop,onOpenVoice:noop,onComposeChat:noop,onSelectionChange:noop,chatModel:()=>null,onStatus:noop,onPendingPlan:noop});await desk.loadWorkflowDesk('/fixture/sb14');[...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Scene Look'))!.click();return root;}
afterEach(()=>{desk.clearWorkflowDesk();resetSurfaceSession();document.body.replaceChildren();vi.resetAllMocks();});
it('offers local Attach on no-slot board, selects copied output and never routes through Jobs or generation',async()=>{
 const f=fixture(); const root=await mount(f);const attach=button('Attach existing image');expect(attach).toBeDefined();expect(attach.closest('details:not([open])')).toBeNull();expect(root.textContent).toContain('not a cut start frame');
 const role=root.querySelector<HTMLSelectElement>('[aria-label="New look slot role"]')!;role.value='mood_reference';
 const after=structuredClone(f);after.state.scenes[0].look_board={slots:[{id:'s1_look_01',role:'mood_reference',title:'Mood',asset_ids:['asset_local']}]};after.state.assets!.push({id:'asset_local',path:'Preproduction/Scene1/references/mood_1.png',role:'mood_reference',status:'draft',scene_id:1});
 host.openDialog.mockResolvedValue('/external/photo.png');host.invoke.mockResolvedValue({...after,attachedLook:{slotId:'s1_look_01',assetId:'asset_local'}});attach.click();
 await vi.waitFor(()=>expect(root.querySelector('.scene-look-stage img')?.getAttribute('data-look-path')).toContain('mood_1.png'));
 expect(host.invoke.mock.calls.map(c=>c[0])).toEqual(['workflow_import','workflow_attach_look']);expect(host.invoke.mock.calls[1][1]).toMatchObject({sceneId:1,role:'mood_reference',sourcePath:'/external/photo.png'});expect(desk.jobsDrawerOpen()).toBe(false);
});
it('cancel makes no slot or command, duplicate picker is prevented and same-root reopen discards its late selection',async()=>{
 const f=fixture(); const root=await mount(f);let resolve!:(value:string|null)=>void;host.openDialog.mockImplementation(()=>new Promise(r=>{resolve=r;}));button('Attach existing image').click();expect(button('Attach existing image').disabled).toBe(true);button('Attach existing image').click();expect(host.openDialog).toHaveBeenCalledTimes(1);resolve(null);await vi.waitFor(()=>expect(button('Attach existing image').disabled).toBe(false));expect(host.invoke).toHaveBeenCalledTimes(1);
 button('Attach existing image').click();await desk.loadWorkflowDesk('/fixture/sb14');resolve('/external/late.png');await new Promise(r=>setTimeout(r,10));expect(host.invoke.mock.calls.map(c=>c[0])).toEqual(['workflow_import','workflow_import']);expect(root.querySelector('.scene-look-stage img')).toBeNull();
});
it('attaches to the explicitly chosen populated slot and selects the new draft over its approved still',async()=>{
 const f=fixture();f.state.scenes[0].look_board={slots:[{id:'other',role:'mood_reference',title:'Other slot',asset_ids:[]},{id:'chosen',role:'interior_reference',title:'Chosen slot',asset_ids:['old'],approved_asset_id:'old'}]};f.state.assets!.push({id:'old',path:'old.png',role:'interior_reference',status:'approved',scene_id:1});const root=await mount(f);
 [...root.querySelectorAll<HTMLButtonElement>('.scene-look-slot')].find(b=>b.textContent==='Chosen slot')!.click();
 const after=structuredClone(f);after.state.scenes[0].look_board!.slots![1].asset_ids!.push('new');after.state.assets!.push({id:'new',path:'new.png',role:'interior_reference',status:'draft',scene_id:1});
 host.openDialog.mockResolvedValue('/external/new.png');host.invoke.mockResolvedValue({...after,attachedLook:{slotId:'chosen',assetId:'new'}});button('Attach existing image').click();
 await vi.waitFor(()=>expect(root.querySelector('.scene-look-stage img')?.getAttribute('data-look-path')).toContain('new.png'));expect(host.invoke.mock.calls[1][1]).toMatchObject({slotId:'chosen'});expect(root.querySelector('.scene-look-slot.active')?.textContent).toBe('Chosen slot');expect(after.state.scenes[0].look_board!.slots![0].asset_ids).toEqual([]);expect(after.state.scenes[0].look_board!.slots![1].approved_asset_id).toBe('old');
});
it('shows picker errors and clears busy for retry',async()=>{const root=await mount(fixture());host.openDialog.mockRejectedValue(Error('Picker unavailable'));button('Attach existing image').click();await vi.waitFor(()=>expect(root.textContent).toContain('Picker unavailable'));expect(button('Attach existing image').disabled).toBe(false);});
