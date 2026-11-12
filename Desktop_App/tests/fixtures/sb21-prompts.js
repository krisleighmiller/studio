import native from './sb10-native-resume.json';
import * as desk from '/src/workflowDesk.ts';
import '/src/styles.css';
let project='/disposable/sb21/A';
const fixture=structuredClone(native.initial);
for (const n of fixture.report.scenes[0].navigation) { n.available=true;n.disabled_reason=null; }
const scene=fixture.state.scenes[0];
scene.source_path='Scripts/local-fixture.md';
scene.look_board={slots:[{id:'room',role:'interior_reference',title:'Room',prompt:'Original warm room',asset_ids:['asset_18']}]};
scene.cuts[0].start_frame_prompt='Original still prompt';
fixture.state.characters=[{id:'char_mara',name:'Mara',dir:'Characters/Mara',profile_path:'Characters/Mara/character.yaml',profile:{identity_prompt:'Saved identity'},reference_asset_ids:['ref_mara']}];
fixture.state.assets.push({id:'ref_mara',character_id:'char_mara',path:'Characters/Mara/refs/face.png',role:'identity_reference'});
window.calls=[];window.handoffs=[];
window.workbench={
 toMediaUrl:async()=> 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAusB9Wl6V6sAAAAASUVORK5CYII=',
 ask:async()=>false,
 invoke:async(cmd,args)=>{
  window.calls.push({cmd,args:structuredClone(args)});
  if(cmd==='workflow_import')return structuredClone(fixture);
  if(cmd==='workflow_generate_script')return {script:'Scene 1 — Locally returned fixture'};
  if(cmd==='workflow_save_production_prompt'){scene.cuts[0].prompt=args.prompt;return structuredClone(fixture);}
  if(cmd==='workflow_save_start_frame_prompt'){scene.cuts[0].start_frame_prompt=args.prompt;return structuredClone(fixture);}
  if(cmd==='start_character_model_sheet')return {pendingAction:`Model sheet [[workbench_approve:CHAR${window.calls.length}]]`};
  if(cmd==='director_cancel')return null;
  if(['workflow_draft_sketches','workflow_generate_look','workflow_generate_start_frame','workflow_prepare_render'].includes(cmd))return {...structuredClone(fixture),pendingAction:`Fresh preparation [[workbench_approve:NEW${window.calls.length}]]`};
  throw Error('Forbidden fake-host command '+cmd);
 }
};
desk.bindWorkflowDesk({root:document.querySelector('#desk'),projectRoot:()=>project,chatModel:()=>({apiId:'fake',modelId:'fake'}),onOpenPath:()=>{},onAddToChat:()=>{throw Error('Chat forbidden');},onOpenVoice:()=>{},onComposeChat:()=>{throw Error('Chat forbidden');},onSelectionChange:()=>{},onStatus:()=>{},onPendingPlan:text=>window.handoffs.push(text)});
window.replaceProject=async()=>{desk.clearWorkflowDesk();await desk.loadWorkflowDesk(project);};
await desk.loadWorkflowDesk(project);window.calls=[];window.fixtureReady=true;
