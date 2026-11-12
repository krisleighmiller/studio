import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-job-progress');
const root = `${out}/disposable-project`;
const shimDir = `${out}/bin`;
const ffmpegStarted = `${out}/ffmpeg-started`;
const ffmpegRelease = `${out}/ffmpeg-release`;
const statePath = `${root}/Preproduction/workflow_state.json`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
const expectedWorking = 'Submitted · Working — Preparing assembly…';
const expectedQueued = 'Submitted · Queued job — Preparing assembly…';
const expectedQueuedIndicator = '1 active job · Queued — Preparing assembly…';
const expectedRunning = 'Submitted · Running job — Preparing assembly…';
const expectedRunningIndicator = '1 active job · Running — Preparing assembly…';
for (const dir of ['Preproduction', 'Scripts', 'Scene_Prompts/Scene1', 'Scenes/Scene1/Cut1']) fs.mkdirSync(`${root}/${dir}`, { recursive: true });
fs.mkdirSync(shimDir, { recursive: true });
fs.writeFileSync(`${root}/Scripts/script.md`, '# Disposable story\n\n## Scene 1 — W1 native progress\nAlex: This is a bounded offline assembly fixture.\n');
fs.writeFileSync(`${root}/Scene_Prompts/Scene1/Cut1.md`, 'source_scene: 1\ncut: 1\ntitle: Offline shot\nscene_type: b_roll\ncontinuity:\n  group: test\n  relation_to_previous: new_setup\nediting:\n  transition_in: cut\n  transition_out: cut\nspeech:\n  mode: none\nprompt: bounded offline fixture\n');
const realFfmpeg = spawnSync('sh', ['-lc', 'command -v ffmpeg'], { encoding: 'utf8' }).stdout.trim();
assert.ok(realFfmpeg, 'ffmpeg must be available for the offline fixture');
const generated = spawnSync(realFfmpeg, ['-v', 'error', '-f', 'lavfi', '-i', 'color=c=blue:s=160x90:r=24:d=1', '-c:v', 'libx264', '-threads', '1', '-pix_fmt', 'yuv420p', `${root}/Scenes/Scene1/Cut1/take.mp4`]);
assert.equal(generated.status, 0, generated.stderr?.toString());
fs.writeFileSync(`${shimDir}/ffmpeg`, `#!/bin/sh\nprintf '%s\\n' "$*" > ${JSON.stringify(ffmpegStarted)}\nwhile [ ! -f ${JSON.stringify(ffmpegRelease)} ]; do sleep 0.1; done\nexec ${JSON.stringify(realFfmpeg)} "$@"\n`, { mode: 0o755 });

