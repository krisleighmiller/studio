import { expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
it('standalone audio settles as draft without publishing a video card',()=>{
 const source=readFileSync('host/src/workflow_continuation.rs','utf8');
 const branch=source.slice(source.indexOf('if stage == Stage::Audio {'),source.indexOf('} else {',source.indexOf('if stage == Stage::Audio {')));
 expect(branch).toContain('dialogue_audio::settle');expect(branch).not.toContain('publish(parent');
});
it('video rejects missing selected speech before it can reach a TTS request',()=>{
 const source=readFileSync('host/src/specialists.rs','utf8');
 const run=source.slice(source.indexOf('fn run_text_sync('));
 expect(run.indexOf('Video generation requires saved dialogue audio')).toBeGreaterThan(0);
 expect(run.slice(0,run.indexOf('// --- Step 2: pad handles ---'))).not.toContain('Generating performance TTS');
});
