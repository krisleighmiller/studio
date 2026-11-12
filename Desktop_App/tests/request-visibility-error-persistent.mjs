import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { CDP, until } from '/source/Studio/Desktop_App/tests/ui-tracer.mjs';

const app = '/source/Studio/Desktop_App';
const out = `${app}/evidence/w1-22/runtime-363`;
const root = `${out}/disposable-project`;
const command = "docker exec studio-dev-workbench-1 bash -lc 'cd /source/Studio/Desktop_App && xvfb-run -a node tests/request-visibility-error-persistent.mjs'";
const refusal = 'Offline fixture refused this request. Check the model access and try again.';
const expectedError = 'Submitted · Error — Generate script from notes';
fs.rmSync(out, { recursive: true, force: true });
for (const dir of ['/runtime/w1-22-config', '/runtime/w1-22-cache', '/runtime/w1-22-data']) fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-22 project\n');

const result = {
  check: 'request_visibility_error_persistent',
  expected: 'After one delayed request is refused away from its origin, returning to the origin keeps the plain-language error prominent and the initiating control retryable.',
  actual: null, command, readback: null, hostCalls: null, network: null, exit: 1,
};
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const viteLog = fs.openSync(`${out}/vite.log`, 'w');
const vite = spawn(`${app}/node_modules/.bin/vite`, ['--host', '127.0.0.1', '--port', '1420', '--strictPort'], { cwd: app, stdio: ['ignore', viteLog, viteLog], env: process.env });
const electronLog = fs.openSync(`${out}/electron.log`, 'w');
let electron, main, cdp;
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
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9814', '--remote-debugging-port=9815'], {
    cwd: app, stdio: ['ignore', electronLog, electronLog],
    env: { ...process.env, WORKBENCH_DEV: '1', WORKBENCH_HOST_BIN: '/runtime/target/debug/workbench-host', XDG_CONFIG_HOME: '/runtime/w1-22-config', XDG_CACHE_HOME: '/runtime/w1-22-cache', XDG_DATA_HOME: '/runtime/w1-22-data' },
  });
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9814/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl); await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9815/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  await main.evaluate(`(() => {
    const {BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w122={calls:[],blocked:[],reject:null,settled:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w122.calls.push({cmd,payload});
        return await new Promise((_,reject)=>{globalThis.__w122.reject=()=>{globalThis.__w122.settled=true;reject(new Error(${JSON.stringify(refusal)}));};});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w122.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) await click(visibleButton('Create script'));
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-22 refused prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);
  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('host request held', () => main.evaluate(`globalThis.__w122.calls.length===1 && typeof globalThis.__w122.reject==='function' && !globalThis.__w122.settled`));
  await click(`[...document.querySelectorAll('.studio-surface-nav button')].find(e => [...e.children].some(c => c.textContent?.trim() === 'Profile'))`);
  await until('Profile surface', () => ev(`[...document.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].some(e => [...e.children].some(c => c.textContent?.trim() === 'Profile'))`));
  await main.evaluate('globalThis.__w122.reject()');
  await until('refusal settled away from origin', () => main.evaluate('globalThis.__w122.settled'));
  await until('active indicator cleared', () => ev(`!document.querySelector('[data-global-request-indicator]')`));
  await click(`[...document.querySelectorAll('.studio-surface-nav button')].find(e => [...e.children].some(c => c.textContent?.trim() === 'Script'))`);
  await until('Script surface', () => ev(`[...document.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].some(e => [...e.children].some(c => c.textContent?.trim() === 'Script'))`));
  await until('persistent refusal receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="error"] strong')?.textContent === ${JSON.stringify(expectedError)}`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="error"]');
    const heading=receipt.querySelector('strong'), detail=receipt.querySelector('p');
    const retry=[...workspace.querySelectorAll('button')].find(e=>e.textContent?.trim()==='Generate script from notes');
    const r=receipt.getBoundingClientRect(), style=getComputedStyle(receipt);
    return {
      activeSurface:[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Script')?.textContent?.trim()).filter(Boolean),
      status:heading.textContent, detail:detail.textContent, outcome:receipt.dataset.requestOutcome, role:receipt.getAttribute('role'),
      visible:receipt.getClientRects().length>0&&r.width>0&&r.height>0&&style.visibility==='visible'&&style.display!=='none'&&Number(style.opacity)>0,
      inInitialViewport:r.top>=0&&r.left>=0&&r.bottom<=innerHeight&&r.right<=innerWidth&&scrollX===0&&scrollY===0,
      fontSize:style.fontSize, retry:{label:retry?.textContent?.trim()??null,disabled:retry?.disabled??null,ariaBusy:retry?.getAttribute('aria-busy')??null},
      jobsExpanded:document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded')??null,
    };
  })()`);
  result.readback=readback;
  result.hostCalls=await main.evaluate('globalThis.__w122.calls');
  result.network={blockedRemoteRequests:await main.evaluate('globalThis.__w122.blocked'),hostSettled:await main.evaluate('globalThis.__w122.settled')};
  assert.deepEqual(readback.activeSurface,['Script']);
  assert.equal(readback.status,expectedError); assert.equal(readback.detail,refusal); assert.equal(readback.outcome,'error'); assert.equal(readback.role,'alert');
  assert.equal(readback.visible,true); assert.equal(readback.inInitialViewport,true); assert.ok(Number.parseFloat(readback.fontSize)>=14);
  assert.deepEqual(readback.retry,{label:'Generate script from notes',disabled:false,ariaBusy:'false'});
  assert.equal(readback.jobsExpanded,'false'); assert.equal(result.hostCalls.length,1); assert.equal(result.hostCalls[0].cmd,'workflow_generate_script');
  assert.equal(result.network.hostSettled,true); assert.deepEqual(result.network.blockedRemoteRequests,[]);
  await click(visibleButton('Generate script from notes'));
  await until('retry prompt opened', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  assert.equal(await main.evaluate('globalThis.__w122.calls.length'),1);
  const shot=await cdp.call('Page.captureScreenshot',{format:'png'}); fs.writeFileSync(`${out}/persistent-refusal-retry.png`,Buffer.from(shot.data,'base64'));
  result.actual=`PASS: after leaving Script and rejecting the delayed request, returning showed "${readback.status}" with plain-language detail in the initial viewport; the initiating control reopened its retry prompt.`;
  result.exit=0;
} catch(error) {
  result.error=error.stack;
  try { result.diagnostic={body:await ev('document.body.innerText'),calls:await main.evaluate('globalThis.__w122?.calls'),blocked:await main.evaluate('globalThis.__w122?.blocked'),settled:await main.evaluate('globalThis.__w122?.settled')}; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`,JSON.stringify(result,null,2));
  if(main){try{await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.quit()`);}catch{}}
  cdp?.close(); main?.close(); if(electron?.exitCode===null)electron.kill('SIGTERM'); if(vite.exitCode===null)vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/persistent-refusal-retry.png`},null,2));
}
process.exitCode=result.exit;
