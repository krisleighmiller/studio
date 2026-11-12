import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, expect, it, vi } from 'vitest';
vi.mock('../src/settingsPanel', () => ({ bindSettingsHost: vi.fn(), getSettings: () => ({}), loadAppSettings: vi.fn(), populateChatModelSelect: vi.fn(), persistSettings: vi.fn(), renderSettingsPanel: vi.fn(), setSelectedChatModelFromSelect: vi.fn(), settingsAreDirty: () => false }));
vi.mock('../src/voicePanel', () => ({ bindVoiceHost: vi.fn(), openVoicePanelContext: vi.fn(), renderVoicePanel: vi.fn() }));
afterEach(() => {
  for (const target of [document, window]) for (const [type, listener, options] of vi.mocked(target.addEventListener).mock.calls) target.removeEventListener(type, listener, options);
  vi.clearAllTimers(); vi.useRealTimers(); vi.restoreAllMocks(); vi.resetModules(); document.body.innerHTML = '';
});
it('lets unowned text reach native clipboard while retaining custom menus', async () => {
  vi.useFakeTimers(); vi.spyOn(document, 'addEventListener'); vi.spyOn(window, 'addEventListener');
  document.body.innerHTML = readFileSync(resolve('index.html'), 'utf8').match(/<body>([\s\S]*)<\/body>/)![1];
  const fixture = JSON.parse(readFileSync('tests/fixtures/sc05-import-result.json', 'utf8'));
  const invoke = async (command: string) => {
    if (command === 'list_dir') return [];
    if (command === 'workflow_import') return structuredClone(fixture);
    throw Error(`Unexpected native/provider call: ${command}`);
  };
  Object.assign(window, { workbench: { onCloseRequested: () => () => {}, invoke, openProject: async () => '/fixture/project', rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {} } });
  await import('../src/main'); await vi.advanceTimersByTimeAsync(100);
  const context = (node: Element) => { const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true }); node.dispatchEvent(event); return event.defaultPrevented; };
  for (const markup of ['<textarea></textarea>', '<input type="text">', '<input type="search">', '<div contenteditable="true"><span>editable</span></div>']) {
    const holder = document.createElement('div'); holder.innerHTML = markup; document.body.append(holder);
    expect(context(holder.querySelector('span') ?? holder.firstElementChild!), markup).toBe(false);
    holder.remove();
  }
  for (const markup of ['<textarea readonly></textarea>', '<input disabled>', '<button>Not text</button>', '<input type="checkbox">']) {
    const holder = document.createElement('div'); holder.innerHTML = markup; document.body.append(holder);
    expect(context(holder.firstElementChild!), markup).toBe(false); holder.remove();
  }
  expect(context(document.querySelector('#editor')!)).toBe(true);
  expect(document.querySelector('#context-menu')?.textContent).toContain('Find');
  expect(context(document.querySelector('#editor-tabs')!)).toBe(true);
  expect(document.querySelector('#context-menu')?.textContent).toContain('New Text File');
  expect(context(document.querySelector('#file-tree')!)).toBe(false); // no project yet
  document.querySelector<HTMLButtonElement>('#home-open-project')!.click();
  await vi.advanceTimersByTimeAsync(150);
  expect(context(document.querySelector('#file-tree')!)).toBe(true);
});
