/** Owned native Electron/UI/host lifecycle; no default or retired chat scenario. */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import crypto from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { appRoot as root, prepareOutput } from './runtime.mjs';
import { CDP, checkedValue, until, attemptAll, admitGroups, plannerBody, connectionErrors,
  sleep, deadline, jsonAt, listen, freePort, accepting, processInfo, processes, identity, displayEnv } from './ui.mjs';

export function nativeOptions(env = process.env) {
  const sandbox = env.STUDIO_TEST_NO_SANDBOX ?? '0';
  assert(['0', '1'].includes(sandbox), 'STUDIO_TEST_NO_SANDBOX must be 0 or 1');
  const socketInspector = env.STUDIO_TEST_SOCKET_INSPECTOR ?? 'ss';
  assert(['ss', 'proc'].includes(socketInspector), 'STUDIO_TEST_SOCKET_INSPECTOR must be ss or proc');
  return { noSandbox: sandbox === '1', socketInspector };
}

export function requireScenario(scenario) {
  assert(typeof scenario?.setup === 'function' && typeof scenario?.exercise === 'function', 'Native scenario requires setup and exercise');
  return scenario;
}

export function verifyListeners(entries, method = 'ss') {
  assert(['ss', 'proc'].includes(method), 'Unsupported listener ownership inspector');
  const listening = method === 'ss' ? execFileSync('ss', ['-H', '-ltnp'], {encoding:'utf8', timeout:5000}).trim().split('\n') : null;
  const tcp = method === 'proc' ? fs.readFileSync('/proc/net/tcp', 'utf8').trim().split('\n').slice(1).map(line => line.trim().split(/\s+/)) : null;
  return entries.map(({name, port, expectedPid}) => {
    if (method === 'ss') {
      const lines = listening.filter(line => line.split(/\s+/)[3]?.endsWith(`:${port}`));
      assert.equal(lines.length, 1, `One listener for ${name}`);
      assert(lines[0].includes(`127.0.0.1:${port}`) && lines[0].includes(`pid=${expectedPid},`), `Wrong listener owner: ${lines[0]}`);
      return {name, port, expectedPid, method, socket:lines[0]};
    }
    const portHex = port.toString(16).toUpperCase().padStart(4, '0');
    const lines = tcp.filter(fields => fields[1].split(':')[1] === portHex && fields[3] === '0A');
    assert.equal(lines.length, 1, `One listener for ${name}`);
    const fields = lines[0], inode = fields[9];
    const owned = fs.readdirSync(`/proc/${expectedPid}/fd`).some(fd => {
      try { return fs.readlinkSync(`/proc/${expectedPid}/fd/${fd}`) === `socket:[${inode}]`; } catch { return false; }
    });
    assert(fields[1] === `0100007F:${portHex}` && owned, `Wrong listener owner: ${name} pid=${expectedPid} inode=${inode}`);
    return {name, port, expectedPid, method, inode, socket:fields.join(' ')};
  });
}

