import { readFileSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { getSurfaceHost } from '../src/studio/surfaces/session';
import { createSpendConfirmationHandler } from '../src/spendConfirmation';
const native = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8'));
const flush = async () => { await new Promise(r => setTimeout(r, 0)); };
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); localStorage.clear(); });
async function setup(twoScenes=false) {
  let project = '/disposable/sb22/A';
  const fixture = structuredClone(native.initial);
  if(twoScenes) fixture.state.scenes.push({...structuredClone(fixture.state.scenes[0]),id:2,stable_id:'other-scene'});
  const calls: {cmd:string;args:any}[]=[];
  const chat = vi.fn();
  const bridge = {invoke: async(cmd:string,args:any) => {
    calls.push({cmd,args:structuredClone(args)});
    if(cmd==='workflow_import')return structuredClone(fixture);
    if(cmd==='workflow_prepare_assemble')return {...structuredClone(fixture),pendingAction:'Exact selected take /fixture/take.mp4 → /fixture/master.mp4 [[workbench_approve:HOST1]]'};
    if(cmd==='director_approve')return {reply:'Local fake execution complete',toolEvents:['approve:done']};
    if(cmd==='director_cancel')return null;
    throw Error('Forbidden command '+cmd);
  },ask:vi.fn(async()=>false)};
  Object.defineProperty(window,'workbench',{configurable:true,value:bridge});
  const root=document.createElement('main');document.body.append(root);
  desk.bindWorkflowDesk({root,projectRoot:()=>project,onOpenPath:vi.fn(),onAddToChat:chat,onOpenVoice:vi.fn(),onComposeChat:chat,onSelectionChange:vi.fn(),chatModel:()=>({apiId:'fake',modelId:'fake'}),onStatus:vi.fn(),onPendingPlan:chat});
  await desk.loadWorkflowDesk(project);calls.length=0;
  const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!;
  const prepare=async()=>{await getSurfaceHost().run('Assemble fixture','workflow_prepare_assemble',{sceneId:1});await flush();};
  const replace=async(same=true)=>{desk.clearWorkflowDesk();if(!same)project='/disposable/sb22/B';await desk.loadWorkflowDesk(project);};
  return {calls,chat,root,bridge,button,prepare,replace,project:()=>project};
}
it('keeps the prepared scene and exact operation when selection changes before approval',async()=>{
  const s=await setup(true);await s.prepare();
  const other=getSurfaceHost().state!.scenes.find(scene=>scene.id===2)!;
  getSurfaceHost().rememberScene(other, other.cuts[0]);
  expect(desk.workflowSelection().sceneId).toBe(2);
  expect(document.querySelector('dialog[data-operation-approval]')?.textContent).toContain('/fixture/take.mp4');
  s.button('Approve exact operation').click();await flush();await flush();
  expect(s.calls.filter(c=>c.cmd==='workflow_prepare_assemble')).toEqual([{cmd:'workflow_prepare_assemble',args:{projectRoot:s.project(),sceneId:1}}]);
  expect(s.calls.filter(c=>c.cmd==='director_approve')).toEqual([{cmd:'director_approve',args:{id:'HOST1',progressId:expect.stringMatching(/^studio-operation-/)}}]);
  expect(s.chat).not.toHaveBeenCalled();
});

