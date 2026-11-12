import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { CDP, until } from './helpers/ui.mjs';
import { appRoot as app, prepareOutput } from './helpers/runtime.mjs';

const out = prepareOutput('request-visibility-media-saved');
const root = `${out}/disposable-project`;
const shimDir = `${out}/bin`;
const ffmpegStarted = `${out}/ffmpeg-started`;
const ffmpegRelease = `${out}/ffmpeg-release`;
const statePath = `${root}/Preproduction/workflow_state.json`;
const command = [process.execPath, ...process.execArgv, ...process.argv.slice(1)].map(arg => JSON.stringify(arg)).join(' ');
const expectedWorking = 'Submitted · Working — Preparing assembly…';
const expectedQueued = 'Submitted · Queued job — Preparing assembly…';
const expectedRunning = 'Submitted · Running job — Preparing assembly…';
const expectedSaved = 'Submitted · Output saved — Preparing assembly…';
for (const dir of ['Preproduction', 'Scripts', 'Scene_Prompts/Scene1', 'Scenes/Scene1/Cut1']) fs.mkdirSync(`${root}/${dir}`, { recursive: true });
fs.mkdirSync(shimDir, { recursive: true });
fs.writeFileSync(`${root}/Scripts/script.md`, '# Disposable story\n\n## Scene 1 — W1 native saved output\nAlex: This is a bounded offline assembly fixture.\n');
fs.writeFileSync(`${root}/Scene_Prompts/Scene1/Cut1.md`, 'source_scene: 1\ncut: 1\ntitle: Offline shot\nscene_type: b_roll\ncontinuity:\n  group: test\n  relation_to_previous: new_setup\nediting:\n  transition_in: cut\n  transition_out: cut\nspeech:\n  mode: none\nprompt: bounded offline fixture\n');
const realFfmpeg = spawnSync('sh', ['-lc', 'command -v ffmpeg'], { encoding: 'utf8' }).stdout.trim();
assert.ok(realFfmpeg, 'ffmpeg must be available for the offline fixture');
const generated = spawnSync(realFfmpeg, ['-v', 'error', '-f', 'lavfi', '-i', 'color=c=blue:s=160x90:r=24:d=1', '-c:v', 'libx264', '-threads', '1', '-pix_fmt', 'yuv420p', `${root}/Scenes/Scene1/Cut1/take.mp4`]);
assert.equal(generated.status, 0, generated.stderr?.toString());
fs.writeFileSync(`${shimDir}/ffmpeg`, `#!/bin/sh\nprintf '%s\\n' "$*" > ${JSON.stringify(ffmpegStarted)}\nwhile [ ! -f ${JSON.stringify(ffmpegRelease)} ]; do sleep 0.1; done\nexec ${JSON.stringify(realFfmpeg)} "$@"\n`, { mode: 0o755 });

