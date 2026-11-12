/** Execute production declarations with an isolated host/paint seam.
 * Full built-Electron visibility proof is retained in S04/run24 as well.
 */
import { describe, expect, it, vi } from 'vitest';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import vm from 'node:vm';
import { refreshSavedScript } from '../src/workflowDesk';

const source = readFileSync(resolve('src/main.ts'), 'utf8');
const ast = ts.createSourceFile('main.ts', source, ts.ScriptTarget.Latest, true);
const names = ['openFile', 'openFileDialog', 'placeFileSurface', 'rememberFileTab', 'isFileTab',
  'saveActiveFile', 'saveActiveFileAs', 'activeEditor', 'isTextTab', 'isSettingsTab', 'isDocsTab', 'joinPath'];
const declarations = names.map(name => {
  const declaration = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  if (!declaration) throw new Error(`Missing production function ${name}`);
  return declaration.getText(ast);
}).join('\n');
const compiled = ts.transpileModule(declarations, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } }).outputText;

function harness(surface = 'studio', kind = 'text', failRead = false) {
  const editor = document.createElement('textarea');
  editor.hidden = surface !== 'files' && surface !== 'assets';
  const invoke = vi.fn(async (command: string) => {
    if (command === 'classify_file') return kind;
    if (command === 'read_text_file') {
      if (failRead) throw new Error('read refused');
      return '# canonical handoff\nprompt: Moving camera';
    }
    throw new Error(`Unexpected command ${command}`);
  });
  let counter = 0;
  const sandbox: Record<string, any> = {
    editorTabs: [], activeEditorId: null, filesEditorId: null, shellSurface: surface,
    projectRoot: '/project', projectEpoch: 0, projectTransitionInFlight: false,
    selectedPath: '/project/keep.txt', expandedDirs: new Set(), refreshSavedScript,
    openDialog: vi.fn(async () => '/external/notes.md'), OPEN_FILTERS: [],
    dirname: (p: string) => p.slice(0, p.lastIndexOf('/')), renderExplorer: vi.fn(),
    invoke, syncTabFromEditor: vi.fn(), pauseAllMedia: vi.fn(),
    uid: () => `editor-${++counter}`, basename: (p: string) => p.split('/').at(-1),
    isMediaKind: (k: string) => ['video', 'image', 'audio'].includes(k),
    setStatus: vi.fn(), window: { alert: vi.fn() }, UNSUPPORTED_HINT: 'unsupported',
    renderAssetBrowser: vi.fn(),
  };
  sandbox.paintSurface = vi.fn(() => { editor.hidden = !['files', 'assets'].includes(sandbox.shellSurface); });
  sandbox.renderEditor = vi.fn(() => { editor.value = sandbox.editorTabs.find((t: any) => t.id === sandbox.activeEditorId)?.contents ?? ''; });
  sandbox.focusEditor = vi.fn(() => editor.focus());
  vm.createContext(sandbox);
  vm.runInContext(compiled, sandbox);
  return { sandbox, editor, invoke, open: (p = '/project/Scene_Prompts/Scene1/Cut1.md') => sandbox.openFile(p) as Promise<void> };
}

function saveHarness(path: string | null = '/external/notes.md') {
  const h = harness('files');
  const s = h.sandbox;
  const original = '# original\r\nunchanged on disk\n';
  const edited = '# edited\r\nexact unsaved bytes Ω\n';
  const tab = { id: 'keep', name: 'notes.md', path, kind: 'text', contents: edited,
    original, dirty: true, selectionStart: 4, selectionEnd: 15, preview: false };
  s.editorTabs.push(tab); s.activeEditorId = tab.id; s.filesEditorId = tab.id;
  h.editor.value = edited; h.editor.setSelectionRange(4, 15); h.editor.focus();
  const statusLeft = document.createElement('span');
  const statusRight = document.createElement('span');
  s.els = { editor: h.editor, statusLeft, statusRight }; s.statusRevision = 0;
  // Execute the real editor synchronization and status DOM writer, not feedback mocks.
  for (const name of ['syncTabFromEditor', 'setStatus']) {
    const node = ast.statements.find(n => ts.isFunctionDeclaration(n) && n.name?.text === name)!;
    vm.runInContext(ts.transpileModule(node.getText(ast), {
      compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
    }).outputText, s);
  }
  s.saveDialog = vi.fn(async () => null);
  s.normalizePath = (p: string) => p;
  const disk = new Map([[path, original]]);
  h.invoke.mockImplementation(async (command: string, args?: any) => {
    if (command !== 'write_text_file') throw new Error(`Unexpected command ${command}`);
    if (args.path === '/external/notes.md') throw new Error('That path is outside the open project or lacks a one-time grant.');
    disk.set(args.path, args.contents);
    return '';
  });
  const snapshot = () => ({ tabs: JSON.stringify(s.editorTabs), active: s.activeEditorId,
    files: s.filesEditorId, selected: s.selectedPath, value: h.editor.value,
    start: h.editor.selectionStart, end: h.editor.selectionEnd });
  return { ...h, tab, disk, original, edited, statusLeft, statusRight, snapshot };
}

