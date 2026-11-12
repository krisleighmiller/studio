import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-readable-type');
const root = `${out}/disposable-project`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
fs.mkdirSync(root, { recursive: true });
fs.writeFileSync(`${root}/sentinel.txt`, 'Disposable offline W1-03 project\n');

const result = {
  check: 'request_visibility_readable_type',
  expected: 'The delayed request primary status line is visible at normal body-text size and color through the workflow-banner hierarchy, not helper/meta typography.',
  actual: null,
  command,
  control: null,
  surface: null,
  build: null,
  viewport: null,
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
  electron = spawn(`${app}/node_modules/.bin/electron`, ['.', '--no-sandbox', '--disable-gpu', '--inspect=127.0.0.1:9790', '--remote-debugging-port=9791'], {
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
  const mt = await until('main target', async () => { try { return (await (await fetch('http://127.0.0.1:9790/json/list')).json())[0]; } catch { return null; } });
  main = await CDP.open(mt.webSocketDebuggerUrl);
  await main.call('Runtime.enable');
  const rt = await until('renderer target', async () => { try { return (await (await fetch('http://127.0.0.1:9791/json/list')).json()).find(target => target.type === 'page'); } catch { return null; } });
  cdp = await CDP.open(rt.webSocketDebuggerUrl);
  await until('ready', () => ev(`document.querySelector('#status-left')?.textContent?.startsWith('Ready —')`));

  const mainIdentity = await main.evaluate(`(() => {
    const {app,BrowserWindow,dialog,ipcMain,session}=process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron');
    const w=BrowserWindow.getAllWindows()[0];
    w.setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w103={calls:[],blocked:[],resolve:null};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(cmd==='workflow_generate_script'){
        globalThis.__w103.calls.push({cmd,payload});
        return await new Promise(resolve=>{globalThis.__w103.resolve=resolve;});
      }
      return await original(...args);
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w103.blocked.push(request.url);
      callback({cancel:remote});
    });
    return {electron:process.versions.electron,appVersion:app.getVersion(),url:w.webContents.getURL(),mainPid:process.pid,rendererPid:w.webContents.getOSProcessId()};
  })()`);
  result.build = {
    ...mainIdentity,
    gitHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: app, encoding: 'utf8' }).trim(),
    dirty: Boolean(execFileSync('git', ['status', '--porcelain'], { cwd: app, encoding: 'utf8' }).trim()),
    sourceSha256: {
      workflowDesk: crypto.createHash('sha256').update(fs.readFileSync(`${app}/src/workflowDesk.ts`)).digest('hex'),
      requestReceipt: crypto.createHash('sha256').update(fs.readFileSync(`${app}/src/studio/requestReceipt.ts`)).digest('hex'),
    },
  };

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await ev(`import('/src/settingsPanel.ts').then(m=>m.setSelectedChatModelFromSelect('offline-fixture::no-submit-model'))`);
  if (!await ev(`!!document.querySelector('.workflow-script-draft')`)) {
    await click(visibleButton('Create script'));
  }
  await until('script draft', () => ev(`document.querySelector('.workflow-script-draft')?.getAttribute('aria-busy') === 'false'`));
  await ev(`(() => { const n=document.querySelector('.workflow-script-draft [name="notes"]'); n.value='Disposable W1-03 delayed prompt, no provider call.'; n.dispatchEvent(new Event('input',{bubbles:true})); })()`);

  await click(visibleButton('Generate script from notes'));
  await until('editable request prompt', () => ev(`!!document.querySelector('dialog[data-generation-prompt][open]')`));
  await click(visibleButton('Prepare request'));
  await until('host request held', () => main.evaluate(`globalThis.__w103.calls.length === 1 && typeof globalThis.__w103.resolve === 'function'`));
  await until('working receipt', () => ev(`document.querySelector('[data-request-receipt][data-request-outcome="working"]')?.textContent?.includes('Generate script from notes')`));

  const readback = await ev(`(() => {
    const control=[...document.querySelectorAll('.workflow-script-draft button')].find(e=>e.textContent?.trim()==='Generating script…');
    const receipt=document.querySelector('[data-request-receipt][data-request-outcome="working"]');
    const primary=receipt.querySelector('strong');
    const bodyStyle=getComputedStyle(document.body), cardStyle=getComputedStyle(receipt), primaryStyle=getComputedStyle(primary);
    const rect=e=>{const r=e.getBoundingClientRect();return{x:r.x,y:r.y,width:r.width,height:r.height,bottom:r.bottom,right:r.right}};
    const primaryRect=rect(primary);
    return {
      viewport:{width:innerWidth,height:innerHeight,scrollX,scrollY},
      control:{label:control.textContent.trim(),disabled:control.disabled,rect:rect(control)},
      activeStatus:{
        role:receipt.getAttribute('role'), outcome:receipt.dataset.requestOutcome, text:receipt.innerText,
        cardClasses:[...receipt.classList], primaryClasses:[...primary.classList], primaryText:primary.textContent,
        bodyFontSize:bodyStyle.fontSize, fontSize:primaryStyle.fontSize,
        bodyColor:bodyStyle.color, color:primaryStyle.color,
        visibility:primaryStyle.visibility, display:primaryStyle.display, opacity:primaryStyle.opacity,
        rect:primaryRect, visible:primary.getClientRects().length>0 && primaryRect.width>0 && primaryRect.height>0 && primaryStyle.visibility==='visible' && primaryStyle.display!=='none' && Number(primaryStyle.opacity)>0,
        cardVisibility:cardStyle.visibility, cardDisplay:cardStyle.display,
      },
    };
  })()`);
  result.control = readback.control;
  result.surface = { label: 'Script', originatingControl: 'Generate script from notes', confirmingControl: 'Prepare request', statusRegion: '[data-request-receipt][data-request-outcome="working"]' };
  result.viewport = readback;
  result.hostCalls = await main.evaluate('globalThis.__w103.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w103.blocked') };
  assert.equal(readback.activeStatus.outcome, 'working');
  assert.equal(readback.activeStatus.visible, true);
  assert.ok(readback.activeStatus.cardClasses.includes('workflow-banner'));
  assert.ok(!readback.activeStatus.cardClasses.includes('workflow-field-hint'));
  assert.ok(!readback.activeStatus.primaryClasses.includes('workflow-field-hint'));
  assert.ok(parseFloat(readback.activeStatus.fontSize) >= parseFloat(readback.activeStatus.bodyFontSize));
  assert.equal(readback.activeStatus.color, readback.activeStatus.bodyColor);
  assert.equal(result.hostCalls.length, 1);
  assert.equal(result.hostCalls[0].cmd, 'workflow_generate_script');
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/readable-active-status.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: the visible primary status line "${readback.activeStatus.primaryText}" computed to ${readback.activeStatus.fontSize} and ${readback.activeStatus.color}, matching the ${readback.activeStatus.bodyFontSize} body hierarchy and body color through the workflow-banner class rather than helper/meta typography.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w103?.calls'), blocked: await main.evaluate('globalThis.__w103?.blocked') }; } catch {}
} finally {
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await main.evaluate(`globalThis.__w103?.resolve?.({cancelled:true}); process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/readable-active-status.png`},null,2));
}
process.exitCode = result.exit;