it.each(['stop','replace'])('ignores a delayed post-approval refresh after %s, including a late cancellation failure',async how=>{
  const s=await setup(),base=s.bridge.invoke;
  let finishRefresh!:(value:any)=>void, rejectCancel!:(error:Error)=>void;
  let holdRefresh=true;
  s.bridge.invoke=async(cmd,args)=>{
    if(cmd==='workflow_import' && args.persist && holdRefresh){holdRefresh=false;s.calls.push({cmd,args});return new Promise(r=>finishRefresh=r);}
    if(cmd==='director_approve'){s.calls.push({cmd,args});return {reply:'Stale next action [[workbench_approve:HOST2]]',toolEvents:[]};}
    if(cmd==='director_cancel'){s.calls.push({cmd,args});return new Promise((_r,reject)=>rejectCancel=reject);}
    return base(cmd,args);
  };
  await s.prepare();s.button('Approve exact operation').click();await flush();
  expect(finishRefresh).toBeTypeOf('function');
  if(how==='stop') s.button('Stop operation').click();
  else await s.replace(false);
  await flush();
  expect(rejectCancel).toBeTypeOf('function');
  const stale=structuredClone(native.initial);stale.note='STALE REFRESH MUST NOT APPEAR';
  finishRefresh(stale);await flush();
  expect(s.root.textContent).not.toContain('STALE REFRESH MUST NOT APPEAR');
  expect(document.querySelector('dialog[data-operation-approval]')).toBeNull();
  expect(s.calls.filter(c=>c.cmd==='director_approve')).toHaveLength(1);
  expect(s.calls.filter(c=>c.cmd==='director_cancel')).toEqual([{cmd:'director_cancel',args:{id:'HOST1'}}]);
  rejectCancel(new Error('LATE CANCEL FAILURE'));await flush();
  if(how==='replace') expect(s.root.textContent).not.toContain('LATE CANCEL FAILURE');
  expect(s.chat).not.toHaveBeenCalled();
});

it('routes exact host operation to a modal, never chat; approves a detached duplicate only once',async()=>{
  const s=await setup();await s.prepare();
  const dialog=document.querySelector('dialog[data-operation-approval]');
  expect(dialog,'pending Studio operation must be inspectable without Director chat').not.toBeNull();
  expect(dialog?.textContent).toContain('/fixture/take.mp4');
  expect(s.chat).not.toHaveBeenCalled();
  const approve=s.button('Approve exact operation');approve.click();approve.click();await flush();await flush();
  expect(s.calls.filter(c=>c.cmd==='director_approve')).toEqual([{cmd:'director_approve',args:{id:'HOST1',progressId:expect.stringMatching(/^studio-operation-/)}}]);
  expect(s.bridge.ask).not.toHaveBeenCalled();expect(s.chat).not.toHaveBeenCalled();
  expect(s.calls.some(c=>c.cmd==='media_generate_confirmed')).toBe(false);
});

