import { expect, it } from 'vitest';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
const require=createRequire(import.meta.url);
it('permits the Scene Look speech command through the real Electron allowlist and native dispatcher',()=>{
 const {HOST_COMMANDS}=require('../electron/commandAllowlist.cjs');
 expect(HOST_COMMANDS.has('workflow_prepare_dialogue_audio')).toBe(true);
 expect(readFileSync('host/src/lib.rs','utf8')).toContain('"workflow_prepare_dialogue_audio" => workflow::cmd_prepare_dialogue_audio');
 expect(readFileSync('src/workflowDesk.ts','utf8')).toContain('workspace.prepend(renderSceneDialogueAudio(scene');
});
