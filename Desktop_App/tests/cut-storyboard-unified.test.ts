import { afterEach, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { renderSketchStage } from '../src/studio/surfaces/sketchStage';
import { bindSurfaceHost, resetSurfaceSession } from '../src/studio/surfaces/session';
import type { SurfaceHost } from '../src/studio/surfaces/host';

const fixture = JSON.parse(readFileSync('tests/fixtures/sb15-native-card.json', 'utf8'));
afterEach(() => { resetSurfaceSession(); document.body.replaceChildren(); });
it('puts each shot timing and scratch controls on its own cut card while keeping scene preview approval', () => {
 const state = structuredClone(fixture.initial.state);
 const scene = state.scenes[0];
 expect(scene.animatic.shots.length).toBeGreaterThan(1);
 const emit = vi.fn();
 bindSurfaceHost({state,selectedCut:1,busy:null,error:null,recipeRequires:()=>false,continuityDependents:()=>[],blockerNames:()=>[],chatReady:true,resolvePath:(p:string)=>p,mediaUrl:async(p:string)=>p,rerender:()=>{}} as unknown as SurfaceHost);
 const root=renderSketchStage(scene, undefined, emit, vi.fn()); document.body.append(root);
 expect(root.querySelector('section[aria-label="Animatic"]')).toBeNull();
 for (const shot of scene.animatic.shots) {
  const card=[...root.querySelectorAll<HTMLElement>('.workflow-sketch-card')].find(c=>c.querySelector('h3')?.textContent?.startsWith(`Cut ${shot.cut_id.cut} ·`));
  expect(card).toBeDefined();
  const duration=card!.querySelector<HTMLInputElement>(`input[aria-label="Duration for cut ${shot.cut_id.cut}"]`);
  expect(duration).not.toBeNull();
  duration!.value='4.7'; duration!.dispatchEvent(new Event('change'));
  expect(emit).toHaveBeenCalledWith({type:'set-duration',shotId:shot.id,durationSeconds:4.7});
  expect([...card!.querySelectorAll('button')].some(b=>/scratch/i.test(b.textContent ?? ''))).toBe(true);
 }
 expect(root.textContent).toMatch(/sketch preview/i);
 expect(root.textContent).toMatch(/approve sketch animatic/i);
});
