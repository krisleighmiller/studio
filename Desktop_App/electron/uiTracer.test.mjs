import test from 'node:test';
import assert from 'node:assert/strict';
import { CDP, checkedValue, until, attemptAll, admitGroups, plannerBody, connectionErrors } from '../tests/ui-tracer.mjs';
import { Readable } from 'node:stream';

class Socket extends EventTarget {
  send(text) { this.request = JSON.parse(text); }
  reply(data) { this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify(data) })); }
  close() { this.dispatchEvent(new Event('close')); }
}

test('UI tracer rejects CDP evaluation exceptions rather than accepting empty values', () => {
  assert.throws(() => checkedValue({ exceptionDetails: { text: 'real page exception' } }), /real page exception/);
  assert.equal(checkedValue({ result: { value: false } }), false);
});

test('UI tracer correlates protocol replies and rejects protocol errors', async () => {
  const socket = new Socket(); const cdp = new CDP(socket);
  const pending = cdp.call('Runtime.enable');
  socket.reply({ id: socket.request.id + 1, result: { wrong: true } });
  socket.reply({ id: socket.request.id, result: { accepted: true } });
  assert.deepEqual(await pending, { accepted: true });
  const error = cdp.call('Unknown.method');
  socket.reply({ id: socket.request.id, error: { message: 'real protocol error' } });
  await assert.rejects(error, /real protocol error/);
  assert.equal(cdp.pending.size, 0);
});

test('UI tracer closes and rejects in-flight requests without retaining their timers', async () => {
  const socket = new Socket(); const cdp = new CDP(socket);
  const request = cdp.call('Runtime.evaluate');
  cdp.close(); await assert.rejects(request, /CDP closed/);
  assert.equal(cdp.pending.size, 0);
});

test('UI tracer polling is bounded and does not swallow assertion failures', async () => {
  await assert.rejects(until('bounded fixture marker', () => false, 1), /Timed out: bounded fixture marker/);
  await assert.rejects(until('assertion', () => { throw new Error('actual assertion marker'); }), /actual assertion marker/);
  assert.equal(await until('ready', () => 'ready'), 'ready');
});

test('UI tracer contains malformed CDP data and rejects current and future work', async () => {
  const socket = new Socket(); const cdp = new CDP(socket);
  const pending = cdp.call('Runtime.enable');
  socket.dispatchEvent(new MessageEvent('message', { data: 'not JSON' }));
  await assert.rejects(pending, SyntaxError);
  await assert.rejects(cdp.call('Runtime.evaluate'), SyntaxError);
  assert.equal(cdp.pending.size, 0);
});

test('UI tracer contains throwing and rejected CDP event listeners', async () => {
  for (const listener of [() => { throw new Error('sync event failure'); }, async () => { throw new Error('async event failure'); }]) {
    const socket = new Socket(); const cdp = new CDP(socket); cdp.listeners.push(listener);
    const pending = cdp.call('Runtime.enable'); socket.reply({ method: 'Test.event' });
    await assert.rejects(pending, /event failure/); assert.equal(cdp.pending.size, 0);
  }
});

test('UI tracer retains a late CDP failure for the final owning-run verdict without another request', () => {
  const socket = new Socket(); const cdp = new CDP(socket);
  socket.dispatchEvent(new MessageEvent('message', { data: 'late malformed JSON' }));
  assert.equal(cdp.pending.size, 0);
  assert.equal(connectionErrors([cdp]).length, 1);
  assert.match(connectionErrors([cdp])[0], /SyntaxError/);
  assert.deepEqual(connectionErrors([new CDP(new Socket())]), []);
});

test('UI tracer attempts later independent cleanups after two failures', async () => {
  const attempted = [];
  const errors = await attemptAll(['first', 'second', 'third'].map(label => [label, async () => {
    attempted.push(label); if (label !== 'second') throw new Error(`${label} failed`);
  }]));
  assert.deepEqual(attempted, ['first', 'second', 'third']);
  assert.deepEqual(errors.map(e => e.label), ['first', 'third']);
});

test('UI tracer rejects malformed and interrupted planner bodies as awaited failures', async () => {
  await assert.rejects(plannerBody(Readable.from(['{bad'])), SyntaxError);
  await assert.rejects(plannerBody((async function* () { yield '{'; throw new Error('stream interrupted'); })()), /stream interrupted/);
  assert.deepEqual(await plannerBody(Readable.from(['{"model":"local"}'])), { model: 'local' });
});

test('UI tracer admits reparented group members but rejects reused leaders and stale ancestry', () => {
  const leader = { pid: 10, ppid: 1, pgrp: 10, sid: 10, birth: '100' };
  const child = { pid: 11, ppid: 10, pgrp: 10, sid: 10, birth: '101' };
  const owned = new Map([[10, leader]]); const groups = [{ pid: 10, birth: '100' }];
  admitGroups([leader, child], groups, owned); assert(owned.has(11));
  const orphan = { pid: 12, ppid: 1, pgrp: 10, sid: 10, birth: '102' };
  admitGroups([{ ...child, ppid: 1 }, orphan], groups, owned); assert(owned.has(12));
  const unrelated = { pid: 13, ppid: 10, pgrp: 10, sid: 10, birth: '201' };
  admitGroups([{ ...leader, birth: '200' }, unrelated], groups, owned);
  assert(!owned.has(13)); assert.equal(groups[0].retired, true);
  const stale = [{ pid: 20, birth: '300' }];
  admitGroups([], stale, new Map());
  const other = new Map(); admitGroups([{ pid: 21, ppid: 20, pgrp: 20, sid: 20, birth: '400' }], stale, other);
  assert.equal(other.size, 0);
});