export async function run(out, scenario, options = nativeOptions()) {
  requireScenario(scenario);
  const injectFailure = options.injectFailure ?? false;
  out = prepareOutput('sb14-native-ui', out);
  const save = (name, value) => fs.writeFileSync(path.join(out, name), JSON.stringify(value, null, 2));
  const apiId = `t01-ui-${crypto.randomUUID().replaceAll('-', '')}`;
  const ollamaId = `t01-ui-${crypto.randomUUID().replaceAll('-', '')}`;
  const report = { command: process.argv, root, options, injectFailure, apiId, ollamaId, runner: processInfo(process.pid), steps: [], requests: [],
    observed: {}, errors: [], owned: [], ports: {}, cleanup: {} };
  const children = []; const groups = []; const servers = []; const serverSockets = new Set(); const sockets = []; const streams = []; const owned = new Map();
  let asyncFailure;
  const failAsync = error => { asyncFailure ??= error; };
  const wait = (label, fn, timeout) => until(label, () => { if (asyncFailure) throw asyncFailure; return fn(); }, timeout);
  const env = { ...process.env, TMPDIR: path.join(out, 'tmp') };
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
    Object.assign(env, displayEnv());
    const configured = await scenario.setup({ out, env, report, trackServer });
    fs.mkdirSync(env.TMPDIR, { recursive: true });
    const userData = configured?.userData ?? path.join(out, 'userData');
    const project = configured?.project ?? path.join(out, 'project');
    fs.mkdirSync(path.join(userData, 'config'), { recursive: true });
    const hostBin = configured?.hostBin ?? process.env.STUDIO_TEST_HOST_BIN ?? process.env.WORKBENCH_HOST_BIN ?? path.join(root, 'host/target/debug/workbench-host');
    report.hostBinaryIdentity = identity(hostBin);
    const electronBin = path.join(root, 'node_modules/electron/dist/electron');
    report.identities = Object.fromEntries(['tests/helpers/native-runner.mjs', 'tests/helpers/ui.mjs', 'tests/sb14-native-ui.mjs', 'tests/ui-tracer-native.py', 'electron/main.mjs', 'electron/sidecar.mjs',
      'electron/preload.cjs', 'src/main.ts', 'package.json', 'node_modules/electron/dist/electron']
      .map(file => [file, identity(path.join(root, file))]));
    report.keyringBefore = [native('check', apiId), native('check', ollamaId)];
    assert(report.keyringBefore.every(item => item.remaining === 0));
    // No provider is part of this native fixture. Any attempted call fails closed.
    const providerGuard = http.createServer((request, response) => {
      (async () => {
        const record = { method: request.method, path: request.url, body: await plannerBody(request) };
        report.requests.push(record); save('provider-requests.json', report.requests);
        throw new Error('Unexpected provider call in native UI scenario');
      })().catch(error => { failAsync(error); response.writeHead(403); response.end('Native fixture denies provider calls'); });
    });
    trackServer(providerGuard); report.ports.providerGuard = await listen(providerGuard);
    const denied = report.deniedNetwork = [];
    const deny = http.createServer((req, res) => { denied.push({ method: req.method, url: req.url }); res.writeHead(403); res.end(); });
    deny.on('connect', (req, socket) => { denied.push({ method: req.method, url: req.url }); socket.end('HTTP/1.1 403 Forbidden\r\n\r\n'); });
    trackServer(deny); report.ports.denyProxy = await listen(deny);
    const baseUrl = `http://127.0.0.1:${report.ports.providerGuard}`;
    const api = (id, kind, enabled) => ({ id, name: 'T01 loopback only', kind, baseUrl, apiKey: enabled ? 't01-synthetic-fixture-only' : '', secretKey: '', enabled,
      catalog: enabled ? [{ id: 't01-planner', displayName: 'T01 Planner', type: 'Text' }] : [], catalogFetchedAt: new Date().toISOString(), pinnedLlmModelIds: enabled ? ['t01-planner'] : [] });
    const settingsFile = path.join(userData, 'config/settings.json');
    const fixtureSettings = { apis: [api(apiId, 'atlas-cloud', true), api(ollamaId, 'ollama', false)],
      capabilityDefaults: {}, adultContentEnabled: false, adultCapabilityDefaults: {}, selectedChatModel: null };
    if (configured?.configureSettings) configured.configureSettings(fixtureSettings);
    fs.writeFileSync(settingsFile, JSON.stringify(fixtureSettings));
    fs.writeFileSync(path.join(userData, 'recent-projects.json'), JSON.stringify([project]));
    report.fixture = { userData, settingsFile, project, baseUrl };
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
    const electron = start('electron', electronBin, ['.', ...(options.noSandbox ? ['--no-sandbox'] : []), '--ozone-platform=x11', `--inspect-brk=127.0.0.1:${report.ports.mainInspector}`,
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
    await wait('native UI bridge ready', async () => await page.evaluate(`!!window.workbench && !!document.querySelector('#shell-project-name')`));
    report.windowIdentity = await main.evaluate(`__t01Electron.BrowserWindow.getAllWindows().filter(w=>w.webContents.getURL()==='http://127.0.0.1:1420/').map(w=>({pid:w.webContents.getOSProcessId(),url:w.webContents.getURL(),webContentsId:w.webContents.id}))`);
    assert.equal(report.windowIdentity.length, 1);
    scanOwned(); assert(owned.has(report.windowIdentity[0].pid), 'Renderer must descend from owned Electron');
    const host = processes().find(p => p.ppid === electron.pid && p.exe === fs.realpathSync(hostBin));
    assert(host, 'Owned real Rust sidecar missing'); report.host = host;
    assert.equal(identity(`/proc/${host.pid}/exe`).sha256, report.hostBinaryIdentity.sha256);
    const hostEnv = Object.fromEntries(fs.readFileSync(`/proc/${host.pid}/environ`, 'utf8').split('\0').filter(s => s.startsWith('WORKBENCH_SETTINGS_DIR=')).map(s => s.split(/=(.*)/s).slice(0, 2)));
    assert.equal(hostEnv.WORKBENCH_SETTINGS_DIR, path.join(userData, 'config')); report.hostSettingsDir = hostEnv.WORKBENCH_SETTINGS_DIR;
    report.portOwnership = verifyListeners(Object.entries(report.ports).map(([name, port]) => ({
      name, port, expectedPid: name === 'vite' ? children[0].pid : ['mainInspector', 'rendererCDP'].includes(name) ? electron.pid : process.pid,
    })), options.socketInspector);
    // Observational debugger pause captures main session and the actual open_project result.
    await main.call('Debugger.enable');
    const lines = fs.readFileSync(path.join(root, 'electron/main.mjs'), 'utf8').split('\n');
    const lineNumber = lines.findIndex((line, index) => line.includes('pathGrants.clear()') && index > lines.findIndex(l => l.startsWith('async function openSession')));
    main.listeners.push(event => {
      if (event.method !== 'Debugger.paused') return;
      (async () => {
        try {
          report.observed.project = checkedValue(await main.call('Debugger.evaluateOnCallFrame', {
            callFrameId: event.params.callFrames[0].callFrameId,
            expression: '({mainRoot:sessionRoot,hostRoot:result.root})', returnByValue: true,
          }));
        } catch (error) { report.errors.push(error.message); }
        finally { await main.call('Debugger.resume'); }
      })().catch(error => report.errors.push(error.message));
    });
    const breakpoint = await main.call('Debugger.setBreakpointByUrl', { url: pathToFileURL(path.join(root, 'electron/main.mjs')).href, lineNumber });
    // Display the EXISTING native Open Recent submenu, then choose it with XTest.
    // No .click callback, synthetic menu IPC, useProject or host dispatch invocation.
    await main.evaluate(`(() => { const w=__t01Electron.BrowserWindow.getAllWindows().find(w=>w.webContents.getURL()==='http://127.0.0.1:1420/'); w.show(); w.focus(); })()`);
    await wait('owned native window focused', async () => await main.evaluate(`__t01Electron.BrowserWindow.getAllWindows().some(w=>w.isFocused())`));
    report.nativeMenu = await main.evaluate(`(() => { const w=__t01Electron.BrowserWindow.getAllWindows().find(w=>w.webContents.getURL()==='http://127.0.0.1:1420/'); const file=__t01Electron.Menu.getApplicationMenu().items.find(i=>i.label.replaceAll('&','')==='File'); const recent=file.submenu.items.find(i=>i.label==='Open Recent').submenu; globalThis.__t01MenuEvents=[]; recent.on('menu-will-show',()=>__t01MenuEvents.push('show')); recent.on('menu-will-close',()=>__t01MenuEvents.push('close')); recent.popup({window:w}); return recent.items.map(i=>({label:i.label,type:i.type,enabled:i.enabled})); })()`);
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
    await wait('fixture key migrated off disk', () => !JSON.parse(fs.readFileSync(settingsFile)).apis[0].apiKey);
    report.keyringDuring = native('check', apiId); assert.equal(report.keyringDuring.remaining, 1);
    if (injectFailure) throw new Error('Native runner deliberate bounded failure after project open');
    await scenario.exercise({ page, main, report, save, wait, out, project, root });
    assert.equal(report.requests.length, 0, 'No provider calls allowed');
    assert.deepEqual(denied, [], 'Unexpected non-loopback HTTP(S) attempt');
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
