import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { afterEach, expect, it, vi } from 'vitest';
import * as desk from '../src/workflowDesk';
import { resetSurfaceSession } from '../src/studio/surfaces/session';
import { buildSceneLook } from '../src/studio/sceneLookModel';
import { renderSceneLookView } from '../src/studio/sceneLookView';
import type { WorkflowImportResult } from '../src/workflowTypes';

const host = vi.hoisted(() => ({invoke: vi.fn(), openDialog: vi.fn(), ask: vi.fn(), toMediaUrlAsync: vi.fn(async () => 'data:image/png;base64,')}));
vi.mock('../src/host', () => host);
const native = JSON.parse(readFileSync('tests/fixtures/sb10-native-resume.json', 'utf8')).initial as WorkflowImportResult;
function fixture() {
  const result = structuredClone(native);
  // This isolated UI fixture has an existing saved board; reflect that in
  // its host navigation receipt (SB10's audio fixture had no Scene Intent).
  const nav = result.report.scenes[0].navigation!.find(n => n.id === 'scene_look')!;
  nav.available = true; nav.disabled_reason = null; nav.status = 'draft';
  result.state.assets ??= [];
  result.state.scenes[0].look_board = {slots: [{id: 'empty-slot', role: 'interior_reference', title: 'Interior', prompt: 'Warm room', asset_ids: []}]};
  return result;
}
function find(root: HTMLElement, label: string) {
  return [...root.querySelectorAll<HTMLButtonElement>('button')].find(b => b.textContent === label);
}
function exposed(button: HTMLButtonElement) {
  expect(button.closest('details:not([open])')).toBeNull();
}
afterEach(() => {desk.clearWorkflowDesk(); resetSurfaceSession(); document.body.replaceChildren(); vi.clearAllMocks();});

it('exposes first-image choices and routes Attach only to the picker/local attach, then shows Add alternate', async () => {
  const initial = fixture();
  const attached = structuredClone(initial);
  attached.state.scenes[0].look_board!.slots![0].asset_ids = ['fixture-image'];
  attached.state.assets!.push({id: 'fixture-image', role: 'interior_reference', scene_id: 1, path: 'Preproduction/Scene1/references/local.png', status: 'draft'});
  host.openDialog.mockResolvedValue('/fixture/input.png');
  host.invoke.mockImplementation(async (cmd: string) => {
    if (cmd === 'workflow_import') return structuredClone(initial);
    if (cmd === 'workflow_attach_look') return structuredClone(attached);
    throw new Error(`Forbidden: ${cmd}`);
  });
  const unused = vi.fn(); const root = document.createElement('main'); document.body.append(root);
  desk.bindWorkflowDesk({root, projectRoot: () => '/fixture/sb13', onOpenPath: unused, onAddToChat: unused, onOpenVoice: unused, onComposeChat: unused, onSelectionChange: unused, chatModel: () => null, onStatus: unused, onPendingPlan: unused});
  await desk.loadWorkflowDesk('/fixture/sb13');
  const nav = [...root.querySelectorAll<HTMLButtonElement>('.studio-surface-nav button')].find(b => b.textContent?.includes('Scene Look'))!;
  expect(nav).toBeDefined(); expect(nav.disabled).toBe(false); nav.click();
  const attach = find(root, 'Attach existing image')!;
  expect(attach).toBeDefined(); exposed(attach);
  const generate = find(root, 'Generate still')!;
  expect(generate).toBeDefined(); exposed(generate);
  expect(root.querySelector('.scene-look-alts')?.textContent).toContain('provider');
  expect(root.querySelector('.scene-look-alts')?.textContent).toContain('approval');
  expect(find(root, 'Add alternate')).toBeUndefined();
  const before = root.innerHTML;
  attach.click();
  await vi.waitFor(() => expect(find(root, 'Add alternate')).toBeDefined());
  expect(host.openDialog).toHaveBeenCalledOnce();
  expect(host.invoke.mock.calls.map(c => c[0])).toEqual(['workflow_import', 'workflow_attach_look']);
  expect(host.invoke.mock.calls[1][1]).toMatchObject({sceneId: 1, slotId: 'empty-slot', sourcePath: '/fixture/input.png'});
  expect(find(root, 'Generate still')).toBeUndefined();
  exposed(find(root, 'Add alternate')!);
  expect(root.querySelector('.scene-look-stage img')?.getAttribute('data-look-path')).toContain('local.png');
  expect(unused.mock.calls.filter(c => typeof c[0] === 'string' && c[0].includes('approval-'))).toHaveLength(0);
  mkdirSync('evidence/sb13', {recursive: true});
  writeFileSync('evidence/sb13/ui-readback.json', JSON.stringify({before, after: root.innerHTML, calls: host.invoke.mock.calls, generationRequests: 0, generateClicked: false, boundary: 'Mounted real desk with isolated host receipt; no native copy/provider execution'}, null, 2));
});

it('uses the selected slot versions, not images elsewhere, and disables first-image actions while busy', () => {
  const f = fixture();
  f.state.assets!.push({id: 'unrelated', role: 'interior_reference', path: 'other.png', status: 'approved'});
  const model = buildSceneLook({scene: f.state.scenes[0], report: undefined, assets: f.state.assets!, jobs: [], characters: [], blockers: [], focusedSlotId: null, focusedAssetId: null, directorSlotId: null, identityHint: null, busy: 'Working', resolvePath: p => p ?? null});
  const intent = vi.fn(); const root = renderSceneLookView(model, intent);
  expect(find(root, 'Add alternate')).toBeUndefined();
  for (const label of ['Attach existing image', 'Generate still']) {
    const b = find(root, label)!; expect(b).toBeDefined(); expect(b.disabled).toBe(true); b.click();
  }
  expect(intent).not.toHaveBeenCalled();
});
