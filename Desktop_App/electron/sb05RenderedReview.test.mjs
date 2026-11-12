import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
const {HOST_COMMANDS}=createRequire(import.meta.url)('./commandAllowlist.cjs');
test('explicit rendered review is admitted without relaxing deny-by-default',()=>{
 assert.equal(HOST_COMMANDS.has('workflow_approve_rendered_sequence'),true);
 assert.equal(HOST_COMMANDS.has('workflow_approve_animatic'),true);
 assert.equal(HOST_COMMANDS.has('workflow_fake_command'),false);
 assert.equal(HOST_COMMANDS.has('workflow_approve_everything'),false);
});
