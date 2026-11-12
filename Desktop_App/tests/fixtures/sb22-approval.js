import native from './sb10-native-resume.json';
import * as desk from '/src/workflowDesk.ts';
import {getSurfaceHost} from '/src/studio/surfaces/session.ts';
import '/src/styles.css';
let root='/disposable/sb22/A', serial=0;
const fixture=structuredClone(native.initial);
window.calls=[];window.chatCalls=[];window.mode='success';
window.workbench={
 toMediaUrl:async()=> 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6V6sAAAAASUVORK5CYII=',
 invoke:async(cmd,args)=>{
  window.calls.push({cmd,args:structuredClone(args)});
  if(cmd==='workflow_import')return structuredClone(fixture);
  if(cmd==='workflow_prepare_assemble')return {...structuredClone(fixture),pendingAction:`Selected input: Scenes/Scene1/Cut1/take.mp4\nOutput: Scenes/master.mp4\nLocal assembly — no provider request. [[workbench_approve:HOST${++serial}]]`};
  if(cmd==='director_cancel')return null;
  if(cmd==='director_approve'){
   if(window.mode==='expiry')throw Error('That approval expired. Ask for a new plan.');
   if(window.mode==='delay')await new Promise(resolve=>window.release=resolve);
   return {reply:'Fake local assembly complete; inspect output and Jobs.',toolEvents:['approve:done']};
  }
  throw Error('Forbidden fake host command '+cmd);
 },ask:async()=>{throw Error('Paid generation forbidden in this local operation fixture');}
};
const chat=(...args)=>window.chatCalls.push(args);
desk.bindWorkflowDesk({root:document.querySelector('#desk'),projectRoot:()=>root,chatModel:()=>({apiId:'fake',modelId:'fake'}),onOpenPath:()=>{},onAddToChat:chat,onOpenVoice:()=>{},onComposeChat:chat,onSelectionChange:()=>{},onStatus:()=>{},onPendingPlan:chat});
window.prepare=()=>getSurfaceHost().run('Assemble fixture','workflow_prepare_assemble',{sceneId:1});
window.replaceProject=async(same=true)=>{desk.clearWorkflowDesk();if(!same)root='/disposable/sb22/B';await desk.loadWorkflowDesk(root);};
await desk.loadWorkflowDesk(root);window.calls=[];window.fixtureReady=true;
