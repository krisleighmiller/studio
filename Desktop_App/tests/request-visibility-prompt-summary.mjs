import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w1-12/runtime-353`;
const root = `${out}/disposable-project`;
const command = "docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/request-visibility-prompt-summary.mjs'";
const originalNotes = 'Disposable original notes that must not survive editing.';
const submittedNotes = 'Café 🚀 雪 <tag> & "quotes"\nSecond line: naïve façade — exact.';
const expectedSummary = `notes: ${submittedNotes}`;
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-12 project\n');

const result = {
  check: 'request_visibility_prompt_summary',
  expected: 'Edited Unicode creative text containing <tag> is echoed exactly and inertly in the visible request summary, matches the final workflow_generate_script payload, and exposes no unrelated arguments.',
  actual: null,
  command,
  readback: null,
  hostCalls: null,
  network: null,
  exit: 1,
};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const viteLog = fs.openSync(`${out}/vite.log`, 'w');
const vite = spawn(`${app}/node_modules/.bin/vite`, ['--host', '127.0.0.1', '--port', '1420', '--strictPort'], {
  cwd: app, stdio: ['ignore', viteLog, viteLog], env: process.env,
});
const electronLog = fs.openSync(`${out}/electron.log`, 'w');
let electron;
let main;
let cdp;
const ev = source => cdp.evaluate(source);
const visibleButton = label => `[...document.querySelectorAll('button')].find(e => e.textContent?.trim() === ${JSON.stringify(label)} && e.getClientRects().length)`;
async function click(source) {
  const point = await ev(`(() => { const e=${source}; if(!e || e.disabled || !e.getClientRects().length) throw Error('Missing visible enabled target'); const r=e.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
  await cdp.call('Input.dispatchMouseEvent', { type: 'mousePressed', ...point, button: 'left', clickCount: 1 });
  await cdp.call('Input.dispatchMouseEvent', { type: 'mouseReleased', ...point, button: 'left', clickCount: 1 });
  await sleep(180);
}

try {
  await until('Vite dev server', async () => { try { return (await fetch('http://127.0.0.1:1420/')).ok; } catch { return false; } });
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9810', '--remote-debugging-port=9811'], {
    cwd: app,
    stdio: ['ignore', electronLog, electronLog],
    env: {
      ...process.env,
      WORKBENCH_DEV: '1',
      WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: '/runtime/w1-12-config',
      XDG_CACHE_HOME: '/runtime/w1-12-cache',
      XDG_DATA_HOME: '/runtime/w1-12-data',
    },
  });
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9810/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9811/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w112={calls:[],blocked:[],resolve:null,settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w112.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w112.resolve=value=>{globalThis.__w112.settled=true;resolve(value);};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w112.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await click(visibleButton('Studio'));
  await until('Studio workspace', () => ev(`!!document.querySelector('.workflow-main')`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value=${JSON.stringify(originalNotes)}; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);

  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await ev(`(() => { const n=document.querySelector('dialog[data-generation-prompt] textarea[name="prompt"]'); n.value=${JSON.stringify(submittedNotes)}; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);
  await click(visibleButton('Prepare request'));
  await until('host request held', () => main.evaluate(`globalThis.__w112.calls.length === 1 && typeof globalThis.__w112.resolve === 'function' && !globalThis.__w112.settled`));
  await until('exact prompt summary', () => ev(`document.querySelector('[data-request-receipt][data-request-outcome="working"] pre')?.textContent === ${JSON.stringify(expectedSummary)}`));

  result.hostCalls = await main.evaluate('globalThis.__w112.calls');
  result.network = {
    blockedRemoteRequests: await main.evaluate('globalThis.__w112.blocked'),
    hostSettled: await main.evaluate('globalThis.__w112.settled'),
  };
  result.readback = await ev(`(() => {
    const receipt=document.querySelector('[data-request-receipt][data-request-outcome="working"]');
    const summary=receipt?.querySelector('pre');
    const r=summary?.getBoundingClientRect();
    const style=summary?getComputedStyle(summary):null;
    return {
      summaryText:summary?.textContent??null,
      summaryHtml:summary?.innerHTML??null,
      tagElementCount:summary?.querySelectorAll('tag').length??null,
      childElementCount:summary?.childElementCount??null,
      visible:!!summary&&summary.getClientRects().length>0&&r.width>0&&r.height>0&&style.visibility==='visible'&&style.display!=='none'&&Number(style.opacity)>0,
      inInitialViewport:!!r&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth&&scrollX===0&&scrollY===0,
      jobsOpen:!!document.querySelector('[data-surface="jobs"]'),
    };
  })()`);

  const payload = result.hostCalls[0]?.payload;
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_generate_script');
  assert.equal(payload.notes, submittedNotes);
  assert.equal(result.readback.summaryText, expectedSummary);
  assert.equal(result.readback.summaryText.slice('notes: '.length), payload.notes);
  assert.equal(result.readback.summaryText.includes(originalNotes), false);
  for (const unrelated of ['projectRoot:', 'apiId:', 'modelId:', 'replaceExisting:']) assert.equal(result.readback.summaryText.includes(unrelated), false);
  assert.equal(result.readback.tagElementCount, 0);
  assert.equal(result.readback.childElementCount, 0);
  assert.equal(result.readback.summaryHtml.includes('&lt;tag&gt;'), true);
  assert.equal(result.readback.visible, true);
  assert.equal(result.readback.inInitialViewport, true);
  assert.equal(result.readback.jobsOpen, false);
  assert.equal(result.network.hostSettled, false);
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/exact-inert-prompt-summary.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: the visible summary echoed the edited Unicode notes exactly, matched the final ${result.hostCalls[0].cmd} payload, rendered <tag> as inert text, omitted unrelated arguments, and remained in the initial viewport.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w112?.calls'), blocked: await main.evaluate('globalThis.__w112?.blocked'), settled: await main.evaluate('globalThis.__w112?.settled') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w112?.resolve?.({cancelled:true}); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.quit()`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/exact-inert-prompt-summary.png`},null,2));
}
process.exitCode = result.exit;