const result = {
  check: 'request_visibility_job_progress',
  expected: 'One actual persisted native job automatically changes from queued to running through the ordinary read-only Jobs poll while Jobs stays collapsed.',
  actual: null,
  command,
  readback: null,
  hostCalls: null,
  persisted: null,
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
      PATH: `${shimDir}:${process.env.PATH}`,
      WORKBENCH_DEV: '1',
      WORKBENCH_HOST_BIN: process.env.WORKBENCH_HOST_BIN ?? '/runtime/target/debug/workbench-host',
      XDG_CONFIG_HOME: `${out}/xdg-config`,
      XDG_CACHE_HOME: `${out}/xdg-cache`,
      XDG_DATA_HOME: `${out}/xdg-data`,
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
    const nodeFs=process.getBuiltinModule('fs');
    BrowserWindow.getAllWindows()[0].setSize(1600,1200);
    dialog.showOpenDialog=async()=>({canceled:false,filePaths:[${JSON.stringify(root)}]});
    globalThis.__w118={calls:[],blocked:[],fixtureReady:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(['workflow_prepare_assemble','director_approve','workflow_status'].includes(cmd)) globalThis.__w118.calls.push({cmd,payload});
      const answer=await original(...args);
      if(cmd==='workflow_import' && !globalThis.__w118.fixtureReady){
        const scene=answer.state.scenes[0], cut=scene?.cuts?.[0];
        if(!scene || !cut) throw Error('Native import did not create the disposable scene/cut');
        cut.production_note_path='Scene_Prompts/Scene1/Cut1.md';
        cut.visual_source={strategy:'no_start_frame'};
        cut.takes=[{id:'take-1',path:'Scenes/Scene1/Cut1/take.mp4',status:'selected'}];
        cut.selected_take='take-1';
        answer.state.assets=[...(answer.state.assets||[]),{id:'take-1',path:'Scenes/Scene1/Cut1/take.mp4',role:'video_take',status:'present',scene_id:scene.id,cut_id:cut.id}];
        scene.animatic={id:'anim-1',approved:true,shots:[{id:'anim-shot-1',cut_id:cut.id,duration_seconds:1,trim_in_seconds:0,trim_out_seconds:0,transition:'cut'}]};
        nodeFs.writeFileSync(${JSON.stringify(statePath)},JSON.stringify(answer.state,null,2));
        globalThis.__w118.fixtureReady=true;
      }
      return answer;
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w118.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await until('scene imported', () => ev(`document.querySelector('.workflow-scenes button')?.textContent?.includes('W1 native progress')`));
  await click(`[...document.querySelectorAll('.studio-finish-actions button')].find(e => e.textContent?.trim() === 'Prepare assembly')`);
  await until('stage-local working receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="working"] strong')?.textContent === ${JSON.stringify(expectedWorking)}`));
  await until('queued native job receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="queued"] strong')?.textContent === ${JSON.stringify(expectedQueued)}`));
  await until('queued global indicator', () => ev(`document.querySelector('[data-global-request-indicator]')?.textContent?.trim() === ${JSON.stringify(expectedQueuedIndicator)}`));
  assert.equal(await ev(`document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded')`), 'false');
  const queuedState = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  const queuedJob = queuedState.jobs.find(job => job.kind === 'assemble' && job.status === 'pending');
  assert.ok(queuedJob?.id, 'native prepare must persist one queued assembly job');
  await until('exact operation modal', () => ev(`document.querySelector('dialog[data-operation-approval][open]')?.getClientRects().length > 0`));
  await click(`[...document.querySelectorAll('dialog[data-operation-approval] button')].find(e => e.textContent?.trim() === 'Approve exact operation')`);
  await until('native ffmpeg dispatch started', () => fs.existsSync(ffmpegStarted));
  const runningState = await until('native job persisted running', () => {
    try {
      const persisted = JSON.parse(fs.readFileSync(statePath, 'utf8'));
      return persisted.jobs.find(job => job.id === queuedJob.id && job.status === 'running') ? persisted : null;
    } catch { return null; }
  });
  result.persisted = {
    queued: { id: queuedJob.id, kind: queuedJob.kind, status: queuedJob.status },
    running: runningState.jobs.find(job => job.id === queuedJob.id),
    ffmpegStarted: fs.readFileSync(ffmpegStarted, 'utf8').trim(),
  };

  // The production lifecycle must start its own jobs-only observation. The
  // fixture neither mutates a JobRecord nor calls a renderer refresh helper.
  await until('running global indicator', () => ev(`document.querySelector('[data-global-request-indicator]')?.textContent?.trim() === ${JSON.stringify(expectedRunningIndicator)}`));
  await until('running local receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="running"] strong')?.textContent === ${JSON.stringify(expectedRunning)}`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="running"]');
    const r=receipt.getBoundingClientRect(), style=getComputedStyle(receipt);
    return {
      activeSurface:[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Production Notes')?.textContent?.trim()).filter(Boolean),
      status:receipt.querySelector('strong')?.textContent ?? null,
      indicatorText:document.querySelector('[data-global-request-indicator]')?.textContent?.trim() ?? null,
      jobId:receipt.dataset.requestJobId ?? null,
      detail:receipt.querySelector('p')?.textContent ?? null,
      outcome:receipt.dataset.requestOutcome,
      visible:receipt.getClientRects().length>0 && r.width>0 && r.height>0 && style.visibility==='visible' && style.display!=='none' && Number(style.opacity)>0,
      inInitialViewport:r.top>=0 && r.left>=0 && r.bottom<=innerHeight && r.right<=innerWidth && scrollX===0 && scrollY===0,
      jobsExpanded:document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded') ?? null,
      jobsBodyVisible:Boolean(document.querySelector('.workflow-jobs-body')?.getClientRects().length),
      viewport:{width:innerWidth,height:innerHeight,scrollX,scrollY},
    };
  })()`);
  result.readback = readback;
  result.hostCalls = await main.evaluate('globalThis.__w118.calls');
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w118.blocked') };
  assert.deepEqual(readback.activeSurface, ['Production Notes']);
  assert.equal(readback.status, expectedRunning);
  assert.equal(readback.indicatorText, expectedRunningIndicator);
  assert.equal(readback.jobId, queuedJob.id);
  assert.equal(readback.detail, `Scene 1 · Running native job ${queuedJob.id}. Jobs owns authoritative progress.`);
  assert.equal(readback.outcome, 'running');
  assert.equal(readback.visible, true);
  assert.equal(readback.inInitialViewport, true);
  assert.equal(readback.jobsExpanded, 'false');
  assert.equal(readback.jobsBodyVisible, false);
  assert.equal(result.persisted.running.status, 'running');
  assert.equal(result.persisted.running.kind, 'assemble');
  assert.equal(result.hostCalls.filter(call => call.cmd === 'workflow_prepare_assemble').length, 1);
  assert.equal(result.hostCalls.filter(call => call.cmd === 'director_approve').length, 1);
  assert.ok(result.hostCalls.some(call => call.cmd === 'workflow_status' && call.payload?.jobsOnly === true));
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/queued-to-running-native-job.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: native assembly job ${queuedJob.id} persisted queued → running, then the automatic ordinary jobs-only read updated the visible global status to "${readback.indicatorText}" while Jobs stayed collapsed.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w118?.calls'), blocked: await main.evaluate('globalThis.__w118?.blocked'), persisted: fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, 'utf8')).jobs : null }; } catch {}
} finally {
  fs.writeFileSync(ffmpegRelease, 'release\n');
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await sleep(500); await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/queued-to-running-native-job.png`},null,2));
}
process.exitCode = result.exit;
