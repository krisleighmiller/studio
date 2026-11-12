import assert from 'node:assert/strict';
import fs from 'node:fs';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w4-05/runtime-313`;
const command = 'workflow_delete_model_sheet_photo';
const args = {
  projectRoot: `${out}/missing-disposable-project`,
  characterId: 'w4_bridge_character_owner',
  assetId: 'w4_bridge_photo_selected',
};
const invocation = `docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/model-sheet-delete-electron-bridge.mjs'`;

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(args.projectRoot, { recursive: true });
const result = {
  check: 'model_sheet_delete_electron_bridge',
  expected: 'One ordinary Electron invocation forwards the exact command and {projectRoot, characterId, assetId} once through preload, ipcMain and the sidecar.',
  actual: null,
  command: invocation,
  requested: { command, args },
  observed: null,
  exit: 1,
};

const viteLog = fs.openSync(`${out}/vite.log`, 'w');
const vite = spawn(`${app}/node_modules/.bin/vite`, ['--host', '127.0.0.1', '--port', '1420', '--strictPort'], {
  cwd: app,
  stdio: ['ignore', viteLog, viteLog],
  env: process.env,
});
const electronLog = fs.openSync(`${out}/electron.log`, 'w');
let electron;
let main;
let page;

try {
  await until('Vite dev server', async () => {
    try { return (await fetch('http://127.0.0.1:1420/')).ok; } catch { return false; }
  });
  electron = spawn(`${app}/node_modules/.bin/electron`, [
    '.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9770', '--remote-debugging-port=9771',
  ], {
    cwd: app,
    stdio: ['ignore', electronLog, electronLog],
    env: {
      ...process.env,
      WORKBENCH_DEV: '1',
      WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: '/tmp/w4-05-config',
      XDG_CACHE_HOME: '/tmp/w4-05-cache',
      XDG_DATA_HOME: '/tmp/w4-05-data',
    },
  });
  const mainTarget = await until('Electron main target', async () => {
    try { return (await (await fetch('http://127.0.0.1:9770/json/list')).json())[0]; } catch { return null; }
  });
  main = await CDP.open(mainTarget.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const pageTarget = await until('Electron renderer target', async () => {
    try { return (await (await fetch('http://127.0.0.1:9771/json/list')).json()).find(target => target.type === 'page'); } catch { return null; }
  });
  page = await CDP.open(pageTarget.webSocketDebuggerUrl);
  await until('preload bridge', () => page.evaluate('typeof window.workbench?.invoke === "function"'));

  result.build = await main.evaluate(`(() => {
    const { app, BrowserWindow, dialog, ipcMain } = process.getBuiltinModule('module').createRequire(process.cwd() + '/electron/main.mjs')('electron');
    const window = BrowserWindow.getAllWindows()[0];
    dialog.showOpenDialog = async () => ({ canceled: false, filePaths: [${JSON.stringify(args.projectRoot)}] });
    const original = ipcMain._invokeHandlers.get('host:invoke');
    if (!original) throw new Error('host:invoke handler unavailable');
    globalThis.__w405 = { calls: [] };
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke', async (...values) => {
      const record = { command: values[1], args: values[2], outcome: null };
      globalThis.__w405.calls.push(record);
      try {
        const response = await original(...values);
        record.outcome = { ok: true, response };
        return response;
      } catch (error) {
        record.outcome = { ok: false, error: String(error) };
        throw error;
      }
    });
    return {
      electron: process.versions.electron,
      appVersion: app.getVersion(),
      url: window.webContents.getURL(),
      mainPid: process.pid,
      rendererPid: window.webContents.getOSProcessId(),
    };
  })()`);

  const openedRoot = await page.evaluate('window.workbench.openProject()');
  assert.equal(openedRoot, args.projectRoot);
  const rendererOutcome = await page.evaluate(`window.workbench.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})
    .then(value => ({ ok: true, value }))
    .catch(error => ({ ok: false, error: String(error) }))`);
  const calls = await main.evaluate('globalThis.__w405.calls');
  result.observed = { calls, rendererOutcome };

  assert.equal(calls.length, 1);
  assert.equal(calls[0].command, command);
  assert.deepEqual(calls[0].args, args);
  assert.equal(calls[0].outcome.ok, false);
  assert.match(calls[0].outcome.error, new RegExp('character `' + args.characterId + '` not found'));
  assert.equal(rendererOutcome.ok, false);
  assert.doesNotMatch(rendererOutcome.error, /not available to the window|sidecar is not started/i);

  result.actual = `PASS: Electron ${result.build.electron} forwarded ${command} exactly once with the exact three-field payload; Rust dispatch returned the expected character-not-found error for the disposable project.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.observed = { calls: await main.evaluate('globalThis.__w405?.calls') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) {
    try { await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd() + '/electron/main.mjs')('electron').app.quit()`); } catch {}
  }
  page?.close();
  main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({ exit: result.exit, actual: result.actual, error: result.error, result: `${out}/result.json` }, null, 2));
}

process.exitCode = result.exit;
