import { readFileSync, writeFileSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { getSurfaceHost } from '../src/studio/surfaces/session';
// This suite owns post-submit receipts; SB21 exercises the real editable modal.
vi.mock('../src/studio/generationPrompt', async () => ({...await vi.importActual('../src/studio/generationPrompt'), editGenerationPrompt: async (spec: {prompt: string}) => spec.prompt}));

const native = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8'));
afterEach(() => { desk.clearWorkflowDesk(); vi.restoreAllMocks(); document.body.replaceChildren(); localStorage.clear(); });

it('retains initiating-surface request receipts through delayed success, pending, failure, cancellation and project replacement without duplicate dispatch', async () => {
  let project = '/fixture/sb20/A';
  let resolve!: (value: unknown) => void;
  let reject!: (error: Error) => void;
  const pending = vi.fn();
  const calls: {cmd: string; args: any}[] = [];
  const invoke = vi.fn(async (cmd: string, args: any) => {
    calls.push({cmd, args});
    if (cmd === 'workflow_import') return structuredClone(native.initial);
    if (cmd === 'director_cancel') return null;
    if (cmd === 'workflow_generate_script' || cmd === 'workflow_prepare_render') {
      return await new Promise((yes, no) => { resolve = yes; reject = no; });
    }
    throw Error(`Forbidden ${cmd}`);
  });
  Object.defineProperty(window, 'workbench', {configurable: true, value: {invoke}});
  const root = document.createElement('main'); document.body.append(root);
  const unused = vi.fn();
  desk.bindWorkflowDesk({root, projectRoot: () => project, onOpenPath: unused, onAddToChat: unused, onOpenVoice: unused, onComposeChat: unused, onSelectionChange: unused, chatModel: () => ({apiId: 'fake', modelId: 'fake'}), onStatus: unused, onPendingPlan: pending});
  const flush = async () => { await new Promise(r => setTimeout(r, 0)); };
  const nav = (text: string) => {
    const b = [...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b => b.textContent?.includes(text));
    expect(b, root.textContent ?? '').toBeDefined(); expect(b!.disabled).toBe(false); b!.click();
  };
  const button = (text: string) => [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === text)!;
  const receipt = () => root.querySelector<HTMLElement>('[data-request-receipt]');
  const snapshots: unknown[] = [];
  const snapshot = (phase: string) => snapshots.push({phase, project, surface: root.querySelector('.workflow-desk')?.getAttribute('data-stage'), receipt: receipt()?.outerHTML ?? null});
  await desk.loadWorkflowDesk(project); nav('Script'); button('Create script').click();
  const notes = root.querySelector<HTMLTextAreaElement>('[name=notes]')!;
  notes.value = 'Exact notes <not markup> — one scene.'; notes.dispatchEvent(new Event('input', {bubbles: true}));
  const send = button('Generate script from notes'); send.click(); send.click(); await flush();
  expect(receipt()?.textContent).toContain('Submitted'); expect(receipt()?.textContent).toContain('Working');
  expect(receipt()?.textContent).toContain(notes.value); expect(receipt()?.querySelector('not')).toBeNull();
  const id = receipt()!.dataset.requestReceipt; snapshot('script working');
  nav('Profile'); expect(receipt()).toBeNull(); nav('Script'); expect(receipt()!.dataset.requestReceipt).toBe(id);
  resolve({script: 'Scene 1 — Arrival\nRecovered script'}); await flush();
  expect(receipt()?.textContent).toContain('Draft returned'); snapshot('script success');
  expect(root.querySelector<HTMLTextAreaElement>('[name=script]')!.value).toContain('Recovered script');
  expect(calls.filter(c => c.cmd === 'workflow_generate_script')).toHaveLength(1);
  button('Generate script from notes').click(); await flush(); reject(Error('Fixture model refused')); await flush();
  expect(receipt()?.textContent).toContain('Error'); expect(receipt()?.textContent).toContain('Fixture model refused'); snapshot('script error');
  button('Generate script from notes').click(); await flush(); resolve({cancelled: true}); await flush();
  expect(receipt()?.textContent).toContain('Cancelled'); snapshot('script cancelled');
  nav('Production Notes'); button('Resume with saved audio').click(); await flush();
  expect(receipt()?.textContent).toContain('Working'); const renderId = receipt()!.dataset.requestReceipt;
  // Exercise duplicate dispatch at the wired command seam as well as a detached initiating button.
  expect(await getSurfaceHost().run('Preparing saved-audio recovery…', 'workflow_prepare_render', {sceneId: 1, cut: 1, resumeSavedAudio: true})).toBe(false);
  nav('Script'); expect(receipt()!.dataset.requestReceipt).not.toBe(renderId); nav('Production Notes');
  resolve(structuredClone(native.prepared)); await flush();
  expect(receipt()?.textContent).toContain('Queued job'); expect(receipt()?.textContent).toContain('job_5'); expect(receipt()?.textContent).not.toContain('Success');
  expect(pending).not.toHaveBeenCalled(); expect(document.querySelector('dialog[data-operation-approval]')).not.toBeNull(); snapshot('pending modal transport');
  [...document.querySelectorAll<HTMLButtonElement>('dialog button')].find(b=>b.textContent==='Deny operation')!.click();await flush();
  button('Resume with saved audio').click(); await flush();
  const lateId = receipt()!.dataset.requestReceipt; const finishA = resolve;
  desk.clearWorkflowDesk(); project = '/fixture/sb20/B'; await desk.loadWorkflowDesk(project); nav('Production Notes');
  expect(receipt()).toBeNull();
  button('Resume with saved audio').click(); await flush(); const finishB = resolve;
  finishB(structuredClone(native.prepared)); await flush();
  const bReceipt = receipt()!.outerHTML; expect(receipt()?.textContent).toContain('Queued job'); snapshot('B pending');
  finishA(structuredClone(native.prepared)); await flush();
  expect(receipt()!.outerHTML).toBe(bReceipt); expect(pending).not.toHaveBeenCalled();
  desk.clearWorkflowDesk(); project = '/fixture/sb20/A'; await desk.loadWorkflowDesk(project); nav('Production Notes');
  expect(receipt()!.dataset.requestReceipt).toBe(lateId); expect(receipt()?.textContent).toContain('Cancelled'); snapshot('A restored settled receipt only');
  expect(bReceipt).not.toContain(lateId);
  expect(calls.filter(c => c.cmd === 'workflow_prepare_render' && c.args.projectRoot.endsWith('/A'))).toHaveLength(2);
  button('Resume with saved audio').click(); await flush(); const lateError = reject;
  desk.clearWorkflowDesk(); await desk.loadWorkflowDesk(project); nav('Production Notes');
  expect(receipt()?.textContent).toContain('Working');
  lateError(Error('Old session refusal')); await flush();
  expect(receipt()?.textContent).toContain('Error'); expect(receipt()?.textContent).toContain('Old session refusal');
  expect(getSurfaceHost().error).toBeNull(); expect(getSurfaceHost().busy).toBeNull();
  expect(pending).not.toHaveBeenCalled(); snapshot('same root read-only error receipt');
  if (process.env.SB20_RECEIPT) writeFileSync(process.env.SB20_RECEIPT, JSON.stringify({snapshots, calls, approvalHandoffs: pending.mock.calls.length, providerCalls: 0}, null, 2));
});