it.each(['deny','escape'])('%s cancels host authorization without dispatch or opening chat',async how=>{
  const s=await setup();await s.prepare();
  const duplicate=s.button('Approve exact operation');
  if(how==='deny')s.button('Deny operation').click();
  else document.querySelector('dialog')!.dispatchEvent(new Event('cancel',{cancelable:true}));
  await flush();duplicate.click();await flush();
  expect(s.calls.map(c=>c.cmd)).toEqual(['workflow_prepare_assemble','director_cancel']);
  expect(s.calls.find(c=>c.cmd==='director_cancel')?.args).toEqual({id:'HOST1'});
  expect(s.chat).not.toHaveBeenCalled();expect(desk.modelSheetMediaOwners.size).toBe(0);
});
it('keeps cancellation ownership until the host replies so a new preparation cannot be cancelled by the old denial',async()=>{
  const s=await setup(),base=s.bridge.invoke;let release!:()=>void;
  s.bridge.invoke=async(cmd,args)=>{if(cmd==='director_cancel'){s.calls.push({cmd,args});await new Promise<void>(r=>release=r);return null;}return base(cmd,args);};
  await s.prepare();s.button('Deny operation').click();await flush();
  expect(await getSurfaceHost().run('New preparation','workflow_prepare_assemble',{sceneId:1})).toBe(false);
  expect(s.calls.filter(c=>c.cmd==='workflow_prepare_assemble')).toHaveLength(1);
  release();await flush();expect(desk.modelSheetMediaOwners.size).toBe(0);
});
it('shows the exact host expiry error, refuses repeated ID and never silently prepares or retries',async()=>{
  const s=await setup(),base=s.bridge.invoke;
  s.bridge.invoke=async(cmd,args)=>{if(cmd==='director_approve'){s.calls.push({cmd,args});throw Error('That approval expired. Ask for a new plan.');}return base(cmd,args);};
  await s.prepare();s.button('Approve exact operation').click();await flush();
  expect(s.root.textContent).toContain('That approval expired. Ask for a new plan.');
  await s.prepare();expect(document.querySelector('dialog')).toBeNull();
  expect(s.root.textContent).toContain('already handled');
  expect(s.calls.filter(c=>c.cmd==='director_approve')).toHaveLength(1);
  expect(s.chat).not.toHaveBeenCalled();
});
it.each([true,false])('closes pending modal on project replacement (same root=%s); stale detached approval sends nothing',async same=>{
  const s=await setup();await s.prepare();const approve=s.button('Approve exact operation');
  await s.replace(same);approve.click();await flush();
  expect(document.querySelector('dialog')).toBeNull();
  expect(s.calls.map(c=>c.cmd)).toEqual(['workflow_prepare_assemble','director_cancel','workflow_import']);
  expect(s.calls.find(c=>c.cmd==='director_cancel')?.args).toEqual({id:'HOST1'});
  expect(s.chat).not.toHaveBeenCalled();
});
it('retires an exact prepared ID returned after project replacement without publishing a modal',async()=>{
  const s=await setup(),base=s.bridge.invoke;let release!:(value:any)=>void;
  s.bridge.invoke=async(cmd,args)=>cmd==='workflow_prepare_assemble'?new Promise(r=>release=r):base(cmd,args);
  const preparing=getSurfaceHost().run('Late preparation','workflow_prepare_assemble',{sceneId:1});
  await flush();await s.replace(false);
  release({...structuredClone(native.initial),pendingAction:'Late [[workbench_approve:LATE1]]'});
  await preparing;await flush();
  expect(document.querySelector('dialog')).toBeNull();
  expect(s.calls.filter(c=>c.cmd==='director_cancel')).toEqual([{cmd:'director_cancel',args:{id:'LATE1'}}]);
});
it('rejects missing or ambiguous host IDs without dispatching',async()=>{
  const s=await setup(),base=s.bridge.invoke;
  for(const text of ['No ID','Bad [[workbench_approve:NOT-VALID]]','Two [[workbench_approve:A]] [[workbench_approve:B]]']){
    s.bridge.invoke=async(cmd,args)=>cmd==='workflow_prepare_assemble'?{...await base(cmd,args),pendingAction:text}:base(cmd,args);
    await s.prepare();expect(document.querySelector('dialog')).toBeNull();
  }
  expect(s.calls.filter(c=>c.cmd==='director_approve')).toHaveLength(0);expect(s.chat).not.toHaveBeenCalled();
});