describe('FX05 text save failure recovery', () => {
  it.each(['denial', 'disk failure'])('contains %s rejection, displays actual error and Save As guidance, and preserves editor/disk', async failure => {
    const h = saveHarness();
    const message = failure === 'denial'
      ? 'That path is outside the open project or lacks a one-time grant.'
      : 'Disk full: text write failed';
    if (failure !== 'denial') h.invoke.mockRejectedValue(message);
    const before = h.snapshot();
    // A rejecting promise here is exactly what the void Ctrl+S caller leaves unhandled.
    await expect(h.sandbox.saveActiveFile()).resolves.toBeUndefined();
    expect(h.statusLeft.textContent).toContain('Could not save file');
    expect(h.statusLeft.textContent).toContain('Save As');
    expect(h.statusRight.textContent).toContain(message);
    expect(h.snapshot()).toEqual(before);
    expect(h.disk.get(h.tab.path)).toBe(h.original);
    expect(h.invoke).toHaveBeenCalledExactlyOnceWith('write_text_file', { path: h.tab.path, contents: h.edited });
    expect(h.sandbox.saveDialog).not.toHaveBeenCalled();
    expect(h.sandbox.renderEditor).not.toHaveBeenCalled();
  });

  it('authorized in-project save writes exact bytes and marks the existing tab clean', async () => {
    const h = saveHarness('/project/notes.md');
    await h.sandbox.saveActiveFile();
    expect(h.disk.get(h.tab.path)).toBe(h.edited);
    expect(h.tab.original).toBe(h.edited); expect(h.tab.dirty).toBe(false);
    expect(h.statusLeft.textContent).toBe('Saved');
    expect(h.statusRight.textContent).toBe('/project/notes.md');
    expect(h.sandbox.renderEditor).toHaveBeenCalledOnce();
  });

  it('existing Save As cancel after refusal retains dirty state; explicit recovery saves only the chosen destination', async () => {
    const h = saveHarness(); const s = h.sandbox;
    await expect(s.saveActiveFile()).resolves.toBeUndefined();
    const before = h.snapshot(); h.invoke.mockClear();
    await s.saveActiveFileAs();
    expect(h.snapshot()).toEqual(before);
    expect(h.invoke).not.toHaveBeenCalled();
    s.saveDialog.mockResolvedValue('/project/recovered.md');
    await s.saveActiveFileAs();
    expect(h.disk.get('/external/notes.md')).toBe(h.original);
    expect(h.disk.get('/project/recovered.md')).toBe(h.edited);
    expect(h.tab.path).toBe('/project/recovered.md'); expect(h.tab.dirty).toBe(false);
    expect(h.statusLeft.textContent).toBe('Saved');
  });

  it('untitled Save picker cancel stays non-destructive', async () => {
    const h = saveHarness(null); const before = h.snapshot();
    await h.sandbox.saveActiveFile();
    expect(h.snapshot()).toEqual(before);
    expect(h.invoke).not.toHaveBeenCalled();
    expect(h.statusLeft.textContent).toBe('');
  });
});

