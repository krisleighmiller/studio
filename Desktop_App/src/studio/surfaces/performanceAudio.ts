import type { CutRecord } from '../../workflowTypes';
import type { NotesIntent } from './surfaceIntents';
import { auditionPerformance, getSurfaceHost } from './session';
import { actionButton, block } from './shared';

/** Native controls audition the real version, never a playback-rate simulation. */
export function renderPerformanceAudio(cut: CutRecord, emit: (intent: NotesIntent) => void, showVideoPreparation = true): HTMLElement {
  const host = getSurfaceHost();
  const section = document.createElement('section');
  section.className = 'workflow-performance-audio';
  section.append(block('workflow-field-hint', 'Import a complete dialogue performance for this shot. WAV only, at most 64 MiB and 300 seconds. A local copy is saved as Draft; importing does not select it, verify the transcript, or submit speech/video.'));
  const acquire = actionButton('Import dialogue WAV…', () => emit({type:'import-performance',cut:cut.id.cut}));
  acquire.disabled = Boolean(host.busy) || cut.speech_mode !== 'source_video_lipsync';
  section.append(acquire);
  section.append(block('workflow-field-hint', 'Dialogue audio before video: audition the source and local versions. Select only a complete performance you want used for this shot. Selection is not automatic transcript verification. Speed preserves pitch, pitch approximately preserves duration, and trim keeps a half-open source window [start, end). These edits never overwrite the source. Review the entire dialogue after trimming.'));
  const lines = cut.dialogue?.length ? cut.dialogue : cut.sketch?.dialogue ?? [];
  section.append(block('workflow-field-hint', lines.length
    ? `Review every spoken line against this WAV: ${lines.map(line => `${line.speaker}: “${line.text}”`).join(' ')}`
    : 'No cut dialogue is available here. Open the current script and full production note, then check the entire WAV before confirming.'));
  if (cut.script_dialogue_review) section.append(block('workflow-banner warn', 'Script dialogue changed; reconcile the current script, cut and production note before selecting audio.'));
  const shot = host.state?.scenes.find(scene => scene.id === cut.id.source_scene)?.animatic?.shots?.find(item =>
    item.cut_id.source_scene === cut.id.source_scene && item.cut_id.cut === cut.id.cut);
  const prefix = `Scenes/Scene${cut.id.source_scene}/Cut${cut.id.cut}/`;
  const candidates = (host.state?.assets ?? []).filter(a =>
    ['source_audio', 'audio_take', 'voice_sample'].includes(a.role)
    && a.path.startsWith(prefix) && !a.path.slice(prefix.length).includes('/') && /\.wav$/i.test(a.path));
  let selectedReview: HTMLInputElement | null = null;
  let prepareButton: HTMLButtonElement | null = null;
  for (const asset of candidates) {
    const row = document.createElement('figure');
    const caption = document.createElement('figcaption');
    caption.textContent = `${asset.path}${asset.parent_asset_id ? ` · derived from ${asset.parent_asset_id}` : ' · source'}${cut.selected_performance_asset_id === asset.id ? ' · selected for video' : ''}`;
    if (asset.notes) {
      try {
        const window = JSON.parse(asset.notes).source_window;
        if (window && Number.isFinite(window.start_sample) && Number.isFinite(window.end_sample_exclusive) && Number.isFinite(window.sample_rate)) {
          caption.textContent += ` · source samples [${window.start_sample}, ${window.end_sample_exclusive}) at ${window.sample_rate} Hz`;
        }
      } catch { /* Older notes may be plain text. */ }
    }
    const audio = document.createElement('audio');
    audio.controls = true; audio.preload = 'metadata';
    audio.dataset.performancePath = asset.path;
    audio.setAttribute('aria-label', `Audition ${asset.path}`);
    audio.addEventListener('play', () => auditionPerformance(audio));
    const error = block('workflow-field-hint', '');
    error.addEventListener('contextmenu', () => {
      const selection = window.getSelection();
      if (!selection || !error.textContent) return;
      const range = document.createRange();
      range.selectNodeContents(error);
      selection.removeAllRanges();
      selection.addRange(range);
    });
    void host.mediaUrl(host.resolvePath(asset.path) ?? asset.path).then(url => {
      // A late URL must not start/play media from a replaced project/surface.
      if (!audio.isConnected) return;
      audio.src = url;
    }).catch(() => { if (audio.isConnected) error.textContent = 'Cannot load this WAV. No selection or video was submitted.'; });
    audio.addEventListener('error', () => { error.textContent = 'Cannot play this WAV. Choose a readable, complete local version.'; });
    const confirmed = document.createElement('input'); confirmed.type = 'checkbox';
    const review = document.createElement('label');
    review.append(confirmed, document.createTextNode(' I listened to this entire WAV and compared every line with the current cut and full production note. Timing is an estimate, not proof of dialogue completeness.'));
    const select = actionButton('Select complete WAV', () => emit({type:'select-performance',cut:cut.id.cut,assetId:asset.id,reviewedAssetId:asset.id}));
    select.dataset.selectPerformance = asset.id;
    select.disabled = Boolean(host.busy) || cut.selected_performance_asset_id === asset.id || !confirmed.checked;
    confirmed.addEventListener('change', () => {
      select.disabled = Boolean(host.busy) || cut.selected_performance_asset_id === asset.id || !confirmed.checked;
      if (cut.selected_performance_asset_id === asset.id && prepareButton) prepareButton.disabled = Boolean(host.busy) || !confirmed.checked;
    });
    if (cut.selected_performance_asset_id === asset.id) selectedReview = confirmed;
    if (cut.selected_performance_asset_id === asset.id) {
      const timing = block('workflow-field-hint', 'Selected WAV timing: loading metadata…');
      row.append(timing);
      audio.addEventListener('loadedmetadata', () => {
        const measured = audio.duration;
        if (!Number.isFinite(measured) || measured <= 0) { timing.textContent = 'Selected WAV timing unavailable. Review the whole performance manually.'; return; }
        if (!shot) { timing.textContent = `Selected WAV ${measured.toFixed(2)}s; no editorial shot duration available. Review timing manually.`; return; }
        const delta = measured - shot.duration_seconds;
        timing.textContent = `Selected WAV ${measured.toFixed(2)}s vs editorial screen ${shot.duration_seconds.toFixed(2)}s: WAV ${delta >= 0 ? 'longer' : 'shorter'} by ${Math.abs(delta).toFixed(2)}s. Review lead, after-line beat, full dialogue and source window; no automatic retiming or transcript verification.`;
        if (Math.abs(delta) > 0.5) timing.classList.add('warn');
      });
    }
    const label = document.createElement('label');label.textContent = 'Local speed (pitch preserved) ';
    const speed = document.createElement('input'); speed.type='number';speed.min='0.5';speed.max='2';speed.step='0.05';speed.value='1.25';
    speed.setAttribute('aria-label', `Speed for ${asset.path}`);label.append(speed);
    const version = actionButton('Create local speed version', () => {
      const value = Number(speed.value);
      if (!speed.value || !Number.isFinite(value) || value < 0.5 || value > 2) { error.textContent='Choose a speed between 0.5 and 2.'; return; }
      emit({type:'version-audio',cut:cut.id.cut,assetId:asset.id,speed:value});
    });
    version.disabled=Boolean(host.busy);speed.disabled=Boolean(host.busy);
    const pitchLabel = document.createElement('label');pitchLabel.textContent = 'Local pitch (approximate duration retained) ';
    const pitch = document.createElement('input');pitch.type='number';pitch.min='0.5';pitch.max='2';pitch.step='0.05';pitch.value='1.5';
    pitch.setAttribute('aria-label', `Pitch for ${asset.path}`);pitchLabel.append(pitch);
    const pitchVersion = actionButton('Create local pitch version', () => {
      const value = Number(pitch.value);
      if (!pitch.value || !Number.isFinite(value) || value < 0.5 || value > 2) { error.textContent='Choose a pitch multiplier between 0.5 and 2.'; return; }
      emit({type:'version-pitch',cut:cut.id.cut,assetId:asset.id,pitch:value});
    });
    pitchVersion.disabled=Boolean(host.busy);pitch.disabled=Boolean(host.busy);
    const trimStartLabel = document.createElement('label'); trimStartLabel.textContent='Trim start (seconds) ';
    const trimStart = document.createElement('input');trimStart.type='number';trimStart.min='0';trimStart.max='300';trimStart.step='0.001';trimStart.value='0';trimStart.setAttribute('aria-label',`Trim start for ${asset.path}`);trimStartLabel.append(trimStart);
    const trimEndLabel = document.createElement('label');trimEndLabel.textContent='Trim end (seconds, exclusive) ';
    const trimEnd = document.createElement('input');trimEnd.type='number';trimEnd.min='0';trimEnd.max='300';trimEnd.step='0.001';trimEnd.value='';trimEnd.placeholder='Source duration';trimEnd.setAttribute('aria-label',`Trim end for ${asset.path}`);trimEndLabel.append(trimEnd);
    audio.addEventListener('loadedmetadata',()=>{if (!trimEnd.value && Number.isFinite(audio.duration)) trimEnd.value=String(Math.min(300,audio.duration));});
    const trimVersion=actionButton('Create local trim version',()=>{
      const start=Number(trimStart.value), end=Number(trimEnd.value);
      if (!trimStart.value || !trimEnd.value || !Number.isFinite(start) || !Number.isFinite(end) || start<0 || end<=start || end>300 || (Number.isFinite(audio.duration) && end>audio.duration+0.000001)) {
        error.textContent='Choose a valid start and end within this WAV (at most 300 seconds).';return;
      }
      emit({type:'version-trim',cut:cut.id.cut,assetId:asset.id,startSeconds:start,endSeconds:end});
    });
    trimStart.disabled=trimEnd.disabled=trimVersion.disabled=Boolean(host.busy);
    row.append(caption,audio,error,review,select,label,version,pitchLabel,pitchVersion,trimStartLabel,trimEndLabel,trimVersion);section.append(row);
  }
  if (!candidates.length) section.append(block('workflow-field-hint', 'No saved shot WAV is available yet. This local review does not generate speech.'));
  prepareButton = actionButton('Prepare video only with selected WAV', () => {
    if (cut.selected_performance_asset_id && selectedReview?.checked) emit({type:'prepare-performance',cut:cut.id.cut,reviewedAssetId:cut.selected_performance_asset_id});
  });
  prepareButton.disabled = Boolean(host.busy) || !selectedReview?.checked;
  if (showVideoPreparation) section.append(prepareButton);
  if (cut.selected_performance_asset_id && !candidates.some(a => a.id === cut.selected_performance_asset_id)) {
    section.append(block('workflow-banner warn', 'Selected WAV is unavailable in this shot. Clear the selection and review another full version before video.'));
  }
  if (cut.selected_performance_asset_id) {
    const clear=actionButton('Clear WAV selection / invalidate video card', () => emit({type:'select-performance',cut:cut.id.cut,assetId:null}));
    clear.disabled=Boolean(host.busy);section.append(clear);
  }
  return section;
}
