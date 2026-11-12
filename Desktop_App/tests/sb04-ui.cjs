// Isolated real Chromium playback of the accepted SB03 local master. No Studio host/provider.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const out = path.join(root, 'evidence/sb04');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(label, fn, timeout = 15000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { const value = await fn(); if (value) return value; await sleep(70); }
  throw new Error(`Timed out: ${label}`);
}
async function runElectron() {
  const {app, BrowserWindow} = require('electron');
  app.disableHardwareAcceleration();
  app.setPath('userData', process.env.SB04_USER_DATA);
  await app.whenReady();
  const win = new BrowserWindow({width:1180,height:1050,webPreferences:{nodeIntegration:false,contextIsolation:true,backgroundThrottling:false}});
  const wc = win.webContents;
  const evaluate = js => wc.executeJavaScript(js);
  const result = {fixture:'three native selected takes / SB03 master', providerCalls:0, nativeHostCalls:0};
  try {
    await win.loadURL(process.env.SB04_URL);
    await until('fixture mount',()=>evaluate('window.fixtureReady'));
    await until('actual decoded video metadata',()=>evaluate(`document.querySelector('video')?.readyState >= 2`));
    result.initial = await evaluate(`(()=>{const v=document.querySelector('video');return {duration:v.duration,src:v.currentSrc,controls:v.controls,order:[...document.querySelectorAll('[aria-label="Saved rendered sequence order"] li')].map(r=>r.dataset.takeId),missingAnimatic:document.body.textContent.includes('Scene 3: no sketch animatic'),planningHeading:document.querySelector('.workflow-animatic h2').textContent}})()`);
    assert.deepEqual(result.initial.order,['asset_27','asset_34','asset_45']);
    assert(result.initial.controls && result.initial.duration>22 && result.initial.duration<23);
    assert(result.initial.missingAnimatic);
    assert.equal(result.initial.planningHeading,'Sketch animatic · 0.0s');
    async function click(selector) {
      const point = await evaluate(`(()=>{const e=${selector};e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();return {x:Math.round(r.x+r.width/2),y:Math.round(r.y+r.height/2)}})()`);
      wc.sendInputEvent({type:'mouseDown',button:'left',clickCount:1,...point});
      wc.sendInputEvent({type:'mouseUp',button:'left',clickCount:1,...point});
    }
    await click(`document.querySelector('[aria-label="Rendered rough-cut review"] button')`);
    await until('actual playback advancing',()=>evaluate(`!document.querySelector('video').paused && document.querySelector('video').currentTime>0.5`));
    result.playing = await evaluate(`({time:document.querySelector('video').currentTime,paused:document.querySelector('video').paused,label:document.querySelector('[aria-label="Rendered rough-cut review"] button').textContent})`);
    await click(`document.querySelector('[aria-label="Rendered rough-cut review"] button')`);
    await until('pause',()=>evaluate(`document.querySelector('video').paused`));
    // Click the visible range control at positions inside each saved rendered take.
    result.seeks=[];
    for (const target of [5,10,15]) {
      const point=await evaluate(`(()=>{const e=document.querySelector('input[aria-label="Seek rendered rough cut"]');e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect();return {x:Math.round(r.x+8+(r.width-16)*${target}/Number(e.max)),y:Math.round(r.y+r.height/2)}})()`);
      wc.sendInputEvent({type:'mouseDown',button:'left',clickCount:1,...point});
      wc.sendInputEvent({type:'mouseUp',button:'left',clickCount:1,...point});
      await until('visible sequence seek',()=>evaluate(`!document.querySelector('video').seeking && Math.abs(document.querySelector('video').currentTime-${target})<0.4`));
      result.seeks.push(await evaluate(`(()=>{const v=document.querySelector('video'); const c=document.createElement('canvas');c.width=32;c.height=32;const ctx=c.getContext('2d');ctx.drawImage(v,0,0,32,32);return {time:v.currentTime,paused:v.paused,decodedFrames:v.getVideoPlaybackQuality().totalVideoFrames,frame:[...ctx.getImageData(15,15,1,1).data]}})()`));
    }
    assert(result.seeks[1].time>7.64 && result.seeks[1].time<12.29,'second native seek must reach take 2');
    assert(result.seeks[2].time>12.29,'third native seek must reach take 3');
    assert.notDeepEqual(result.seeks[1].frame,result.seeks[2].frame,'different clips decode after seeking');
    result.stateUnchanged=await evaluate(`JSON.stringify(window.fixtureState)===window.fixtureBefore && window.fixtureIntents.length===0`);
    assert(result.stateUnchanged,'play/seek must not save, approve or alter timing');
    await evaluate(`document.querySelector('[aria-label="Rendered rough-cut review"]').scrollIntoView({block:'start',behavior:'instant'})`);
    fs.writeFileSync(path.join(out,'rendered-review.png'),(await wc.capturePage()).toPNG());
    result.exit=0;
  } catch(error) { result.exit=1; result.error=error.stack; }
  fs.writeFileSync(path.join(out,'ui-result.json'),JSON.stringify(result,null,2)+'\n');
  console.log(JSON.stringify(result,null,2));
  win.destroy(); app.exit(result.exit);
}
async function runNode() {
  const {createServer}=await import('vite');
  const server=await createServer({root,server:{host:'127.0.0.1',port:0,strictPort:false}});
  const userData=fs.mkdtempSync(path.join(process.env.TMPDIR || '/runtime','sb04-electron-'));
  try {
    await server.listen();
    const port=server.httpServer.address().port;
    const child=spawn(require('electron'),['--no-sandbox',__filename],{cwd:root,stdio:'inherit',env:{...process.env,SB04_USER_DATA:userData,SB04_URL:`http://127.0.0.1:${port}/tests/fixtures/sb04-review.html`}});
    process.exitCode=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',code=>resolve(code??1));});
  } finally { await server.close(); fs.rmSync(userData,{recursive:true,force:true}); }
}
(process.versions.electron ? runElectron() : runNode()).catch(error=>{console.error(error);process.exit(1)});
