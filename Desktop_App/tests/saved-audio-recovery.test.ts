import { expect, it, vi } from 'vitest';
import { renderProductionNoteCard } from '../src/studio/surfaces/notesStage';
import { bindSurfaceHost } from '../src/studio/surfaces/session';
import type { SurfaceHost } from '../src/studio/surfaces/host';
import type { CutRecord } from '../src/workflowTypes';

it('offers explicit saved-audio recovery without dispatching a normal render', () => {
  bindSurfaceHost({ busy: false, chatReady: true, resolvePath: (p: string | null | undefined) => p ? `/fixture/${p}` : null, hooks: { onOpenPath: vi.fn() } } as unknown as SurfaceHost);
  const cut = { id: { source_scene: 1, cut: 1 }, stable_id: 'shot_1', title: 'Mom', speech_mode: 'source_video_lipsync', render_lane: 'audio_driven_i2v', prompt: 'A talking shot.', production_note_path: 'Scene_Prompts/Scene1/Cut1.txt', approved_start_frame: 'frame.png', visual_source: { strategy: 'preproduction_start_frame', asset_id: 'frame' }, sketch: { title: 'Mom', shot_role: 'performance', mouth_policy: 'speaking_visible', speaker: 'Mom', visual_prompt: 'Talking', dialogue: [] }, takes: [] } as unknown as CutRecord;
  const emit = vi.fn();
  const card = renderProductionNoteCard(cut, emit);
  const resume = Array.from(card.querySelectorAll('button')).find(b => b.textContent === 'Resume with saved audio');
  expect(resume, 'A real surface action is required for already-paid audio after restart').toBeDefined();
  expect(resume!.disabled).toBe(false);
  resume!.click();
  expect(emit).toHaveBeenCalledExactlyOnceWith({ type: 'resume-audio', cut: 1 });
});
