import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { installEditableContextMenu } from './editableContextMenu.mjs';
test('native clipboard roles use Chromium eligibility for editable context', () => {
  let listener, template, popup;
  const win = { webContents: { on(name, fn) { assert.equal(name, 'context-menu'); listener = fn; } } };
  installEditableContextMenu(win, { buildFromTemplate(t) { template = t; return { popup(opts) { popup = opts; } }; } });
  listener({}, { isEditable: true, editFlags: { canCut: false, canCopy: true, canPaste: false, canSelectAll: true } });
  assert.deepEqual(template, [{ role: 'cut', enabled: false }, { role: 'copy', enabled: true }, { role: 'paste', enabled: false }, { type: 'separator' }, { role: 'selectAll', enabled: true }]);
  assert.equal(popup.window, win);
  const main = readFileSync(new URL('./main.mjs', import.meta.url), 'utf8');
  assert.match(main, /installEditableContextMenu\(mainWindow, Menu, clipboard\)/);
});

test('right_click_readonly_copy_selection uses native selection bytes', () => {
  let listener, template, popup, copied;
  const win = { webContents: { on(_name, fn) { listener = fn; } } };
  const clipboard = { writeText(text) { copied = text; } };
  installEditableContextMenu(win, { buildFromTemplate(items) { template = items; return { popup(options) { popup = options; } }; } }, clipboard);
  listener({}, { isEditable: false, selectionText: 'middle, exactly' });
  assert.equal(popup?.window, win);
  assert.equal(template.length, 1);
  assert.equal(template[0].label, 'Copy Selection');
  template[0].click();
  assert.equal(copied, 'middle, exactly');
  // Renderer must not suppress this unowned event in capture phase.
  const renderer = readFileSync(new URL('../src/main.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(renderer, /if \(!editable\) event\.preventDefault\(\)/);
});
