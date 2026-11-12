import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
const {HOST_COMMANDS}=createRequire(import.meta.url)('./commandAllowlist.cjs');
test('local audio version/selection are admitted while unrelated commands remain denied',()=>{
 for(const command of ['workflow_version_audio','workflow_select_performance','workflow_prepare_render']) assert.equal(HOST_COMMANDS.has(command),true,command);
 for(const command of ['workflow_select_any_path','workflow_fake_command','workflow_approve_everything']) assert.equal(HOST_COMMANDS.has(command),false,command);
});
