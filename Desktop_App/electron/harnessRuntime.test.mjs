import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';
import * as runtime from '../tests/helpers/runtime.mjs';
const { prepareOutput } = runtime;

// Copying the helper distinguishes module-relative discovery from cwd/hardcoding.
test('app root follows the helper location including a relocated checkout', async t => {
  assert.equal(runtime.appRoot, path.resolve(fileURLToPath(new URL('../', import.meta.url))));
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'studio-relocated-root-'));
  t.after(() => fs.rmSync(parent, { recursive: true, force: true }));
  const root = path.join(parent, 'checkout with spaces');
  const helper = path.join(root, 'tests/helpers/runtime.mjs');
  fs.mkdirSync(path.dirname(helper), { recursive: true });
  fs.copyFileSync(new URL('../tests/helpers/runtime.mjs', import.meta.url), helper);
  const relocated = await import(pathToFileURL(helper).href);
  assert.equal(relocated.appRoot, root);
});

// A recursive mkdir or delete-and-recreate implementation must fail this case.
test('explicit fresh output is created and returned as an absolute directory', t => {
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'studio-runtime-test-'));
  t.after(() => fs.rmSync(parent, { recursive: true, force: true }));
  const target = path.join(parent, 'fresh');
  const relative = path.relative(process.cwd(), target);
  assert.equal(prepareOutput('explicit', relative), target);
  assert.equal(fs.statSync(target).isDirectory(), true);
  assert.throws(() => prepareOutput('explicit', relative), { code: 'EEXIST' });
});

test('STUDIO_TEST_OUT is honored and refused on reuse', t => {
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'studio-runtime-test-'));
  t.after(() => fs.rmSync(parent, { recursive: true, force: true }));
  const previous = process.env.STUDIO_TEST_OUT;
  t.after(() => {
    if (previous === undefined) delete process.env.STUDIO_TEST_OUT;
    else process.env.STUDIO_TEST_OUT = previous;
  });
  process.env.STUDIO_TEST_OUT = path.join(parent, 'from-env');
  assert.equal(prepareOutput('environment'), process.env.STUDIO_TEST_OUT);
  assert.throws(() => prepareOutput('environment'), { code: 'EEXIST' });
});

test('existing files and directory symlinks are refused without modification', t => {
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'studio-runtime-test-'));
  t.after(() => fs.rmSync(parent, { recursive: true, force: true }));
  const file = path.join(parent, 'file');
  fs.writeFileSync(file, 'preserve file');
  assert.throws(() => prepareOutput('file', file), { code: 'EEXIST' });
  assert.equal(fs.readFileSync(file, 'utf8'), 'preserve file');
  const link = path.join(parent, 'link');
  fs.symlinkSync(parent, link, 'dir');
  assert.throws(() => prepareOutput('link', link), { code: 'EEXIST' });
  assert.equal(fs.readlinkSync(link), parent);
});

// A recursive mkdir or delete-and-recreate implementation must fail this case.
test('explicit existing output is refused without deleting its sentinel', t => {
  const parent = fs.mkdtempSync(path.join(os.tmpdir(), 'studio-runtime-test-'));
  t.after(() => fs.rmSync(parent, { recursive: true, force: true }));
  const target = path.join(parent, 'existing');
  fs.mkdirSync(target);
  const sentinel = path.join(target, 'sentinel.txt');
  fs.writeFileSync(sentinel, 'keep this evidence');
  assert.throws(() => prepareOutput('refusal', target), { code: 'EEXIST' });
  assert.equal(fs.readFileSync(sentinel, 'utf8'), 'keep this evidence');
});

test('default output directories are unique absolute children of the OS temp directory', t => {
  const previous = process.env.STUDIO_TEST_OUT;
  delete process.env.STUDIO_TEST_OUT;
  t.after(() => {
    if (previous === undefined) delete process.env.STUDIO_TEST_OUT;
    else process.env.STUDIO_TEST_OUT = previous;
  });
  const first = prepareOutput('unique-default');
  t.after(() => fs.rmSync(first, { recursive: true, force: true }));
  const second = prepareOutput('unique-default');
  t.after(() => fs.rmSync(second, { recursive: true, force: true }));
  assert.notEqual(first, second);
  for (const output of [first, second]) {
    assert.equal(path.isAbsolute(output), true);
    assert.equal(path.dirname(output), path.resolve(os.tmpdir()));
    assert.equal(fs.statSync(output).isDirectory(), true);
    assert.match(path.basename(output), /^unique-default-/);
  }
});