describe('FX01 native picker completion', () => {
  it.each(['picker', 'classify', 'read'])('surfaces %s errors without an unhandled rejection or changing tabs/selection', async failure => {
    const { sandbox: s, invoke } = harness('files');
    const old = { id: 'keep', path: '/project/keep.txt', kind: 'text', contents: 'unsaved', dirty: true };
    s.editorTabs.push(old); s.activeEditorId = old.id;
    const error = new Error(`FX01 ${failure} refused`);
    if (failure === 'picker') s.openDialog.mockRejectedValue(error);
    else invoke.mockImplementation(async (cmd: string) => {
      if (cmd === 'classify_file' && failure !== 'classify') return 'text';
      throw error;
    });
    await expect(s.openFileDialog()).resolves.toBeUndefined();
    expect(s.setStatus).toHaveBeenCalledWith('Could not open file', String(error));
    expect(s.editorTabs).toEqual([old]);
    expect(s.activeEditorId).toBe('keep');
    expect(s.selectedPath).toBe('/project/keep.txt');
    expect(s.renderExplorer).not.toHaveBeenCalled();
  });
  it('cancel preserves dirty tabs, selection and content without host calls', async () => {
    const { sandbox: s, invoke } = harness('files');
    await s.openFile('/project/keep.txt');
    s.editorTabs[0].contents = 'unsaved'; s.editorTabs[0].dirty = true;
    s.openDialog.mockResolvedValue(null); invoke.mockClear();
    const before = JSON.stringify(s.editorTabs);
    await s.openFileDialog();
    expect(JSON.stringify(s.editorTabs)).toBe(before);
    expect(s.selectedPath).toBe('/project/keep.txt');
    expect(invoke).not.toHaveBeenCalled();
    expect(s.renderExplorer).not.toHaveBeenCalled();
  });
});

describe('S04 handoff file placement', () => {
  it('first-open text from Studio selects Files and reveals the canonical bytes without writing', async () => {
    const { sandbox: s, editor, invoke, open } = harness();
    await open();
    expect(s.shellSurface).toBe('files');
    expect(s.filesEditorId).toBe(s.activeEditorId);
    expect(s.paintSurface).toHaveBeenCalledOnce();
    expect(editor.hidden).toBe(false);
    expect(editor.value).toBe('# canonical handoff\nprompt: Moving camera');
    expect(s.editorTabs[0].dirty).toBe(false);
    expect(invoke.mock.calls.map(c => c[0])).toEqual(['classify_file', 'read_text_file']);
  });
  it.each(['files', 'assets'])('new text already in %s retains that surface and remembers the editor', async surface => {
    const { sandbox: s, editor, open } = harness(surface);
    await open();
    expect(s.shellSurface).toBe(surface);
    expect(s.filesEditorId).toBe(s.activeEditorId);
    expect(s.paintSurface).not.toHaveBeenCalled();
    expect(editor.hidden).toBe(false);
  });
  it('reopening text from Studio reveals the existing tab without duplicate reads or lost edits', async () => {
    const { sandbox: s, editor, invoke, open } = harness();
    await open();
    const id = s.activeEditorId;
    s.editorTabs[0].contents = 'unsaved edit';
    s.editorTabs[0].dirty = true;
    s.shellSurface = 'studio'; editor.hidden = true;
    invoke.mockClear();
    await open();
    expect(s.editorTabs).toHaveLength(1);
    expect(s.activeEditorId).toBe(id);
    expect(s.filesEditorId).toBe(id);
    expect(s.shellSurface).toBe('files');
    expect(editor.hidden).toBe(false);
    expect(editor.value).toBe('unsaved edit');
    expect(s.editorTabs[0].dirty).toBe(true);
    expect(invoke).not.toHaveBeenCalled();
  });
  it('new media still selects Files without a text read', async () => {
    const { sandbox: s, invoke, open } = harness('studio', 'video');
    await open('/project/take.mp4');
    expect(s.shellSurface).toBe('files');
    expect(s.filesEditorId).toBe(s.activeEditorId);
    expect(invoke.mock.calls.map(c => c[0])).toEqual(['classify_file']);
  });
  it('unsupported files leave Studio and the editor list untouched', async () => {
    const { sandbox: s, open } = harness('studio', 'unsupported');
    await open();
    expect(s.shellSurface).toBe('studio');
    expect(s.editorTabs).toHaveLength(0);
    expect(s.paintSurface).not.toHaveBeenCalled();
  });
  it('failed text reads leave Studio and existing tabs untouched', async () => {
    const { sandbox: s, open } = harness('studio', 'text', true);
    await expect(open()).rejects.toThrow('read refused');
    expect(s.shellSurface).toBe('studio');
    expect(s.editorTabs).toHaveLength(0);
    expect(s.paintSurface).not.toHaveBeenCalled();
  });
});
