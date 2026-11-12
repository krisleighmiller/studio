#!/usr/bin/env node
/** Minimal actual Electron -> renderer UI -> Rust host -> loopback Director tracer.
 * Linux/Secret Service/X11. No application dispatch, approval or bridge replacement.
 * Run with --out ABSOLUTE_NEW_DIRECTORY; --inject-failure tests outer-finally cleanup.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import net from 'node:net';
import crypto from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function deadline(promise, label) {
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} timeout`)), 5000); })]); }
  finally { clearTimeout(timer); }
}
export async function until(label, fn, timeout = 20000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const value = await fn();
    if (value) return value;
    await sleep(80);
  }
  throw new Error(`Timed out: ${label}`);
}
export class CDP {
  constructor(socket) {
    this.socket = socket; this.nextId = 0; this.pending = new Map(); this.listeners = [];
    socket.addEventListener('message', event => {
      try {
        const data = JSON.parse(event.data);
        if (data.id) {
          const pending = this.pending.get(data.id);
          if (!pending) return;
          this.pending.delete(data.id); clearTimeout(pending.timer);
          if (data.error) pending.reject(new Error(JSON.stringify(data.error)));
          else pending.resolve(data.result);
        } else for (const listener of this.listeners) {
          Promise.resolve(listener(data)).catch(error => this.fail(error));
        }
      } catch (error) { this.fail(error); }
    });
    socket.addEventListener('close', () => {
      for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(new Error('CDP closed')); }
      this.pending.clear();
    });
  }
  static async open(url) {
    const socket = new WebSocket(url);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => { socket.close(); reject(new Error('WebSocket open timeout')); }, 5000);
      socket.addEventListener('open', () => { clearTimeout(timer); resolve(); }, { once: true });
      socket.addEventListener('error', () => { clearTimeout(timer); reject(new Error('WebSocket open error')); }, { once: true });
    });
    return new CDP(socket);
  }
  fail(error) {
    this.failure ??= error;
    for (const p of this.pending.values()) { clearTimeout(p.timer); p.reject(error); }
    this.pending.clear();
  }
  call(method, params = {}) {
    if (this.failure) return Promise.reject(this.failure);
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 10000);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
  async evaluate(expression) {
    return checkedValue(await this.call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }));
  }
  close() { this.socket.close(); }
}
export function connectionErrors(sockets) {
  return sockets.filter(socket => socket.failure).map(socket => socket.failure.stack ?? String(socket.failure));
}
export function checkedValue(result) {
  if (result.exceptionDetails) throw new Error(`CDP evaluation exception: ${JSON.stringify(result.exceptionDetails)}`);
  return result.result?.value;
}
async function jsonAt(port, endpoint) {
  try {
    const response = await fetch(`http://127.0.0.1:${port}${endpoint}`, { signal: AbortSignal.timeout(800) });
    if (!response.ok) return null;
    return await response.json();
  } catch { return null; }
}
async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once('error', reject); server.listen(0, '127.0.0.1', resolve);
  });
  return server.address().port;
}
async function freePort() {
  const server = net.createServer(); const port = await listen(server);
  await new Promise(resolve => server.close(resolve)); return port;
}
async function accepting(port) {
  return new Promise(resolve => {
    const socket = net.connect({ host: '127.0.0.1', port });
    socket.setTimeout(500);
    socket.once('connect', () => { socket.destroy(); resolve(true); });
    socket.once('error', () => { socket.destroy(); resolve(false); });
    socket.once('timeout', () => { socket.destroy(); resolve(false); });
  });
}
function processInfo(pid) {
  try {
    const base = `/proc/${pid}`; const stat = fs.readFileSync(`${base}/stat`, 'utf8');
    const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
    let exe = null; let argv = [];
    try { exe = fs.readlinkSync(`${base}/exe`); argv = fs.readFileSync(`${base}/cmdline`, 'utf8').split('\0').filter(Boolean); } catch { /* Preserve zombie/stat identity until genuinely reaped. */ }
    return { pid, state: fields[0], ppid: +fields[1], pgrp: +fields[2], sid: +fields[3], birth: fields[19], exe, argv };
  } catch { return null; }
}
function processes() {
  return fs.readdirSync('/proc').filter(p => /^\d+$/.test(p)).map(p => processInfo(+p)).filter(Boolean);
}
function identity(file) {
  const stat = fs.statSync(file);
  return { path: fs.realpathSync(file), sha256: crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex'),
    size: stat.size, inode: stat.ino };
}
function displayEnv() {
  if (process.env.DISPLAY && process.env.XAUTHORITY) return {};
  for (const proc of processes()) if (path.basename(proc.exe ?? '') === 'Xwayland') {
    const allowed = new Set(['DISPLAY', 'XAUTHORITY', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS']);
    return Object.fromEntries(fs.readFileSync(`/proc/${proc.pid}/environ`, 'utf8').split('\0')
      .filter(line => allowed.has(line.split('=')[0])).map(line => [line.slice(0, line.indexOf('=')), line.slice(line.indexOf('=') + 1)]));
  }
  throw new Error('No accessible X11 display; supply DISPLAY and XAUTHORITY');
}

export function admitGroups(all, groups, owned) {
  const current = new Map(all.map(info => [info.pid, info]));
  for (const group of groups) {
    if (group.retired) continue;
    const leader = current.get(group.pid);
    if (leader && leader.birth !== group.birth) { group.retired = true; continue; }
    const members = all.filter(info => info.pgrp === group.pid && info.sid === group.pid);
    const trusted = leader?.birth === group.birth || members.some(info => owned.get(info.pid)?.birth === info.birth);
    if (!members.length || !trusted) { group.retired = true; continue; }
    for (const info of members) if (BigInt(info.birth) >= BigInt(group.birth)) {
      const prior = owned.get(info.pid);
      if (!prior) owned.set(info.pid, info);
      // A reused member PID is not admitted merely because its old PID was owned.
    }
  }
}
export async function attemptAll(actions) {
  const errors = [];
  for (const [label, action] of actions) {
    try { await action(); } catch (error) { errors.push({ label, error: error.stack ?? String(error) }); }
  }
  return errors;
}
export async function plannerBody(request) {
  let body = '';
  for await (const chunk of request) { body += chunk; if (body.length > 2_000_000) throw new Error('Planner body limit'); }
  return body ? JSON.parse(body) : null;
}

export async function run(out, injectFailure = false, scenario = null) {
  assert(path.isAbsolute(out), '--out must be absolute');
  fs.mkdirSync(out, { recursive: false, mode: 0o700 });
  const save = (name, value) => fs.writeFileSync(path.join(out, name), JSON.stringify(value, null, 2));
  const apiId = `t01-ui-${crypto.randomUUID().replaceAll('-', '')}`;
  const ollamaId = `t01-ui-${crypto.randomUUID().replaceAll('-', '')}`;
  const report = { command: process.argv, root, injectFailure, apiId, ollamaId, runner: processInfo(process.pid), steps: [], requests: [],
    observed: {}, errors: [], owned: [], ports: {}, cleanup: {} };
  const children = []; const groups = []; const servers = []; const serverSockets = new Set(); const sockets = []; const streams = []; const owned = new Map();
  let asyncFailure;
  const failAsync = error => { asyncFailure ??= error; };
  const wait = (label, fn, timeout) => until(label, () => { if (asyncFailure) throw asyncFailure; return fn(); }, timeout);
  const env = { ...process.env, ...displayEnv(), TMPDIR: path.join(out, 'tmp') };
  const native = (operation, id) => JSON.parse(execFileSync('/usr/bin/python3', [path.join(root, 'tests/ui-tracer-native.py'), operation, ...(id ? [id] : [])],
    { env, timeout: 15000, encoding: 'utf8' }));
  function scanOwned() { admitGroups(processes(), groups, owned); }
  function trackServer(server) {
    servers.push(server); server.on('error', failAsync);
    server.on('connection', socket => { serverSockets.add(socket); socket.on('error', failAsync); socket.on('close', () => serverSockets.delete(socket)); });
  }
  function start(name, binary, args, childEnv) {
    const log = fs.createWriteStream(path.join(out, `${name}.log`)); streams.push(log);
    const child = spawn(binary, args, { cwd: root, env: childEnv, detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
    children.push(child); child.stdout.pipe(log, { end: false }); child.stderr.pipe(log, { end: false });
    for (const emitter of [child, child.stdout, child.stderr, log]) emitter.on('error', failAsync);
    child.on('exit', (code, signal) => report.steps.push({ processExit: name, code, signal }));
    const info = processInfo(child.pid);
    assert(info, `Capture newborn owned ${name}`); owned.set(info.pid, info); groups.push({ pid: info.pid, birth: info.birth });
    assert.equal(info.pgrp, info.pid); assert.equal(info.sid, info.pid);
    return child;
  }
  let main; let page; let ownershipTimer; let failure;
  try {
    const configured = scenario ? await scenario.setup({ out, env, report, trackServer }) : null;
    fs.mkdirSync(env.TMPDIR, { recursive: true });
    const userData = configured?.userData ?? path.join(out, 'userData');
    const project = configured?.project ?? path.join(out, 'project');
    fs.mkdirSync(path.join(userData, 'config'), { recursive: true });
    if (!configured) {
      fs.mkdirSync(project);
      fs.writeFileSync(path.join(project, 'README.md'), '# T01 isolated project\nNo media or paid providers.\n');
    }
    const hostBin = configured?.hostBin ?? path.join(root, 'host/target/debug/workbench-host');
    report.hostBinaryIdentity = identity(hostBin);
    const electronBin = path.join(root, 'node_modules/electron/dist/electron');
    report.identities = Object.fromEntries(['tests/ui-tracer.mjs', 'tests/ui-tracer-native.py', 'electron/main.mjs', 'electron/sidecar.mjs',
      'electron/preload.cjs', 'src/main.ts', 'package.json', 'host/target/debug/workbench-host', 'node_modules/electron/dist/electron']
      .map(file => [file, identity(path.join(root, file))]));
    report.keyringBefore = [native('check', apiId), native('check', ollamaId)];
    assert(report.keyringBefore.every(item => item.remaining === 0));
    const marker = `T01_TERMINAL_${crypto.randomUUID()}`;
    const planner = http.createServer((request, response) => {
      (async () => {
      const record = { method: request.method, path: request.url, remoteAddress: request.socket.remoteAddress, body: await plannerBody(request) };
      const connections = execFileSync('ss', ['-H', '-tnp'], { encoding: 'utf8', timeout: 5000 }).trim().split('\n');
      record.hostSocket = connections.filter(line => line.split(/\s+/)[3]?.endsWith(`:${request.socket.remotePort}`) && line.split(/\s+/)[4]?.endsWith(`:${report.ports.planner}`));
      assert.equal(record.hostSocket.length, 1); assert(record.hostSocket[0].includes(`pid=${report.host.pid},`), 'Planner connection must belong to actual Rust host');
      report.requests.push(record); save('planner-requests.json', report.requests);
      if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
        response.writeHead(400); response.end('Unexpected route'); return;
      }
      response.writeHead(200, { 'Content-Type': 'application/json' });
      const action = scenario ? await scenario.plan({ record, report, save }) : { action: 'done', message: marker };
      response.end(JSON.stringify({ choices: [{ message: { content: `<director_action>${JSON.stringify(action)}</director_action>` } }] }));
      })().catch(error => { failAsync(error); response.destroy(); });
    });
    trackServer(planner); report.ports.planner = await listen(planner);
    const denied = report.deniedNetwork = [];
    const deny = http.createServer((req, res) => { denied.push({ method: req.method, url: req.url }); res.writeHead(403); res.end(); });
    deny.on('connect', (req, socket) => { denied.push({ method: req.method, url: req.url }); socket.end('HTTP/1.1 403 Forbidden\r\n\r\n'); });
    trackServer(deny); report.ports.denyProxy = await listen(deny);
    const baseUrl = `http://127.0.0.1:${report.ports.planner}`;
    const api = (id, kind, enabled) => ({ id, name: 'T01 loopback only', kind, baseUrl, apiKey: enabled ? 't01-synthetic-fixture-only' : '', secretKey: '', enabled,
      catalog: enabled ? [{ id: 't01-planner', displayName: 'T01 Planner', type: 'Text' }] : [], catalogFetchedAt: new Date().toISOString(), pinnedLlmModelIds: enabled ? ['t01-planner'] : [] });
    const settingsFile = path.join(userData, 'config/settings.json');
    const fixtureSettings = { apis: [api(apiId, 'atlas-cloud', true), api(ollamaId, 'ollama', false)],
      capabilityDefaults: {}, adultContentEnabled: false, adultCapabilityDefaults: {}, selectedChatModel: null };
    if (configured?.configureSettings) configured.configureSettings(fixtureSettings);
    fs.writeFileSync(settingsFile, JSON.stringify(fixtureSettings));
    fs.writeFileSync(path.join(userData, 'recent-projects.json'), JSON.stringify([project]));
    report.fixture = { userData, settingsFile, project, baseUrl, marker };
    // Both reqwest and Chromium proxy all non-loopback HTTP(S) to a local deny endpoint.
    const proxy = `http://127.0.0.1:${report.ports.denyProxy}`;
    Object.assign(env, { WORKBENCH_DEV: '1', WORKBENCH_HOST_BIN: hostBin, XDG_CONFIG_HOME: configured?.xdgConfig ?? path.join(out, 'xdg'),
      HTTP_PROXY: proxy, HTTPS_PROXY: proxy, ALL_PROXY: proxy, http_proxy: proxy, https_proxy: proxy, all_proxy: proxy,
      NO_PROXY: '127.0.0.1,localhost,::1', no_proxy: '127.0.0.1,localhost,::1' });
    delete env.ELECTRON_RUN_AS_NODE; delete env.NODE_OPTIONS;
    report.environment = Object.fromEntries(['DISPLAY', 'XAUTHORITY', 'TMPDIR', 'WORKBENCH_DEV', 'WORKBENCH_HOST_BIN', 'XDG_CONFIG_HOME', 'HTTP_PROXY', 'NO_PROXY'].map(k => [k, env[k]]));
    // Preload accepts exactly this dev origin: fail on occupation, never adopt an existing server.
    assert.equal(await accepting(1420), false, 'Vite port already owned by another process');
    report.ports.vite = 1420;
    report.ports.mainInspector = await freePort(); report.ports.rendererCDP = await freePort();
    assert.equal(new Set(Object.values(report.ports)).size, Object.keys(report.ports).length);
    report.beforeLaunch = { processes: processes().filter(p => /electron|workbench-host|vite/.test(p.exe)),
      portsFree: await Promise.all([1420, report.ports.mainInspector, report.ports.rendererCDP].map(async port => ({ port, accepting: await accepting(port) }))) };
    ownershipTimer = setInterval(() => { try { scanOwned(); } catch (error) { failAsync(error); } }, 80);
    start('vite', process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', '1420', '--strictPort'], env);
    await wait('owned Vite', async () => { try { return (await fetch('http://127.0.0.1:1420', { signal: AbortSignal.timeout(500) })).ok; } catch { return false; } });
    const electron = start('electron', electronBin, ['.', '--no-sandbox', '--ozone-platform=x11', `--inspect-brk=127.0.0.1:${report.ports.mainInspector}`,
      `--remote-debugging-port=${report.ports.rendererCDP}`, '--remote-debugging-address=127.0.0.1', `--user-data-dir=${userData}`,
      `--proxy-server=${proxy}`, `--log-net-log=${path.join(out, 'chromium-netlog.json')}`, '--proxy-bypass-list=127.0.0.1;localhost;[::1]', '--disable-background-networking', '--disable-component-update', '--disable-sync', '--no-first-run'], env);
    const mainTargets = await wait('Electron main /json/list', async () => await jsonAt(report.ports.mainInspector, '/json/list'));
    assert.equal(mainTargets.length, 1); report.mainTarget = mainTargets[0];
    main = await CDP.open(report.mainTarget.webSocketDebuggerUrl); sockets.push(main);
    await main.call('Runtime.enable');
    await main.call('Debugger.enable');
    const mainUrl = pathToFileURL(path.join(root, 'electron/main.mjs')).href;
    const sourceLines = fs.readFileSync(path.join(root, 'electron/main.mjs'), 'utf8').split('\n');
    const startup = await main.call('Debugger.setBreakpointByUrl', { url: mainUrl,
      lineNumber: sourceLines.findIndex(line => line.startsWith('if (!app.isPackaged)')) });
    let startupFrame;
    const onStartupPause = event => {
      if (event.method !== 'Debugger.paused') return;
      if (event.params.hitBreakpoints?.includes(startup.breakpointId)) startupFrame = event.params.callFrames[0];
      else main.call('Debugger.resume').catch(error => report.errors.push(error.message));
    };
    main.listeners.push(onStartupPause);
    await main.call('Runtime.runIfWaitingForDebugger');
    await wait('owned main module before startup', () => startupFrame);
    // Imported Electron bindings are initialized here; do not require modules in
    // Node's earlier bootstrap pause (where its own loader is not initialized).
    report.mainIdentity = checkedValue(await main.call('Debugger.evaluateOnCallFrame', { callFrameId: startupFrame.callFrameId,
      expression: `(() => { globalThis.__t01Electron = {app,BrowserWindow,Menu,desktopCapturer:require('electron').desktopCapturer}; app.setPath('userData', ${JSON.stringify(userData)}); app.whenReady().then(()=>{session.defaultSession.setSpellCheckerLanguages([]); session.defaultSession.setSpellCheckerEnabled(false); globalThis.__t01SpellcheckDisabled=!session.defaultSession.isSpellCheckerEnabled();}); return {pid:process.pid,type:process.type,execPath:process.execPath,userData:app.getPath('userData')}; })()`, returnByValue: true }));
    assert.equal(report.mainIdentity.pid, electron.pid); assert.notEqual(electron.pid, process.pid);
    assert.equal(report.mainIdentity.type, 'browser'); assert.equal(report.mainIdentity.userData, userData);
    assert.equal(fs.realpathSync(`/proc/${electron.pid}/exe`), fs.realpathSync(electronBin));
    main.listeners = main.listeners.filter(listener => listener !== onStartupPause);
    await main.call('Debugger.removeBreakpoint', { breakpointId: startup.breakpointId });
    await main.call('Debugger.resume');
    const pageTarget = await wait('matching renderer /json/list page', async () => {
      const targets = await jsonAt(report.ports.rendererCDP, '/json/list');
      return targets?.find(t => t.type === 'page' && t.url === 'http://127.0.0.1:1420/');
    });
    report.pageTarget = pageTarget; page = await CDP.open(pageTarget.webSocketDebuggerUrl); sockets.push(page);
    report.spellcheckDisabled = await main.evaluate('globalThis.__t01SpellcheckDisabled');
    assert.equal(report.spellcheckDisabled, true, 'Suppress Chromium dictionary download');
    await page.call('Runtime.enable');
    await wait('UI settings loaded', async () => await page.evaluate(`!!document.querySelector('#chat-model-select option[value="${apiId}::t01-planner"]')`));
    report.windowIdentity = await main.evaluate(`__t01Electron.BrowserWindow.getAllWindows().filter(w=>w.webContents.getURL()==='http://127.0.0.1:1420/').map(w=>({pid:w.webContents.getOSProcessId(),url:w.webContents.getURL(),webContentsId:w.webContents.id}))`);
    assert.equal(report.windowIdentity.length, 1);
    scanOwned(); assert(owned.has(report.windowIdentity[0].pid), 'Renderer must descend from owned Electron');
    const host = processes().find(p => p.ppid === electron.pid && p.exe === fs.realpathSync(hostBin));
    assert(host, 'Owned real Rust sidecar missing'); report.host = host;
    assert.equal(identity(`/proc/${host.pid}/exe`).sha256, report.hostBinaryIdentity.sha256);
    const hostEnv = Object.fromEntries(fs.readFileSync(`/proc/${host.pid}/environ`, 'utf8').split('\0').filter(s => s.startsWith('WORKBENCH_SETTINGS_DIR=')).map(s => s.split(/=(.*)/s).slice(0, 2)));
    assert.equal(hostEnv.WORKBENCH_SETTINGS_DIR, path.join(userData, 'config')); report.hostSettingsDir = hostEnv.WORKBENCH_SETTINGS_DIR;
    report.portOwnership = 'ss is not installed in this isolated workbench; child identities and listener cleanup retained';
    // Observational debugger pause captures main session and the actual open_project result.
    await main.call('Debugger.enable');
    const lines = fs.readFileSync(path.join(root, 'electron/main.mjs'), 'utf8').split('\n');
    const lineNumber = lines.findIndex((line, index) => line.includes('pathGrants.clear()') && index > lines.findIndex(l => l.startsWith('async function openSession')));
    let resultBreakpoint;
    main.listeners.push(event => {
      if (event.method !== 'Debugger.paused') return;
      (async () => {
        try {
          const isResult = resultBreakpoint && event.params.hitBreakpoints?.includes(resultBreakpoint.breakpointId);
          const value = checkedValue(await main.call('Debugger.evaluateOnCallFrame', { callFrameId: event.params.callFrames[0].callFrameId,
            expression: isResult ? 'message.result' : '({mainRoot:sessionRoot,hostRoot:result.root})', returnByValue: true }));
          if (isResult) {
            report.observed.hostResponse = value;
            (report.observed.hostResponses ??= []).push(value);
          } else report.observed.project = value;
        } catch (error) { report.errors.push(error.message); }
        finally { await main.call('Debugger.resume'); }
      })().catch(error => report.errors.push(error.message));
    });
    const breakpoint = await main.call('Debugger.setBreakpointByUrl', { url: pathToFileURL(path.join(root, 'electron/main.mjs')).href, lineNumber });
    // Display the EXISTING native Open Recent submenu, then choose it with XTest.
    // No .click callback, synthetic menu IPC, useProject or host dispatch invocation.
    await main.evaluate(`(() => { const w=__t01Electron.BrowserWindow.getAllWindows().find(w=>w.webContents.getURL()==='http://127.0.0.1:1420/'); w.show(); w.focus(); })()`);
    await wait('owned native window focused', async () => await main.evaluate(`__t01Electron.BrowserWindow.getAllWindows().some(w=>w.isFocused())`));
    report.nativeMenu = await main.evaluate(`(() => { const w=__t01Electron.BrowserWindow.getAllWindows().find(w=>w.webContents.getURL()==='http://127.0.0.1:1420/'); const file=__t01Electron.Menu.getApplicationMenu().items.find(i=>i.label.replace('&','')==='File'); const recent=file.submenu.items.find(i=>i.label==='Open Recent').submenu; globalThis.__t01MenuEvents=[]; recent.on('menu-will-show',()=>__t01MenuEvents.push('show')); recent.on('menu-will-close',()=>__t01MenuEvents.push('close')); recent.popup({window:w}); return recent.items.map(i=>({label:i.label,type:i.type,enabled:i.enabled})); })()`);
    assert(report.nativeMenu[0].label.startsWith(`${path.basename(project)} — `));
    assert(report.nativeMenu[0].label.endsWith(project.slice(-56)));
    await wait('native menu shown', async () => (await main.evaluate('__t01MenuEvents')).includes('show'));

    fs.writeFileSync(path.join(out, 'native-menu.png'), Buffer.from((await page.call('Page.captureScreenshot', { format: 'png' })).data, 'base64'));
    await page.evaluate(`globalThis.__a02MenuObserved=[];window.workbench.onMenu(id=>__a02MenuObserved.push(id));true`);
    report.nativeSelection = native('keys');
    report.afterNativeSelection = await main.evaluate(`({events:__t01MenuEvents,focused:__t01Electron.BrowserWindow.getAllWindows().map(w=>w.isFocused())})`);
    await wait('actual recent UI host/main open', () => report.observed.project || (report.errors.length ? Promise.reject(new Error(report.errors.join('\n'))) : false));
    assert.equal(report.observed.project.mainRoot, project); assert.equal(report.observed.project.hostRoot, project);
    await main.call('Debugger.removeBreakpoint', { breakpointId: breakpoint.breakpointId });
    await wait('renderer project transition completed', async () => await page.evaluate(`document.querySelector('#shell-project-name')?.title === ${JSON.stringify(project)}`));
    report.observed.rendererRoot = await page.evaluate(`document.querySelector('#shell-project-name').title`);
    report.nativeMenuEvents = await main.evaluate('__t01MenuEvents');
    assert(report.nativeMenuEvents.includes('close'));
    report.steps.push({ pass: 'actual native recent selection and three-layer project identity' });
    report.modelChange = await page.evaluate(`(() => { const s=document.querySelector('#chat-model-select'); s.value=${JSON.stringify(`${apiId}::t01-planner`)}; s.dispatchEvent(new Event('change',{bubbles:true})); return s.value; })()`);
    await wait('model selection persisted by change handler', () => JSON.parse(fs.readFileSync(settingsFile)).selectedChatModel?.apiId === apiId);
    const settings = JSON.parse(fs.readFileSync(settingsFile)); assert(!settings.apis[0].apiKey, 'Fixture key must migrate off disk');
    report.keyringDuring = native('check', apiId); assert.equal(report.keyringDuring.remaining, 1);
    if (scenario) {
      const sidecarLines = fs.readFileSync(path.join(root, 'electron/sidecar.mjs'), 'utf8').split('\n');
      resultBreakpoint = await main.call('Debugger.setBreakpointByUrl', { url: pathToFileURL(path.join(root, 'electron/sidecar.mjs')).href,
        lineNumber: sidecarLines.findIndex(line => line.includes('pending.resolve(message.result)')), condition: "typeof message.result?.reply === 'string'" });
      await scenario.exercise({ page, main, report, save, wait, out, project, root });
      await main.call('Debugger.removeBreakpoint', { breakpointId: resultBreakpoint.breakpointId });
    } else {
    const sidecarLines = fs.readFileSync(path.join(root, 'electron/sidecar.mjs'), 'utf8').split('\n');
    resultBreakpoint = await main.call('Debugger.setBreakpointByUrl', { url: pathToFileURL(path.join(root, 'electron/sidecar.mjs')).href,
      lineNumber: sidecarLines.findIndex(line => line.includes('pending.resolve(message.result)')), condition: `message.result?.reply === ${JSON.stringify(marker)}` });
    const intent = 'T01: reply with the terminal tracer acknowledgement. Do not inspect, write, or generate media.';
    report.submission = await page.evaluate(`(() => { const f=document.querySelector('#chat-form'); const input=document.querySelector('#chat-input'); const send=document.querySelector('#chat-send-btn'); if(!f||!input||!send||send.disabled) throw new Error('Chat form unavailable'); input.value=${JSON.stringify(intent)}; input.dispatchEvent(new Event('input',{bubbles:true})); f.requestSubmit(send); return {form:f.id,submitter:send.id,intent:input.value}; })()`);
    await wait('loopback planner request', () => report.requests.length === 1);
    if (injectFailure) throw new Error('T01 deliberate bounded failure after actual chat submission');
    await wait('actual terminal host envelope', () => report.observed.hostResponse);
    const response = report.observed.hostResponse; assert.equal(response.reply, marker); assert.equal(response.continueSuggested, false);
    assert(!response.stopped && !response.plannerPaused); assert(!(response.toolEvents ?? []).some(e => /approval|error|spend/i.test(e)));
    await main.call('Debugger.removeBreakpoint', { breakpointId: resultBreakpoint.breakpointId });
    await wait('terminal Director assistant bubble and UI cleanup', async () => await page.evaluate(`Array.from(document.querySelectorAll('#chat-messages .chat-bubble')).some(b=>b.querySelector('.role')?.textContent==='Director' && b.querySelector('p')?.textContent===${JSON.stringify(marker)}) && !document.querySelector('#chat-messages .chat-pending') && !document.querySelector('#chat-messages button') && !document.querySelector('#chat-send-btn').disabled && document.querySelector('#chat-stop-btn').classList.contains('hidden')`));
    report.observed.terminal = await page.evaluate(`({text:document.querySelector('#chat-messages').innerText,sendDisabled:document.querySelector('#chat-send-btn').disabled,stopDisabled:document.querySelector('#chat-stop-btn').disabled})`);
    assert.equal(report.requests.length, 1); assert.equal(report.requests[0].body.model, 't01-planner');
    assert.deepEqual(report.requests[0].body.messages.filter(m => m.role === 'user'), [{ role: 'user', content: intent }]);
    assert(report.requests[0].body.messages[0].content.includes(project), 'Real host planner context must carry opened project');
    report.deniedNetwork = denied; assert.deepEqual(denied, [], 'Unexpected non-loopback HTTP(S) attempt');
    const screenshot = await page.call('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(path.join(out, 'terminal.png'), Buffer.from(screenshot.data, 'base64'));
    report.steps.push({ pass: 'change event + actual form -> real host -> exactly one loopback terminal Director response' });
    }
  } catch (error) {
    failure = error; report.errors.push(error.stack);
    if (main) try { report.failureMain = await main.evaluate(`({menus:globalThis.__t01MenuEvents,windows:__t01Electron?.BrowserWindow.getAllWindows().map(w=>({focused:w.isFocused(),url:w.webContents.getURL()}))})`); } catch {}
    if (page) try { report.failurePage = await page.evaluate(`({menuEvents:globalThis.__a02MenuObserved,title:document.querySelector('#shell-project-name')?.title,text:document.body.innerText})`); } catch {}
  }
  finally {
    clearInterval(ownershipTimer);
    const errors = report.cleanup.errors = [];
    const attempt = async actions => errors.push(...await attemptAll(actions));
    await attempt([['ownership scan', () => scanOwned()],
      ['graceful quit', async () => { if (main) await main.evaluate('setTimeout(()=>__t01Electron.app.quit(),0); true'); }]]);
    await attempt(sockets.map((socket, i) => [`CDP close ${i}`, () => socket.close()]));
    await sleep(200);
    await attempt(children.map(child => [`root SIGTERM ${child.pid}`, () => {
      const expected = owned.get(child.pid); const current = processInfo(child.pid);
      if (expected && current?.birth === expected.birth && child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
    }]));
    await sleep(800);
    await attempt([['final ownership scan', () => scanOwned()]]);
    await attempt([...owned.values()].reverse().map(info => [`owned SIGKILL ${info.pid}`, () => {
      if (processInfo(info.pid)?.birth === info.birth) {
        try { process.kill(info.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
      }
    }]));
    await attempt([...serverSockets].map((socket, i) => [`HTTP socket ${i}`, () => socket.destroy()]));
    await attempt(servers.map((server, i) => [`HTTP server ${i}`, async () => {
      if (!server.listening) return;
      await deadline(new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())), 'HTTP close');
    }]));
    await attempt([['process residue', async () => {
      for (let i = 0; i < 60 && [...owned.values()].some(p => processInfo(p.pid)?.birth === p.birth); i++) await sleep(100);
      report.cleanup.processesRemaining = [...owned.values()].map(p => processInfo(p.pid)).filter(p => p && p.birth === owned.get(p.pid).birth);
      assert.equal(report.cleanup.processesRemaining.length, 0, 'Owned processes remain');
    }], ['port residue', async () => {
      report.cleanup.ports = await Promise.all(Object.entries(report.ports).map(async ([name, port]) => ({ name, port, accepting: await accepting(port) })));
      assert(!report.cleanup.ports.some(p => p.accepting), 'Owned listener remains');
    }]]);
    await attempt([['network evidence', () => {
      if (!report.pageTarget) return;
      const log = JSON.parse(fs.readFileSync(path.join(out, 'chromium-netlog.json'), 'utf8'));
      report.chromiumHttpUrls = [...new Set(log.events.map(event => event.params?.url).filter(url => typeof url === 'string' && /^https?:/.test(url)))];
      assert(report.chromiumHttpUrls.every(url => new URL(url).hostname === '127.0.0.1'), 'Unexpected Chromium HTTP(S) URL');
    }]]);
    report.cleanup.keyring = [];
    await attempt([apiId, ollamaId].map(id => [`keyring ${id}`, () => report.cleanup.keyring.push(native('clear', id))]));
    await attempt(streams.map((stream, i) => [`log ${i}`, () => deadline(new Promise(resolve => stream.end(resolve)), 'log close')]));
    report.owned = [...owned.values()]; report.groups = groups;
    report.errors.push(...connectionErrors(sockets));
    if (!failure && (asyncFailure || errors.length || report.errors.length || report.deniedNetwork?.length)) {
      failure = asyncFailure ?? new Error('Tracer cleanup/async/network verification failed'); report.errors.push(failure.stack);
    }
    report.verdict = failure ? 'FAIL' : 'PASS'; report.exitCode = failure ? 1 : 0; save('result.json', report);
  }
  console.log(JSON.stringify({ verdict: report.verdict, out, exitCode: report.exitCode, errors: report.errors }));
  return report.exitCode;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2); const outIndex = args.indexOf('--out');
  if (outIndex < 0 || !args[outIndex + 1] || args.some((v, i) => i !== outIndex + 1 && !['--out', '--inject-failure'].includes(v))) {
    console.error('Usage: node tests/ui-tracer.mjs --out ABSOLUTE_NEW_DIRECTORY [--inject-failure]'); process.exitCode = 2;
  } else {
    try { process.exitCode = await run(args[outIndex + 1], args.includes('--inject-failure')); }
    catch (error) { console.error(error); process.exitCode = 1; }
  }
}
