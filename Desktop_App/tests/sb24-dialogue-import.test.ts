import { readFileSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { getSurfaceHost, resetSurfaceSession } from '../src/studio/surfaces/session';
const native=JSON.parse(readFileSync('tests/fixtures/sb12-native-performance.json','utf8'));
function fixture(){const value=structuredClone(native.selected);value.state.state_revision=73;return value;}
function deferred<T>(){let resolve!:(value:T)=>void;let reject!:(error:Error)=>void;const promise=new Promise<T>((a,b)=>{resolve=a;reject=b});return {promise,resolve,reject};}
afterEach(()=>{desk.clearWorkflowDesk();resetSurfaceSession();vi.restoreAllMocks();document.body.replaceChildren();});
function mount(invoke:any,picker:any){
 Object.defineProperty(window,'workbench',{configurable:true,value:{invoke,openDialog:picker,toMediaUrl:async(path:string)=>`file://${path}`}});
 const root=document.createElement('main');document.body.append(root);const unused=vi.fn(),files=vi.fn(),pending=vi.fn();let project='/fixture/run120';
 desk.bindWorkflowDesk({root,projectRoot:()=>project,onOpenPath:unused,onAddToChat:unused,onOpenVoice:unused,onComposeChat:unused,onSelectionChange:unused,chatModel:()=>null,onStatus:unused,onPendingPlan:pending,onProjectFilesChanged:files});
 return {root,files,pending,setProject:(p:string)=>project=p};
}
function notes(root:HTMLElement){[...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Production Notes'))!.click();}
function button(root:HTMLElement){const control=[...root.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent==='Import dialogue WAV…');expect(control,'mounted native import control').toBeDefined();return control!;}
it('owns native picker cancel/success/duplicate and uses exact captured shot without auto-selection',async()=>{
 const chosen=deferred<string|null>(), imported=deferred<any>();const picker=vi.fn().mockResolvedValueOnce(null).mockImplementationOnce(()=>chosen.promise);
 const invoke=vi.fn(async(cmd:string)=>{if(cmd==='workflow_import')return fixture();if(cmd==='workflow_import_performance')return imported.promise;throw Error(`Forbidden ${cmd}`)});
 const {root,files,pending}=mount(invoke,picker);await desk.loadWorkflowDesk('/fixture/run120');notes(root);
 expect(root.textContent).toContain('64 MiB');expect(root.textContent).toContain('300 seconds');
 button(root).click();await vi.waitFor(()=>expect(getSurfaceHost().busy).toBeNull());
 expect(invoke.mock.calls.map(c=>c[0])).toEqual(['workflow_import']);expect(files).not.toHaveBeenCalled();
 button(root).click();button(root).click();expect(button(root).disabled).toBe(true);expect(picker).toHaveBeenCalledTimes(2);
 expect(picker).toHaveBeenLastCalledWith({title:'Import dialogue WAV',directory:false,multiple:false,filters:[{name:'WAV audio',extensions:['wav']}]});
 chosen.resolve('/external/chosen.wav');await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('workflow_import_performance',{projectRoot:'/fixture/run120',sceneId:1,cut:1,shotStableId:fixture().state.scenes[0].cuts[0].stable_id,expectedRevision:73,sourcePath:'/external/chosen.wav'}));
 expect(button(root).disabled).toBe(true);expect(getSurfaceHost().busy).not.toBeNull();
 const result=fixture();result.note='Draft imported at Scenes/Scene1/Cut1/dialogue.wav; audition and select explicitly.';result.state.assets.push({id:'new-import',path:'Scenes/Scene1/Cut1/dialogue.wav',role:'source_audio',status:'draft',scene_id:1,cut_id:{source_scene:1,cut:1}});
 result.performanceImport={sceneId:1,cut:1,shotStableId:result.state.scenes[0].cuts[0].stable_id,assetId:'new-import',path:'Scenes/Scene1/Cut1/dialogue.wav',created:true,registered:true};
 imported.resolve(result);await vi.waitFor(()=>expect(getSurfaceHost().busy).toBeNull());
 expect(root.querySelector('audio[data-performance-path="Scenes/Scene1/Cut1/dialogue.wav"]')).not.toBeNull();expect(root.textContent).toContain('audition and select explicitly');expect(getSurfaceHost().error).toBeNull();expect(pending).not.toHaveBeenCalled();
 expect(getSurfaceHost().state?.scenes[0].cuts[0].selected_performance_asset_id).toBe(fixture().state.scenes[0].cuts[0].selected_performance_asset_id);
 expect(files).toHaveBeenCalledTimes(1);expect(files.mock.calls[0][0]()).toBe(true);
 expect(invoke.mock.calls.map(c=>c[0])).toEqual(['workflow_import','workflow_import_performance']);
});
it.each([[[]], [''], [['unexpected.wav']]])('rejects malformed picker result %j without acquisition',async(value)=>{
 const invoke=vi.fn(async()=>fixture());const {root}=mount(invoke,vi.fn(async()=>value));await desk.loadWorkflowDesk('/fixture/run120');notes(root);button(root).click();
 await vi.waitFor(()=>expect(getSurfaceHost().busy).toBeNull());expect(getSurfaceHost().error).toContain('single WAV');expect(invoke).toHaveBeenCalledTimes(1);
});
it('shows picker rejection and releases originating busy',async()=>{
 const {root}=mount(vi.fn(async()=>fixture()),vi.fn(async()=>{throw Error('Picker refused')}));await desk.loadWorkflowDesk('/fixture/run120');notes(root);button(root).click();
 await vi.waitFor(()=>expect(getSurfaceHost().busy).toBeNull());expect(getSurfaceHost().error).toContain('Picker refused');expect(button(root).disabled).toBe(false);
});
it.each(['picker','success','error'])('retires late %s after same-root reopen or replacement without clearing B-owned busy',async phase=>{
 for(const replacement of ['/fixture/run120','/fixture/B']){
  desk.clearWorkflowDesk();const held=deferred<any>();let first=true;
  const picker=vi.fn(async()=>{if(phase==='picker' && first){first=false;return held.promise;}return '/external/chosen.wav';});
  const invoke=vi.fn(async(cmd:string)=>{if(cmd==='workflow_import')return fixture();if(cmd==='workflow_import_performance'){if(first){first=false;return held.promise;}return new Promise(()=>{});}throw Error(cmd);});
  const {root,files,pending,setProject}=mount(invoke,picker);await desk.loadWorkflowDesk('/fixture/run120');notes(root);button(root).click();
  await vi.waitFor(()=>expect(phase==='picker'?picker.mock.calls.length:invoke.mock.calls.filter(c=>c[0]==='workflow_import_performance').length).toBe(1));
  desk.clearWorkflowDesk();setProject(replacement);await desk.loadWorkflowDesk(replacement);notes(root);button(root).click();await vi.waitFor(()=>expect(invoke.mock.calls.filter(c=>c[0]==='workflow_import_performance').length).toBe(phase==='picker'?1:2));
  const busy=getSurfaceHost().busy;expect(busy).not.toBeNull();const markup=root.innerHTML;
  if(phase==='error')held.reject(Error('late A error'));else held.resolve(phase==='picker'?'/external/late.wav':fixture());
  await new Promise(r=>setTimeout(r,20));expect(getSurfaceHost().busy).toBe(busy);expect(root.innerHTML).toBe(markup);expect(getSurfaceHost().error).toBeNull();expect(files).not.toHaveBeenCalled();expect(pending).not.toHaveBeenCalled();
 }
});
