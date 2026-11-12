import type { SceneRecord } from '../workflowTypes';
import type { NotesIntent } from './surfaces/surfaceIntents';
import { getSurfaceHost } from './surfaces/session';
import { actionButton, block } from './surfaces/shared';
import { renderPerformanceAudio } from './surfaces/performanceAudio';
/** Cut Sketches owns speech acquisition and audition; Production owns video. */
export function renderSceneDialogueAudio(scene: SceneRecord, emit: (intent: NotesIntent) => void): HTMLElement {
 const root=document.createElement('section'); root.className='scene-dialogue-audio';
 root.append(block('workflow-section-title','Dialogue audio'));
 root.append(block('workflow-field-hint','Generate speech separately, then audition and explicitly select a complete WAV. Provider approval and charges apply; this never starts video.'));
 const host=getSurfaceHost();
 for (const cut of scene.cuts.filter(c=>c.speech_mode==='source_video_lipsync')) {
  const card=document.createElement('article'); card.append(block('workflow-section-title',`Cut ${cut.id.cut} · dialogue audio`));
  for (const line of cut.dialogue ?? []) card.append(block('workflow-field-hint',`${line.speaker}: ${line.text}`));
  const voice=document.createElement('select'); voice.setAttribute('aria-label',`Voice for Cut ${cut.id.cut}`);
  const option=(label:string,value:string)=>{const o=document.createElement('option');o.textContent=label;o.value=value;return o;};
  voice.append(option('Choose a saved voice…',''));
  for (const asset of host.state?.assets ?? []) if (['voice_sample','voice_profile'].includes(asset.role)) voice.append(option(asset.path,asset.id));
  const generate=actionButton('Generate dialogue audio…',()=>emit({type:'generate-dialogue',cut:cut.id.cut,voiceAssetId:voice.value}));
  const update=()=>{generate.disabled=Boolean(host.busy)||!voice.value||!cut.dialogue?.some(l=>l.text.trim())||Boolean(cut.script_dialogue_review);};
  voice.disabled=Boolean(host.busy);voice.addEventListener('change',update);update();
  card.append(voice,generate,renderPerformanceAudio(cut,emit,false));root.append(card);
 }
 return root;
}
