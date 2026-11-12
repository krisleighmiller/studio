import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('model-sheet-delete-files-refresh');
const root = `${out}/disposable-project`;
const characterName = 'W4 Files Refresh Character';
const characterId = 'w4_files_refresh_character';
const assetId = 'w4_files_refresh_photo';
const photoName = 'files_refresh_pose.png';
const photoRel = `Characters/${characterName}/model_sheet/${photoName}`;
const photoAbs = `${root}/${photoRel}`;
const statePath = `${root}/Preproduction/workflow_state.json`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');

fs.mkdirSync(path.dirname(photoAbs), { recursive: true });
fs.mkdirSync(path.dirname(statePath), { recursive: true });
fs.writeFileSync(`${root}/Characters/${characterName}/character.yaml`, `profile_version: 1\nname: ${characterName}\ndescription: Disposable offline W4 Files refresh fixture\n`);
fs.writeFileSync(photoAbs, Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=', 'base64'));
fs.writeFileSync(`${root}/Characters/${characterName}/model_sheet/model_sheet_manifest.yaml`, `character: ${characterName}\nstyle: disposable\nmodel_id: offline-fixture\nmode: text_only\naspect_ratio: "1:1"\nresolution: fixture\noutput_directory: ${root}/Characters/${characterName}/model_sheet\nidentity_source:\n  character_sheet: ${root}/Characters/${characterName}/character.yaml\n  reference_images: []\n  approval_shot: ''\n  approval_shot_path: ''\nidentity_locked: false\ntext_only_gate: false\nshots:\n  - id: files_refresh_pose\n    status: saved\n    path: ${photoAbs}\n    prediction_id: offline-fixture\n    framing: Disposable pose\npreparation_inputs: {}\nhistory: []\n`);
fs.writeFileSync(statePath, JSON.stringify({
  version: 2,
  state_revision: 1,
  project_name: 'w4-files-refresh',
  scripts: [],
  characters: [{
    id: characterId,
    name: characterName,
    dir: `Characters/${characterName}`,
    profile_path: `Characters/${characterName}/character.yaml`,
    identity_ready: false,
    voice_ready: false,
    profile: { description: 'Disposable offline W4 Files refresh fixture' },
    reference_asset_ids: [],
    asset_ids: [assetId],
  }],
  scenes: [],
  assets: [{
    id: assetId,
    path: photoRel,
    role: 'body_reference',
    status: 'present',
    character_id: characterId,
    notes: 'files_refresh_pose',
  }],
  jobs: [],
  notes: [],
  settings: {},
}, null, 2));

const result = {
  check: 'model_sheet_delete_files_refresh',
  expected: 'In ordinary Electron, a successfully deleted model-sheet photo row disappears from Files without manual Refresh.',
  actual: null,
  command,
  subject: { characterName, characterId, assetId, photoName, photoRel },
  before: null,
  after: null,
  build: null,
  network: null,
  exit: 1,
};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const viteLog = fs.openSync(`${out}/vite.log`, 'w');
const vite = spawn(`${app}/node_modules/.bin/vite`, ['--host', '127.0.0.1', '--port', '1420', '--strictPort'], {
  cwd: app,
  stdio: ['ignore', viteLog, viteLog],
  env: process.env,
});
const electronLog = fs.openSync(`${out}/electron.log`, 'w');
let electron;
let main;
let cdp;
const ev = source => cdp.evaluate(source);
async function click(source) {
  const point = await ev(`(() => { const e=${source}; if(!e || e.disabled || !e.getClientRects().length) throw Error('Missing visible enabled target'); e.scrollIntoView({block:'center',behavior:'instant'}); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
  await cdp.call('Input.dispatchMouseEvent', { type: 'mousePressed', ...point, button: 'left', clickCount: 1 });
  await cdp.call('Input.dispatchMouseEvent', { type: 'mouseReleased', ...point, button: 'left', clickCount: 1 });
  await sleep(180);
}
const row = absolutePath => `[...document.querySelectorAll('#file-tree .tree-item[data-path]')].find(e => e.dataset.path === ${JSON.stringify(absolutePath)})`;

try {
  await until('Vite dev server', async () => { try { return (await fetch('http://127.0.0.1:1420/')).ok; } catch { return false; } });
  electron = spawn(`${app}/node_modules/.bin/electron`, [
    '.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9780', '--remote-debugging-port=9781',
  ], {
    cwd: app,
    stdio: ['ignore', electronLog, electronLog],
    env: {
      ...process.env,
      WORKBENCH_DEV: '1',
      WORKBENCH_HOST_BIN: process.env.WORKBENCH_HOST_BIN ?? '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: `${out}/xdg-config`,
      XDG_CACHE_HOME: `${out}/xdg-cache`,
      XDG_DATA_HOME: `${out}/xdg-data`,
    },
  });
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9780/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9781/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));
  result.build = await main.evaluate(`(() => { const {app,BrowserWindow,dialog,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron'); const w=BrowserWindow.getAllWindows()[0]; w.setSize(1500,1100); dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]}); dialog.showMessageBox=async()=>({response:1,checkboxChecked:false}); globalThis.__w419={blocked:[]}; session.defaultSession.webRequest.onBeforeRequest((r,cb)=>{const remote=/^https?:/.test(r.url)&&!r.url.startsWith('http://127.0.0.1:1420/');if(remote)globalThis.__w419.blocked.push(r.url);cb({cancel:remote});}); return {electron:process.versions.electron, appVersion:app.getVersion(), url:w.webContents.getURL()}; })()`);
  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await click(`document.querySelector('#nav-rail [data-surface="files"]')`);
  await until('Files tree visible', () => ev(`document.querySelector('#file-tree')?.getClientRects().length > 0`));

  for (const folder of [
    `${root}/Characters`,
    `${root}/Characters/${characterName}`,
    `${root}/Characters/${characterName}/model_sheet`,
  ]) {
    await until(`Files row ${folder}`, () => ev(`!!${row(folder)}`));
    await click(row(folder));
  }
  await until('photo row initially visible', () => ev(`!!${row(photoAbs)}`));
  result.before = {
    fileExists: fs.existsSync(photoAbs),
    rowVisible: await ev(`!!${row(photoAbs)} && ${row(photoAbs)}.getClientRects().length > 0`),
    treeText: await ev(`document.querySelector('#file-tree')?.innerText`),
  };
  assert.equal(result.before.fileExists, true);
  assert.equal(result.before.rowVisible, true);

  await click(`document.querySelector('#nav-rail [data-surface="studio"]')`);
  await until('Characters navigation visible', () => ev(`[...document.querySelectorAll('.studio-surface-nav button')].some(b => (b.querySelector('span:not(.studio-surface-dot)')?.textContent ?? '').trim() === 'Characters' && b.getClientRects().length)`));
  await click(`[...document.querySelectorAll('.studio-surface-nav button')].find(b => (b.querySelector('span:not(.studio-surface-dot)')?.textContent ?? '').trim() === 'Characters')`);
  const deleteControl = `[...document.querySelectorAll('img[data-look-path]')].find(e => e.dataset.lookPath.endsWith('/${photoName}'))?.closest('figure')?.querySelector('button')`;
  await until('delete control visible', () => ev(`!!${deleteControl}?.getClientRects().length`));
  await click(deleteControl);
  await until('successful deletion applied', () => ev(`!${deleteControl}`));
  await until('Files row removed automatically', () => ev(`!${row(photoAbs)}`));

  result.after = {
    fileExists: fs.existsSync(photoAbs),
    rowPresent: await ev(`!!${row(photoAbs)}`),
    treeText: await ev(`document.querySelector('#file-tree')?.innerText`),
    status: await ev(`document.querySelector('#status-left')?.textContent`),
    trashEntries: fs.existsSync(`${root}/.workbench-trash`) ? fs.readdirSync(`${root}/.workbench-trash`) : [],
  };
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w419.blocked') };
  assert.equal(result.after.fileExists, false);
  assert.equal(result.after.rowPresent, false);
  assert.ok(result.after.trashEntries.length >= 1);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  result.actual = `PASS: Electron ${result.build.electron} showed ${photoName} in the expanded Files tree before deletion, then removed the row automatically after the successful model-sheet Delete action; no manual Refresh was used.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), blocked: await main.evaluate('globalThis.__w419?.blocked') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({ exit: result.exit, actual: result.actual, error: result.error, result: `${out}/result.json` }, null, 2));
}
process.exitCode = result.exit;
