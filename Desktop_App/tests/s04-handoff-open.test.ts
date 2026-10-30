/** Execute production declarations with an isolated host/paint seam.
 * Full built-Electron visibility proof is retained in S04/run24 as well.
 */
import { describe, expect, it, vi } from 'vitest';
import ts from 'typescript';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import vm from 'node:vm';

const source = readFileSync(resolve('src/main.ts'), 'utf8');
const ast = ts.createSourceFile('main.ts', source, ts.ScriptTarget.Latest, true);
const names = ['openFile', 'placeFileSurface', 'rememberFileTab', 'isFileTab'];
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
