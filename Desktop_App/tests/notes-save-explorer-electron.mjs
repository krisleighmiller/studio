import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w6-c1/runtime-308`;
const root = `${out}/disposable-project`;
const file = `${root}/Notes.md`;
const exact = 'W6-C1 same-runtime publication proof — café 雨 🎬\n';
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable W6-C1 project\n');
const result = {
  expected: 'Save notes writes project-root Notes.md and publishes a visible Explorer row without reopen or manual Refresh.',
  actual: null,
  boundary: null,
  identity: null,
  hashes: {},
  exit: 1,
};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const electronLog = fs.openSync(`${out}/electron.log`, 'w');
const electron = spawn(`${app}/node_modules/.bin/electron`, [
  '.', '--disable-gpu', '--inspect=127.0.0.1:9740', '--remote-debugging-port=9741',
], {
  cwd: app,
  stdio: ['ignore', electronLog, electronLog],
  env: {
    ...process.env,
    WORKBENCH_DEV: '1',
    WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
    XDG_CONFIG_HOME: '/tmp/w6-c1-config',
    XDG_CACHE_HOME: '/tmp/w6-c1-cache',
    XDG_DATA_HOME: '/tmp/w6-c1-data',
  },
});
let main;
let cdp;
const ev = source => cdp.evaluate(source);
const button = label => `[...document.querySelectorAll('button')].find(e => (e.querySelector('span:not([class])')?.textContent ?? e.textContent).trim() === ${JSON.stringify(label)} && e.getClientRects().length)`;
async function click(selector) {
  const point = await ev(`(() => { const e=${selector}; if(!e || e.disabled || !e.getClientRects().length) throw Error('Missing visible enabled target'); e.scrollIntoView({block:'center',behavior:'instant'}); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
  await cdp.call('Input.dispatchMouseEvent', { type: 'mousePressed', ...point, button: 'left', clickCount: 1 });
  await cdp.call('Input.dispatchMouseEvent', { type: 'mouseReleased', ...point, button: 'left', clickCount: 1 });
  await sleep(180);
}
try {
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9740/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9741/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));
  result.identity = await main.evaluate(`(() => { const {BrowserWindow,ipcMain,session,dialog}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron'); const w=BrowserWindow.getAllWindows()[0]; w.setSize(1500,1100); dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]}); globalThis.__w6={calls:[],blocked:[]}; const orig=ipcMain._invokeHandlers.get('host:invoke'); ipcMain.removeHandler('host:invoke'); ipcMain.handle('host:invoke',async(...a)=>{const rec={cmd:a[1],args:a[2],started:new Date().toISOString()};globalThis.__w6.calls.push(rec);try{const value=await orig(...a);rec.ok=true;rec.settled=new Date().toISOString();if(a[1]==='list_dir')rec.response=value;if(a[1]==='save_project_notes')rec.response=value;return value;}catch(e){rec.ok=false;rec.error=String(e);rec.settled=new Date().toISOString();throw e;}}); session.defaultSession.webRequest.onBeforeRequest((r,cb)=>{const remote=/^https?:/.test(r.url)&&!r.url.startsWith('http://127.0.0.1:1420/');if(remote)globalThis.__w6.blocked.push(r.url);cb({cancel:remote});}); return {url:w.webContents.getURL(),mainPid:process.pid,rendererPid:w.webContents.getOSProcessId(),xid:w.getNativeWindowHandle().readUInt32LE(0)}; })()`);
  for (const rel of ['src/workflowDesk.ts', 'src/main.ts', 'dist/index.html']) {
    result.hashes[rel] = crypto.createHash('sha256').update(fs.readFileSync(`${app}/${rel}`)).digest('hex');
  }
  await click(`document.querySelector('#home-open-project')`);
  result.nativePicker = 'Dialog result supplied to this disposable project only; save/list IPC remained ordinary.';
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await sleep(500);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) {
    await click(button('Script'));
    await click(button('Create script'));
  }
  await until('draft ready', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const e=document.querySelector('.workflow-script-draft [name="notes"]'); e.focus(); e.value=${JSON.stringify(exact)}; e.dispatchEvent(new Event('input',{bubbles:true})); })()`);
  await main.evaluate('globalThis.__w6.calls=[]');
  await click(button('Save notes'));
  await until('save settled', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await until('Explorer Notes row publication', () => ev(`!![...document.querySelectorAll('#file-tree [data-path]')].find(e => e.dataset.path === ${JSON.stringify(file)})`));
  await click(`document.querySelector('.nav-item[data-surface="files"]')`);
  await until('visible Explorer Notes row', () => ev(`(() => { const e=[...document.querySelectorAll('#file-tree [data-path]')].find(e => e.dataset.path === ${JSON.stringify(file)}); return !!e && e.getClientRects().length > 0; })()`));
  await sleep(200);
  const visible = await ev(`(() => { const e=[...document.querySelectorAll('#file-tree [data-path]')].find(e => e.dataset.path === ${JSON.stringify(file)}); const r=e.getBoundingClientRect(); return {text:e.textContent,path:e.dataset.path,rect:{x:r.x,y:r.y,width:r.width,height:r.height},display:getComputedStyle(e).display,visibility:getComputedStyle(e).visibility,tree:[...document.querySelectorAll('#file-tree [data-path]')].map(n=>({path:n.dataset.path,text:n.textContent}))}; })()`);
  const calls = await main.evaluate('globalThis.__w6.calls');
  const disk = fs.readFileSync(file, 'utf8');
  const saveCall = calls.find(call => call.cmd === 'save_project_notes');
  const listCall = calls.find(call => call.cmd === 'list_dir' && call.args?.path === root && call.response?.some?.(entry => entry.path === file));
  assert.equal(disk, exact);
  assert.equal(saveCall?.ok, true);
  assert.equal(saveCall?.response, 'Notes.md');
  assert.equal(listCall?.ok, true);
  assert.ok(visible.rect.width > 0 && visible.rect.height > 0);
  assert.equal(visible.visibility, 'visible');
  assert.equal(visible.display, 'flex');
  result.actual = 'PASS: native save settled, list_dir returned Notes.md, and the Explorer row was visibly published without reopen/manual Refresh.';
  result.boundary = { calls, disk, visible, blocked: await main.evaluate('globalThis.__w6.blocked') };
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/visible-notes-row.png`, Buffer.from(shot.data, 'base64'));
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.boundary = { calls: await main.evaluate('globalThis.__w6.calls'), body: await ev('document.body.innerText') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.quit()`); } catch {} }
  cdp?.close(); main?.close();
  if (electron.exitCode === null) electron.kill('SIGTERM');
  console.log(JSON.stringify({ exit: result.exit, actual: result.actual, error: result.error, out }, null, 2));
}
process.exitCode = result.exit;
