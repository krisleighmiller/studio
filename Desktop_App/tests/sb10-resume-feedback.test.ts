import { readFileSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { getSurfaceHost, resetSurfaceSession } from '../src/studio/surfaces/session';
import type { WorkflowImportResult } from '../src/workflowTypes';
// This suite owns native refusal feedback after submission; SB21 tests the modal.
vi.mock('../src/studio/generationPrompt', async () => ({...await vi.importActual('../src/studio/generationPrompt'), editGenerationPrompt: async (spec: {prompt: string}) => spec.prompt}));

// Captured from ordinary native preparation with an isolated synthetic WAV,
// pending render + failed child + occupied retry WAV. No live approval is used.
const native = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8'));
afterEach(() => { desk.clearWorkflowDesk(); resetSurfaceSession(); vi.unstubAllGlobals(); document.body.replaceChildren(); });

it.each(['incomplete', 'different', 'prediction', 'valid'])('shows the exact native Resume outcome (%s), never dispatching speech/video', async kind => {
  const initial = structuredClone(kind === 'valid' ? native.initial : native.refusals[kind].initial) as WorkflowImportResult;
  const forbidden = vi.fn(() => { throw new Error('No TTS, video, approval or network permitted'); });
  const calls: {cmd: string; args: unknown}[] = [];
  const pending = vi.fn();
  const invoke = vi.fn(async (cmd: string, args: Record<string, unknown>) => {
    calls.push({cmd, args});
    if (cmd === 'workflow_import') return structuredClone(initial);
    if (cmd === 'workflow_prepare_render') {
      expect(args).toMatchObject({sceneId: 1, cut: 1, resumeSavedAudio: true});
      if (kind === 'valid') return structuredClone(native.prepared);
      throw new Error(`Error invoking remote method 'host:invoke': Error: ${native.refusals[kind].error}`);
    }
    return forbidden();
  });
  Object.defineProperty(window, 'workbench', {configurable: true, value: {invoke}});
  vi.stubGlobal('fetch', forbidden); vi.stubGlobal('WebSocket', forbidden); vi.stubGlobal('XMLHttpRequest', forbidden);
  const root = document.createElement('main'); document.body.append(root);
  desk.bindWorkflowDesk({root, projectRoot: () => '/fixture/sb10', onOpenPath: forbidden, onAddToChat: forbidden, onOpenVoice: forbidden, onComposeChat: forbidden, onSelectionChange: vi.fn(), chatModel: () => null, onStatus: vi.fn(), onPendingPlan: pending});
  await desk.loadWorkflowDesk('/fixture/sb10');
  const notes = [...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b => b.textContent?.includes('Production Notes'));
  expect(notes, root.textContent ?? '').toBeDefined();
  expect(notes!.disabled).toBe(false); notes!.click();
  const resume = [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === 'Resume with saved audio')!;
  expect(resume).toBeDefined(); expect(resume.disabled).toBe(false);
  resume.click();
  await vi.waitFor(() => expect(calls.some(c => c.cmd === 'workflow_prepare_render')).toBe(true));
  await vi.waitFor(() => expect(getSurfaceHost().busy).toBeNull());
  if (kind === 'valid') {
    await vi.waitFor(() => expect(document.querySelector('dialog[data-operation-approval]')).not.toBeNull());
    const dialog = document.querySelector('dialog[data-operation-approval]')!;
    expect(dialog.textContent).toContain('performance_wav');
    expect(dialog.textContent).toContain('No speech synthesis');
    expect(pending).not.toHaveBeenCalled();
    expect(native.prepared.pendingAction).toContain('performance_wav');
    expect(native.prepared.pendingAction).not.toContain('voice_clone');
    expect(getSurfaceHost().error).toBeNull();
  } else {
    const feedback = root.querySelector('.workflow-desk > div > .workflow-banner');
    expect(feedback?.textContent).toContain(native.refusals[kind].error);
    expect(feedback?.textContent).toContain('Cannot resume Scene 1 Cut 1 with saved audio.');
    expect(feedback?.textContent).toContain('No speech or video was submitted.');
    expect(feedback?.textContent).toContain('Do not use Render Cut as a workaround');
    expect(feedback?.getAttribute('role')).toBe('alert');
    expect(pending).not.toHaveBeenCalled();
    expect([...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === 'Resume with saved audio')!.disabled).toBe(false);
  }
  expect(calls.map(c => c.cmd)).toEqual(['workflow_import', 'workflow_prepare_render']);
  expect(forbidden).not.toHaveBeenCalled();

});

it('does not publish a late Resume refusal into a same-root reopened project', async () => {
  const initial = structuredClone(native.initial) as WorkflowImportResult;
  let release!: () => void;
  let reached!: () => void;
  const held = new Promise<void>(r => { reached = r; });
  const gate = new Promise<void>(r => { release = r; });
  const pending = vi.fn();
  const invoke = vi.fn(async (cmd: string) => {
    if (cmd === 'workflow_import') return structuredClone(initial);
    if (cmd === 'workflow_prepare_render') { reached(); await gate; throw new Error(native.refusals.incomplete.error); }
    throw new Error(`Forbidden dispatch: ${cmd}`);
  });
  Object.defineProperty(window, 'workbench', {configurable: true, value: {invoke}});
  const root = document.createElement('main'); document.body.append(root);
  const unused = vi.fn();
  desk.bindWorkflowDesk({root, projectRoot: () => '/fixture/sb10', onOpenPath: unused, onAddToChat: unused, onOpenVoice: unused, onComposeChat: unused, onSelectionChange: unused, chatModel: () => null, onStatus: unused, onPendingPlan: pending});
  await desk.loadWorkflowDesk('/fixture/sb10');
  [...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b => b.textContent?.includes('Production Notes'))!.click();
  [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === 'Resume with saved audio')!.click();
  try {
    await held;
    desk.clearWorkflowDesk(); await desk.loadWorkflowDesk('/fixture/sb10');
    release();
    await vi.waitFor(() => expect(invoke.mock.settledResults.filter(r => r.type === 'rejected')).toHaveLength(1));
    expect(getSurfaceHost().error).toBeNull(); expect(getSurfaceHost().busy).toBeNull();
    expect(root.textContent).not.toContain('Cannot resume');
    expect(pending).not.toHaveBeenCalled();
    expect(invoke.mock.calls.map(c => c[0])).toEqual(['workflow_import', 'workflow_prepare_render', 'workflow_import']);
  } finally { release(); }
});
