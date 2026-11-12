import { afterEach, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { renderSketchStage } from '../src/studio/surfaces/sketchStage';
import { bindSurfaceHost, resetSurfaceSession } from '../src/studio/surfaces/session';
import type { SurfaceHost } from '../src/studio/surfaces/host';

const fixture = JSON.parse(readFileSync('tests/fixtures/sb15-native-card.json', 'utf8'));
afterEach(() => { resetSurfaceSession(); document.body.replaceChildren(); });

it('offers audition of existing dialogue on the cut without a video preparation control', () => {
  const state = structuredClone(fixture.initial.state);
  const scene = state.scenes[0];
  const cut = scene.cuts[0];
  cut.speech_mode = 'source_video_lipsync';
  state.assets.push({id:'dialogue-fixture',path:`Scenes/Scene${cut.id.source_scene}/Cut${cut.id.cut}/sample.wav`,role:'source_audio',status:'draft'});
  bindSurfaceHost({ state, selectedCut: cut.id.cut, busy: null, error: null,
    recipeRequires: () => false, continuityDependents: () => [], blockerNames: () => [],
    chatReady: true, resolvePath: (p: string) => p, mediaUrl: async (p: string) => p,
    rerender: () => {} } as unknown as SurfaceHost);
  const root = renderSketchStage(scene, undefined, vi.fn(), vi.fn(), vi.fn());
  const card = [...root.querySelectorAll<HTMLElement>('.workflow-sketch-card')]
    .find(c => c.querySelector('h3')?.textContent?.startsWith(`Cut ${cut.id.cut} ·`))!;
  expect(card.querySelector('audio[controls]')).not.toBeNull();
  expect(card.textContent).not.toContain('Prepare video only with selected WAV');
});

// Speech generation belongs to Scene Look; covered by scene-look-dialogue-audio.test.ts.
