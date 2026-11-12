// Real Chromium local draft readback in disposable profiles; reuse the SB07 mounted fixture.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const root = path.resolve(__dirname, '..'), out = path.join(root, 'evidence/sb08');
const expected = { title: '  Notes only — 雨  ', notes: '  Before script.\n\nExact notes ☔\n', script: '' };
const latest = expected.notes + 'Retained after failed Save notes.\n' + '☔ '.repeat(100000);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(label, fn) { const end = Date.now() + 20000; while (Date.now() < end) { if (await fn()) return; await sleep(40); } throw Error('Timeout: ' + label); }
async function electron() {
  const { app, BrowserWindow } = require('electron');
  app.disableHardwareAcceleration(); app.setPath('userData', process.env.SB08_PROFILE);
  await app.whenReady();
  const win = new BrowserWindow({ width: 1250, height: 1100, webPreferences: { contextIsolation: true, nodeIntegration: false } });
  const wc = win.webContents, run = js => wc.executeJavaScript(js);
  const result = { phase: process.env.SB08_PHASE, externalRequests: [], exit: 1 };
  wc.session.webRequest.onBeforeRequest((details, callback) => {
    const url = new URL(details.url), allowed = url.origin === new URL(process.env.SB08_URL).origin || (url.protocol === 'ws:' && url.hostname === '127.0.0.1');
    if (!allowed) result.externalRequests.push(details.url); callback({ cancel: !allowed });
  });
  try {
    await win.loadURL(process.env.SB08_URL); await until('mounted', () => run('window.fixtureReady'));
    await run(`window.notesExpected=${JSON.stringify(expected)};window.latestNotes=${JSON.stringify(latest)};window.notesKey='studio.local-script-draft.v1:/disposable/sb07/A'`);
    const settled = () => until('notes action', () => run(`document.querySelector('form')?.getAttribute('aria-busy')==='false'`));
    if (result.phase === 'save') {
      await run(`clickLabel('Create script');for(const [name,value] of Object.entries(notesExpected))editDraft(name,value);clickLabel('Save notes')`); await settled();
      result.saved = await run(`({actual:readDraft(),stored:JSON.parse(localStorage.getItem(notesKey)),status:document.querySelector('[data-draft-recovery]').textContent,location:document.querySelector('form').textContent})`);
      assert.deepEqual(result.saved.actual, expected); assert.equal(result.saved.stored.notes, expected.notes); assert(result.saved.status.includes('Notes saved locally'));
      assert(result.saved.location.includes('not a project file'));
      await run(`clickLabel('Cancel');clickLabel('Create script')`); assert.deepEqual(await run('readDraft()'), expected);
      result.cancelRetained = true;
    } else {
      result.reopened = await run('readDraft()');
      assert.deepEqual(result.reopened, { ...expected, notes: result.phase === 'final' ? latest : expected.notes });
      await run(`clickLabel('Reload saved notes')`); await settled();
      assert.deepEqual(await run('readDraft()'), result.reopened); result.exactReload = true;
      if (result.phase === 'quota') {
        result.quota = await run(`(()=>{let n=0;try{for(;n<1000;n++)localStorage.setItem('sb08-fill-'+n,'x'.repeat(100000));}catch(e){return {name:e.name,filled:n};}throw Error('Quota not reached');})()`);
        assert.equal(result.quota.name, 'QuotaExceededError');
        await run(`window.priorNotes=localStorage.getItem(notesKey);editDraft('notes',latestNotes);clickLabel('Save notes')`); await settled();
        result.failure = await run(`({exact:readDraft().notes===latestNotes,priorUnchanged:localStorage.getItem(notesKey)===priorNotes,status:document.querySelector('[data-draft-recovery]').textContent})`);
        assert(result.failure.exact && result.failure.priorUnchanged); assert(result.failure.status.includes('only in memory'));
        await run(`clickLabel('Reload saved notes')`); await settled(); assert.equal(await run('readDraft().notes'), latest);
        await run(`clickLabel('Cancel');clickLabel('Create script')`); assert.equal(await run('readDraft().notes'), latest);
        await run(`for(let n=0;n<1000;n++)localStorage.removeItem('sb08-fill-'+n);clickLabel('Save notes')`); await settled();
        assert.equal(await run(`JSON.parse(localStorage.getItem(notesKey)).notes`), latest); result.retryExact = true;
      }
    }
    await run(`document.querySelector('[data-draft-recovery]').scrollIntoView({block:'center'});new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)))`);
    await sleep(200); // Let Chromium's compositor present the mounted form before capture.
    fs.writeFileSync(path.join(out, 'notes-' + result.phase + '.png'), (await wc.capturePage()).toPNG());
    result.calls = await run('window.calls'); result.providerAttempts = await run('window.providerAttempts');
    assert(result.calls.every(call => call.command === 'workflow_import')); assert.equal(result.providerAttempts, 0); assert.equal(result.externalRequests.length, 0);
    result.exit = 0;
  } catch (error) { result.error = error.stack; }
  fs.writeFileSync(path.join(out, 'electron-' + result.phase + '.json'), JSON.stringify(result, null, 2) + '\n');
  // Avoid echoing the long exact notes payload into the raw log.
  console.log(JSON.stringify({ phase: result.phase, exit: result.exit, exactReload: result.exactReload, failure: result.failure, error: result.error }));
  win.destroy(); if (result.exit) app.exit(result.exit); else app.quit();
}
async function node() {
  const { createServer } = await import('vite');
  const server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  const profile = fs.mkdtempSync(path.join(process.env.TMPDIR || '/runtime', 'sb08-profile-'));
  fs.writeFileSync(path.join(out, 'profile-path.txt'), profile + '\n');
  try {
    await server.listen(); const url = `http://127.0.0.1:${server.httpServer.address().port}/tests/fixtures/sb07-recovery.html`;
    for (const phase of ['save', 'quota', 'final']) {
      const child = spawn(require('electron'), ['--no-sandbox', __filename], { cwd: root, stdio: 'inherit', env: { ...process.env, SB08_URL: url, SB08_PROFILE: profile, SB08_PHASE: phase } });
      const code = await new Promise((resolve, reject) => { child.on('error', reject); child.on('exit', code => resolve(code ?? 1)); }); assert.equal(code, 0, phase);
    }
  } finally { await server.close(); }
}
(process.versions.electron ? electron() : node()).catch(error => { console.error(error); process.exit(1); });
