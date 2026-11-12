import { readFileSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { resetSurfaceSession } from '../src/studio/surfaces/session';
import type { WorkflowImportResult } from '../src/workflowTypes';
const host=vi.hoisted(()=>({invoke:vi.fn(),openDialog:vi.fn(),ask:vi.fn(),toMediaUrlAsync:vi.fn(async()=> 'data:audio/wav;base64,')}));
vi.mock('../src/host',()=>host);
afterEach(()=>{desk.clearWorkflowDesk();resetSurfaceSession();document.body.replaceChildren();vi.clearAllMocks();});
it('mounts Cut Sketches speech controls and prepares only the chosen cut and voice through the desk',async()=>{
 const result=JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json','utf8')).initial as WorkflowImportResult;
 const scene=result.state.scenes[0],cut=scene.cuts[0];
 for (const id of ['scene_look','cut_sketches']) { const nav=result.report.scenes[0].navigation!.find(n=>n.id===id); if(nav) {nav.available=true;nav.disabled_reason=null;} }
 cut.speech_mode='source_video_lipsync';cut.dialogue=[{speaker:'A',text:'The complete current line.'}];cut.production_note_path=null;cut.approved_start_frame=null;
 scene.look_board={slots:[]};result.state.assets=[{id:'chosen-voice',role:'voice_sample',status:'draft',path:'Characters/A/voice.wav'}];
 host.invoke.mockImplementation(async(cmd:string)=>{if(['workflow_import','workflow_prepare_dialogue_audio'].includes(cmd))return structuredClone(result);throw Error(`Unexpected ${cmd}`);});
 const root=document.createElement('main'),noop=vi.fn();document.body.append(root);
 desk.bindWorkflowDesk({root,projectRoot:()=>'/fixture/speech',onOpenPath:noop,onAddToChat:noop,onOpenVoice:noop,onComposeChat:noop,onSelectionChange:noop,chatModel:()=>null,onStatus:noop,onPendingPlan:noop});
 await desk.loadWorkflowDesk('/fixture/speech');
 [...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Scene Look'))!.click();
 expect(root.querySelector('.scene-dialogue-audio')).toBeNull();
 [...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b=>b.textContent?.includes('Cut Sketches'))!.click();
 const section=root.querySelector('.scene-dialogue-audio')!;expect(section).not.toBeNull();
 const voice=section.querySelector('select')!;voice.value='chosen-voice';voice.dispatchEvent(new Event('change'));
 const generate=[...section.querySelectorAll('button')].find(b=>b.textContent==='Generate dialogue audio…')!;
 generate.click();generate.click();
 await vi.waitFor(()=>expect(host.invoke).toHaveBeenCalledWith('workflow_prepare_dialogue_audio',expect.objectContaining({projectRoot:'/fixture/speech',sceneId:scene.id,cut:cut.id.cut,voiceAssetId:'chosen-voice'})));
 expect(host.invoke.mock.calls.map(c=>c[0])).toEqual(['workflow_import','workflow_prepare_dialogue_audio']);
 expect(section.textContent).not.toContain('Prepare video only');
});
