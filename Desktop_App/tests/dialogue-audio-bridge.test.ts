import { expect, it } from 'vitest';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
const require=createRequire(import.meta.url);
it('permits Cut Sketches speech through the Electron allowlist and native dispatcher',()=>{
 const {HOST_COMMANDS}=require('../electron/commandAllowlist.cjs');
 expect(HOST_COMMANDS.has('workflow_prepare_dialogue_audio')).toBe(true);
 expect(readFileSync('host/src/lib.rs','utf8')).toContain('"workflow_prepare_dialogue_audio" => workflow::cmd_prepare_dialogue_audio');
 const sketch=readFileSync('src/studio/surfaces/sketchStage.ts','utf8');
 expect(sketch).toContain('import { renderSceneDialogueAudio } from "../sceneDialogueAudio"');
 expect(sketch).toContain('if (emitAudio && cut.speech_mode === "source_video_lipsync") card.append(renderSceneDialogueAudio({ ...scene, cuts: [cut] }, emitAudio))');
});
