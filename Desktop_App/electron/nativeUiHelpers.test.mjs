import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import { nativeOptions, verifyListeners, requireScenario } from '../tests/helpers/native-runner.mjs';

test('native runner sandbox stays enabled unless explicitly opted out; invalid options fail closed', () => {
  assert.deepEqual(nativeOptions({}), { noSandbox: false, socketInspector: 'ss' });
  assert.deepEqual(nativeOptions({ STUDIO_TEST_NO_SANDBOX: '1', STUDIO_TEST_SOCKET_INSPECTOR: 'proc' }), { noSandbox: true, socketInspector: 'proc' });
  assert.throws(() => nativeOptions({ STUDIO_TEST_NO_SANDBOX: 'true' }), /STUDIO_TEST_NO_SANDBOX/);
  assert.throws(() => nativeOptions({ STUDIO_TEST_SOCKET_INSPECTOR: 'skip' }), /STUDIO_TEST_SOCKET_INSPECTOR/);
});

test('native runner requires an explicit setup and exercise, never a default chat scenario', () => {
  for (const scenario of [null, {}, {setup() {}}]) assert.throws(() => requireScenario(scenario), /setup and exercise/);
  const scenario = {setup() {}, exercise() {}};
  assert.equal(requireScenario(scenario), scenario);
  const root = path.resolve(import.meta.dirname, '../tests');
  const source = fs.readFileSync(path.join(root, 'helpers/native-runner.mjs'), 'utf8');
  assert.doesNotMatch(source, /#chat-|director_action|selectedChatModel\?\./);
});

test('proc listener inspection verifies real loopback inode ownership and rejects wrong owner/port', async () => {
  const server = net.createServer();
  await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
  const port = server.address().port;
  const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {stdio:'ignore'});
  await once(child, 'spawn');
  try {
    const result = verifyListeners([{name:'fixture', port, expectedPid:process.pid}], 'proc');
    assert.equal(result[0].expectedPid, process.pid);
    assert.equal(result[0].method, 'proc');
    assert.match(result[0].inode, /^\d+$/);
    assert.throws(() => verifyListeners([{name:'wrong owner', port, expectedPid:child.pid}], 'proc'), /Wrong listener owner/);
    assert.throws(() => verifyListeners([{name:'missing listener', port:0, expectedPid:process.pid}], 'proc'), /One listener/);
  } finally {
    const exited=once(child, 'exit');child.kill();await exited;
    await new Promise(resolve => server.close(resolve));
  }
});
