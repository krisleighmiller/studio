import { readFileSync } from 'node:fs';
// This suite owns performance selection/settlement; SB21 exercises the real modal.
vi.mock('../src/studio/generationPrompt', async () => ({...await vi.importActual('../src/studio/generationPrompt'), editGenerationPrompt: async (spec: {prompt: string}) => spec.prompt}));
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { getSurfaceHost, resetSurfaceSession } from '../src/studio/surfaces/session';
// Actual production-library dispatch receipts; expired diagnostic tokens are never executed.
const native = JSON.parse(readFileSync('tests/fixtures/sb12-native-performance.json', 'utf8'));
const derivative=native.selected.state.scenes[0].cuts[0].selected_performance_asset_id;
const paths=native.version.state.assets.filter((a:any)=>a.path.endsWith('.wav')).map((a:any)=>a.path);
afterEach(() => { desk.clearWorkflowDesk(); resetSurfaceSession(); vi.unstubAllGlobals(); document.body.replaceChildren(); });
function mount(invoke:any,pending=vi.fn()) {
 Object.defineProperty(window,'workbench',{configurable:true,value:{invoke,toMediaUrl:async (path:string)=>`file://${path}`}});
 const root=document.createElement('main');document.body.append(root);const unused=vi.fn();
 desk.bindWorkflowDesk({root,projectRoot:()=>'/fixture/sb12',onOpenPath:unused,onAddToChat:unused,onOpenVoice:unused,onComposeChat:unused,onSelectionChange:unused,chatModel:()=>null,onStatus:unused,onPendingPlan:pending});
 return {root,pending};
}
function notes(root:HTMLElement){[...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Production Notes'))!.click();}
function button(root:HTMLElement,text:string){return [...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===text)!;}
it('auditions both exact native WAV versions, selects derivative once and hands off its no-TTS card', async () => {
 let current=structuredClone(native.version);const calls:string[]=[];
 const invoke=vi.fn(async(cmd:string,args:Record<string,unknown>)=>{
  calls.push(cmd);
  if(cmd==='workflow_import')return structuredClone(current);
  if(cmd==='workflow_select_performance'){
   expect(args).toMatchObject({sceneId:1,cut:1,assetId:derivative});current=structuredClone(native.selected);return current;
  }
  if(cmd==='workflow_prepare_render'){
   expect(args).toMatchObject({sceneId:1,cut:1,selectedPerformance:true});return structuredClone(native.prepared);
  }
  throw Error(`Forbidden dispatch ${cmd}`);
 });
 const {root,pending}=mount(invoke);await desk.loadWorkflowDesk('/fixture/sb12');notes(root);
 expect([...root.querySelectorAll<HTMLAudioElement>('audio[data-performance-path]')].map(a=>a.dataset.performancePath)).toEqual(paths);
 expect([...root.querySelectorAll<HTMLAudioElement>('audio')].every(a=>a.controls && !a.autoplay)).toBe(true);
 expect(button(root,'Prepare video only with selected WAV').disabled).toBe(true);
 const select=root.querySelector<HTMLButtonElement>(`button[data-select-performance="${derivative}"]`)!;
 expect(select.disabled).toBe(true); select.closest('figure')!.querySelector<HTMLInputElement>('input[type=checkbox]')!.click();select.click();select.click();
 await vi.waitFor(()=>expect(getSurfaceHost().busy).toBeNull());expect(calls.filter(c=>c==='workflow_select_performance')).toHaveLength(1);
 expect(root.querySelector<HTMLButtonElement>(`button[data-select-performance="${derivative}"]`)!.disabled).toBe(true);
 const selectedRow=root.querySelector<HTMLButtonElement>(`button[data-select-performance="${derivative}"]`)!.closest('figure')!;
 selectedRow.querySelector<HTMLInputElement>('input[type=checkbox]')!.click();
 button(root,'Prepare video only with selected WAV').click();await vi.waitFor(()=>expect(document.querySelector('dialog[data-operation-approval]')).not.toBeNull());
 expect(document.querySelector('dialog[data-operation-approval]')!.textContent).toContain(paths[1]);
 expect(document.querySelector('dialog[data-operation-approval]')!.textContent).toContain('TTS requests: 0');
 expect(pending).not.toHaveBeenCalled();
 expect(native.prepared.pendingAction).toContain(paths[1]);expect(native.prepared.pendingAction).toContain('TTS requests: 0');expect(native.prepared.pendingAction).not.toContain('voice_clone');
 expect(calls).toEqual(['workflow_import','workflow_select_performance','workflow_prepare_render']);
});
it('clears explicit selection through native cancellation without preparing or submitting',async()=>{
 const calls:string[]=[];const invoke=vi.fn(async(cmd:string,args:any)=>{calls.push(cmd);if(cmd==='workflow_import')return structuredClone(native.selected);if(cmd==='workflow_select_performance'){expect(args.assetId).toBeNull();return structuredClone(native.cleared);}throw Error(cmd);});
 const {root,pending}=mount(invoke);await desk.loadWorkflowDesk('/fixture/sb12');notes(root);
 button(root,'Clear WAV selection / invalidate video card').click();await vi.waitFor(()=>expect(getSurfaceHost().busy).toBeNull());
 expect(button(root,'Prepare video only with selected WAV').disabled).toBe(true);expect(pending).not.toHaveBeenCalled();expect(calls).toEqual(['workflow_import','workflow_select_performance']);
});
it.each(['select','prepare','version'])('does not publish a late %s response into same-root reopen or clear B-owned busy',async operation=>{
 let release!:(value:any)=>void;let releaseB!:(value:any)=>void;let reached!:()=>void;const held=new Promise<void>(r=>reached=r);let first=true;
 const cmd=operation==='select'?'workflow_select_performance':operation==='prepare'?'workflow_prepare_render':'workflow_version_audio';
 const invoke=vi.fn(async(name:string)=>{if(name==='workflow_import')return structuredClone(operation==='select'?native.version:native.selected);if(name==='director_cancel')return null;if(name==='workflow_b_owned')return await new Promise(r=>releaseB=r);if(name===cmd && first){first=false;reached();return await new Promise(r=>release=r);}throw Error(name);});
 const {root,pending}=mount(invoke);await desk.loadWorkflowDesk('/fixture/sb12');notes(root);
 const start=()=>{
  if(operation==='select' || operation==='prepare') {
   const row=root.querySelector<HTMLButtonElement>(`button[data-select-performance="${derivative}"]`)!.closest('figure')!;
   row.querySelector<HTMLInputElement>('input[type=checkbox]')!.click();
  }
  const target=operation==='select'?root.querySelector<HTMLButtonElement>(`button[data-select-performance="${derivative}"]`)!:button(root,operation==='prepare'?'Prepare video only with selected WAV':'Create local speed version');
  expect(target.disabled).toBe(false);target.click();
 };
 start();await vi.waitFor(()=>expect(first).toBe(false));await held;desk.clearWorkflowDesk();await desk.loadWorkflowDesk('/fixture/sb12');notes(root);
 const bRun=getSurfaceHost().run('B-owned operation','workflow_b_owned',{});
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(c=>c[0]==='workflow_b_owned')).toHaveLength(1));expect(getSurfaceHost().busy).toContain('B-owned');
 const bState=structuredClone(getSurfaceHost().state);
 release(structuredClone(operation==='prepare'?native.prepared:operation==='select'?native.selected:native.version));
 await vi.waitFor(()=>expect(invoke.mock.settledResults.filter(r=>r.type==='fulfilled').length).toBeGreaterThanOrEqual(3));
 expect(getSurfaceHost().busy).toContain('B-owned');expect(getSurfaceHost().state).toEqual(bState);expect(pending).not.toHaveBeenCalled();expect(getSurfaceHost().error).toBeNull();
 expect(document.querySelector('dialog[data-operation-approval]')).toBeNull();
 if(operation==='prepare') await vi.waitFor(()=>expect(invoke.mock.calls.filter(c=>c[0]==='director_cancel')).toHaveLength(1));
 releaseB(structuredClone(operation==='select'?native.version:native.selected));await bRun;
});