const result = {
  check: 'request_visibility_media_saved',
  expected: 'Only the same actual native job terminally completed with a newly saved non-empty output says output was saved.',
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
    globalThis.__w121={calls:[],blocked:[],fixtureReady:false};
    const original=ipcMain._invokeHandlers.get('host:invoke');
    ipcMain.removeHandler('host:invoke');
    ipcMain.handle('host:invoke',async(...args)=>{
      const cmd=args[1], payload=args[2];
      if(['workflow_prepare_assemble','director_approve','workflow_status'].includes(cmd)) globalThis.__w121.calls.push({cmd,payload});
      const answer=await original(...args);
      if(cmd==='workflow_import' && !globalThis.__w121.fixtureReady){
        const scene=answer.state.scenes[0], cut=scene?.cuts?.[0];
        if(!scene || !cut) throw Error('Native import did not create the disposable scene/cut');
        cut.production_note_path='Scene_Prompts/Scene1/Cut1.md';
        cut.visual_source={strategy:'no_start_frame'};
        cut.takes=[{id:'take-1',path:'Scenes/Scene1/Cut1/take.mp4',status:'selected'}];
        cut.selected_take='take-1';
        answer.state.assets=[...(answer.state.assets||[]),{id:'take-1',path:'Scenes/Scene1/Cut1/take.mp4',role:'video_take',status:'present',scene_id:scene.id,cut_id:cut.id}];
        scene.animatic={id:'anim-1',approved:true,shots:[{id:'anim-shot-1',cut_id:cut.id,duration_seconds:1,trim_in_seconds:0,trim_out_seconds:0,transition:'cut'}]};
        nodeFs.writeFileSync(${JSON.stringify(statePath)},JSON.stringify(answer.state,null,2));
        globalThis.__w121.fixtureReady=true;
      }
      return answer;
    });
    session.defaultSession.webRequest.onBeforeRequest((request,callback)=>{
      const remote=/^https?:/.test(request.url)&&!request.url.startsWith('http://127.0.0.1:1420/');
      if(remote)globalThis.__w121.blocked.push(request.url);
      callback({cancel:remote});
    });
  })()`);

  await click(`document.querySelector('#home-open-project')`);
  await until('project applied', () => ev(`document.querySelector('#project-name')?.textContent === ${JSON.stringify(path.basename(root))}`));
  await until('scene imported', () => ev(`document.querySelector('.workflow-scenes button')?.textContent?.includes('W1 native saved output')`));
  await click(`[...document.querySelectorAll('.studio-finish-actions button')].find(e => e.textContent?.trim() === 'Prepare assembly')`);
  await until('working receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="working"] strong')?.textContent === ${JSON.stringify(expectedWorking)}`));
  await until('queued receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="queued"] strong')?.textContent === ${JSON.stringify(expectedQueued)}`));
  const queuedState = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  const queuedJob = queuedState.jobs.find(job => job.kind === 'assemble' && job.status === 'pending');
  assert.ok(queuedJob?.id && queuedJob?.output_path, 'native prepare must persist one queued assembly job and reserved output path');
  const outputFile = `${root}/${queuedJob.output_path}`;
  assert.equal(fs.existsSync(outputFile), false, 'queued output must not already exist');
  assert.equal(/output saved/i.test(await ev(`document.querySelector('.workflow-main')?.innerText ?? ''`)), false);

  await until('exact operation modal', () => ev(`document.querySelector('dialog[data-operation-approval][open]')?.getClientRects().length > 0`));
  await click(`[...document.querySelectorAll('dialog[data-operation-approval] button')].find(e => e.textContent?.trim() === 'Approve exact operation')`);
  await until('native ffmpeg dispatch started', () => fs.existsSync(ffmpegStarted));
  const runningState = await until('native job persisted running', () => {
    try {
      const persisted = JSON.parse(fs.readFileSync(statePath, 'utf8'));
      return persisted.jobs.find(job => job.id === queuedJob.id && job.status === 'running') ? persisted : null;
    } catch { return null; }
  });
  const runningJob = runningState.jobs.find(job => job.id === queuedJob.id);
  assert.equal(runningJob.output_path, queuedJob.output_path);
  assert.equal(fs.existsSync(outputFile), false, 'running output must not yet exist while the first native ffmpeg command is gated');
  await until('running receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="running"] strong')?.textContent === ${JSON.stringify(expectedRunning)}`));
  assert.equal(/output saved/i.test(await ev(`document.querySelector('.workflow-main')?.innerText ?? ''`)), false);

  fs.writeFileSync(ffmpegRelease, 'release\n');
  const completedState = await until('same native job persisted completed', () => {
    try {
      const persisted = JSON.parse(fs.readFileSync(statePath, 'utf8'));
      const job = persisted.jobs.find(candidate => candidate.id === queuedJob.id);
      return job?.status === 'completed' && job.output_path === queuedJob.output_path ? persisted : null;
    } catch { return null; }
  });
  const completedJob = completedState.jobs.find(job => job.id === queuedJob.id);
  assert.equal(fs.existsSync(outputFile), true, 'completed native output must exist');
  const outputSize = fs.statSync(outputFile).size;
  assert.ok(outputSize > 0, 'completed native output must be non-empty');
  await until('saved output receipt', () => ev(`document.querySelector('.workflow-main > .workflow-request-receipts [data-request-outcome="saved"] strong')?.textContent === ${JSON.stringify(expectedSaved)}`));

  const readback = await ev(`(() => {
    const workspace=document.querySelector('.workflow-main');
    const receipt=workspace.querySelector(':scope > .workflow-request-receipts [data-request-outcome="saved"]');
    const r=receipt.getBoundingClientRect(), style=getComputedStyle(receipt);
    return {
      activeSurface:[...workspace.querySelectorAll('.studio-surface-nav button[aria-current="page"]')].map(e=>[...e.children].find(c=>c.textContent?.trim()==='Production Notes')?.textContent?.trim()).filter(Boolean),
      status:receipt.querySelector('strong')?.textContent ?? null,
      indicatorText:document.querySelector('[data-global-request-indicator]')?.textContent?.trim() ?? null,
      jobId:receipt.dataset.requestJobId ?? null,
      detail:receipt.querySelector('p')?.textContent ?? null,
      outcome:receipt.dataset.requestOutcome,
      role:receipt.getAttribute('role'),
      visible:receipt.getClientRects().length>0 && r.width>0 && r.height>0 && style.visibility==='visible' && style.display!=='none' && Number(style.opacity)>0,
      inInitialViewport:r.top>=0 && r.left>=0 && r.bottom<=innerHeight && r.right<=innerWidth && scrollX===0 && scrollY===0,
      jobsExpanded:document.querySelector('.studio-jobs-toggle')?.getAttribute('aria-expanded') ?? null,
      jobsBodyVisible:Boolean(document.querySelector('.workflow-jobs-body')?.getClientRects().length),
      viewport:{width:innerWidth,height:innerHeight,scrollX,scrollY},
    };
  })()`);
  result.readback = { ...readback, outputPath: queuedJob.output_path, outputSize };
  result.hostCalls = await main.evaluate('globalThis.__w121.calls');
  result.persisted = {
    queued: { id: queuedJob.id, status: queuedJob.status, outputPath: queuedJob.output_path, outputExisted: false },
    running: { id: runningJob.id, status: runningJob.status, outputPath: runningJob.output_path, outputExisted: false },
    completed: { id: completedJob.id, status: completedJob.status, outputPath: completedJob.output_path, outputExisted: true, outputSize },
    ffmpegStarted: fs.readFileSync(ffmpegStarted, 'utf8').trim(),
  };
  result.network = { blockedRemoteRequests: await main.evaluate('globalThis.__w121.blocked') };
  assert.deepEqual(readback.activeSurface, ['Production Notes']);
  assert.equal(readback.status, expectedSaved);
  assert.equal(readback.indicatorText, null);
  assert.equal(readback.jobId, queuedJob.id);
  assert.equal(readback.detail, `Scene 1 · Output saved to ${queuedJob.output_path}.`);
  assert.equal(readback.outcome, 'saved');
  assert.equal(readback.role, 'status');
  assert.equal(readback.visible, true);
  assert.equal(readback.inInitialViewport, true);
  assert.equal(readback.jobsExpanded, 'false');
  assert.equal(readback.jobsBodyVisible, false);
  assert.equal(runningJob.id, queuedJob.id);
  assert.equal(completedJob.id, queuedJob.id);
  assert.equal(result.hostCalls.filter(call => call.cmd === 'workflow_prepare_assemble').length, 1);
  assert.equal(result.hostCalls.filter(call => call.cmd === 'director_approve').length, 1);
  assert.ok(result.hostCalls.some(call => call.cmd === 'workflow_status' && call.payload?.jobsOnly === true));
  assert.deepEqual(result.network.blockedRemoteRequests, []);
  const shot = await cdp.call('Page.captureScreenshot', { format: 'png' });
  fs.writeFileSync(`${out}/native-saved-output-terminal.png`, Buffer.from(shot.data, 'base64'));
  result.actual = `PASS: native assembly job ${queuedJob.id} stayed the exact queued → running → completed identity; only completed state showed "${readback.status}" after ${queuedJob.output_path} existed with ${outputSize} bytes.`;
  result.exit = 0;
} catch (error) {
  result.error = error.stack;
  try { result.diagnostic = { body: await ev('document.body.innerText'), calls: await main.evaluate('globalThis.__w121?.calls'), blocked: await main.evaluate('globalThis.__w121?.blocked'), persisted: fs.existsSync(statePath) ? JSON.parse(fs.readFileSync(statePath, 'utf8')).jobs : null }; } catch {}
} finally {
  if (!fs.existsSync(ffmpegRelease)) fs.writeFileSync(ffmpegRelease, 'release\n');
  fs.writeFileSync(`${out}/result.json`, JSON.stringify(result, null, 2));
  if (main) { try { await sleep(500); await main.evaluate(`process.getBuiltinModule('module').createRequire(process.cwd()+'/electron/main.mjs')('electron').app.exit(0)`); } catch {} }
  cdp?.close(); main?.close();
  if (electron?.exitCode === null) electron.kill('SIGTERM');
  if (vite.exitCode === null) vite.kill('SIGTERM');
  console.log(JSON.stringify({exit:result.exit,actual:result.actual,error:result.error,result:`${out}/result.json`,screenshot:`${out}/native-saved-output-terminal.png`},null,2));
}
process.exitCode = result.exit;
