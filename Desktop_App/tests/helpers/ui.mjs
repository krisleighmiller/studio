import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import crypto from 'node:crypto';

export const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
export async function deadline(promise, label) {
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
export async function jsonAt(port, endpoint) {
  try {
    const response = await fetch(`http://127.0.0.1:${port}${endpoint}`, { signal: AbortSignal.timeout(800) });
    if (!response.ok) return null;
    return await response.json();
  } catch { return null; }
}
export async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once('error', reject); server.listen(0, '127.0.0.1', resolve);
  });
  return server.address().port;
}
export async function freePort() {
  const server = net.createServer(); const port = await listen(server);
  await new Promise(resolve => server.close(resolve)); return port;
}
export async function accepting(port) {
  return new Promise(resolve => {
    const socket = net.connect({ host: '127.0.0.1', port });
    socket.setTimeout(500);
    socket.once('connect', () => { socket.destroy(); resolve(true); });
    socket.once('error', () => { socket.destroy(); resolve(false); });
    socket.once('timeout', () => { socket.destroy(); resolve(false); });
  });
}
export function processInfo(pid) {
  try {
    const base = `/proc/${pid}`; const stat = fs.readFileSync(`${base}/stat`, 'utf8');
    const fields = stat.slice(stat.lastIndexOf(')') + 2).split(' ');
    let exe = null; let argv = [];
    try { exe = fs.readlinkSync(`${base}/exe`); argv = fs.readFileSync(`${base}/cmdline`, 'utf8').split('\0').filter(Boolean); } catch { /* Preserve zombie/stat identity until genuinely reaped. */ }
    return { pid, state: fields[0], ppid: +fields[1], pgrp: +fields[2], sid: +fields[3], birth: fields[19], exe, argv };
  } catch { return null; }
}
export function processes() {
  return fs.readdirSync('/proc').filter(p => /^\d+$/.test(p)).map(p => processInfo(+p)).filter(Boolean);
}
export function identity(file) {
  const stat = fs.statSync(file);
  return { path: fs.realpathSync(file), sha256: crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex'),
    size: stat.size, inode: stat.ino };
}
export function displayEnv() {
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


export function containedPath(root, relative) {
  assert(!path.isAbsolute(relative) && !relative.split('/').includes('..'), 'Relative confined path required');
  assert.equal(fs.realpathSync(root),root,'Fixture root symlink');
  const full=path.join(root,relative);
  let current=root;
  for(const part of relative.split('/').filter(Boolean)) {
    current=path.join(current,part);
    try { fs.lstatSync(current); } catch(e) { if(e.code==='ENOENT') continue; throw e; }
    assert.equal(fs.realpathSync(current),current,'Redirected fixture path');
  }
  return full;
}