async function mediaSetup() {
  const s=await setup(),base=s.bridge.invoke;
  s.bridge.invoke=async(cmd,args)=>{
    if(cmd==='director_approve'){s.calls.push({cmd,args});return {reply:'Await separate media authorization',toolEvents:['approve:awaiting-spend-authorization']};}
    return base(cmd,args);
  };
  await s.prepare();s.button('Approve exact operation').click();await flush();
  const ownerId=[...desk.modelSheetMediaOwners.keys()][0];expect(ownerId).toMatch(/^studio-operation-/);
  const payload={projectRoot:s.project(),provider:'fake-provider',category:'media',operation:'atlas_generate_video',model:'fake-model',ownerRequestId:ownerId,workflowContinuationBound:true,args:{model:'fake-model',save_path:'/disposable/sb22/A/master.mp4'},quote:{currency:'USD',requestHash:'a'.repeat(64),ledgerRevision:'fake-revision',disclosure:'Unknown'}};
  const invoke=vi.fn(async(cmd:string,args:Record<string,unknown>)=>{s.calls.push({cmd,args});if(cmd==='confirm_unbounded_spend')return 'fake-single-token';if(cmd==='media_generate_confirmed')return {mediaResult:'saved:null',continuation:null};throw Error('Forbidden '+cmd);});
  const ask=vi.fn(async()=>false);
  const handler=createSpendConfirmationHandler({invoke,ask,projectRoot:s.project,ownerIsCurrent:id=>!!id&&!!desk.modelSheetMediaOwners.get(id)?.isCurrent(),authorizeWorkflowMedia:({ownerRequestId,operation,args})=>ownerRequestId?desk.modelSheetMediaOwners.get(ownerRequestId)?.authorizeMedia({operation,args}):undefined,status:vi.fn(),onSubmitted:async(result,_current,id)=>{await desk.modelSheetMediaOwners.get(id!)?.settle({success:true,result});},onSettled:async(outcome,_current,id)=>{if(!outcome.success)await desk.modelSheetMediaOwners.get(id!)?.settle(outcome);}});
  return {...s,payload,handler,invoke,ask,ownerId};
}
it('operation approval alone never spends; separate paid denial settles the initiating receipt',async()=>{
  const s=await mediaSetup();expect(s.invoke).not.toHaveBeenCalled();
  await s.handler(s.payload);expect(s.ask).toHaveBeenCalledOnce();expect(s.invoke).not.toHaveBeenCalled();
  expect(s.root.textContent).toContain('Media generation denied');expect(s.chat).not.toHaveBeenCalled();
});
it('separate paid acceptance dispatches exact media arguments once even for duplicate pending events',async()=>{
  const s=await mediaSetup();let release!:(value:boolean)=>void;
  s.ask.mockImplementation(()=>new Promise(r=>release=r));
  const first=s.handler(s.payload);await flush();await s.handler(s.payload);release(true);await first;
  expect(s.invoke.mock.calls).toEqual([['confirm_unbounded_spend',expect.objectContaining({requestHash:'a'.repeat(64)})],['media_generate_confirmed',{projectRoot:s.project(),operation:s.payload.operation,mediaArgs:s.payload.args,token:'fake-single-token'}]]);
  expect(s.ask).toHaveBeenCalledOnce();expect(s.chat).not.toHaveBeenCalled();
});
it('Stop retires the exact owner before late completion; a late paid event cannot dispatch',async()=>{
  const s=await mediaSetup();
  const stop=s.button('Stop operation');expect(stop).toBeDefined();stop.click();stop.click();await flush();
  await s.handler(s.payload);
  expect(s.calls.filter(c=>c.cmd==='director_cancel')).toHaveLength(1);
  expect(s.ask).not.toHaveBeenCalled();expect(s.invoke).not.toHaveBeenCalled();
  expect(s.root.textContent).toContain('Stopped; no further operation');
});
it('continuation needs its own new host ID; host errors never become success',async()=>{
  const s=await setup(),base=s.bridge.invoke;let n=0;
  s.bridge.invoke=async(cmd,args)=>{if(cmd==='director_approve'){s.calls.push({cmd,args});return ++n===1?{reply:'Next exact operation [[workbench_approve:HOST2]]',toolEvents:[]}:{reply:'Fixture provider-failed: refused',toolEvents:['approve:error']};}return base(cmd,args);};
  await s.prepare();s.button('Approve exact operation').click();await flush();await flush();
  expect(document.querySelector('dialog')?.textContent).toContain('Next exact operation');
  expect(s.calls.filter(c=>c.cmd==='director_approve').map(c=>c.args.id)).toEqual(['HOST1']);
  s.button('Approve exact operation').click();await flush();
  expect(s.calls.filter(c=>c.cmd==='director_approve').map(c=>c.args.id)).toEqual(['HOST1','HOST2']);
  expect(s.root.textContent).toContain('Fixture provider-failed: refused');expect(s.chat).not.toHaveBeenCalled();
});
it.each([true,false])('project replacement after operation approval drops late media event before any paid dialog (same root=%s)',async same=>{
  const s=await mediaSetup();await s.replace(same);await s.handler(s.payload);
  expect(s.ask).not.toHaveBeenCalled();expect(s.invoke).not.toHaveBeenCalled();
});
