import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {assemblyInputs, assertHistoryReturn, containedPath} from '../tests/ui-assembly-tracer.mjs';

test('A01 oracle inputs preserve estimated ordered windows and only override opening fade',()=>{
  const a=assemblyInputs('test',false), b=assemblyInputs('test',true);
  assert(!('fade_in' in a));assert(!('fade_out' in a));assert(!('fade_out' in b));assert.equal(b.fade_in,false);
  assert.deepEqual(a.scenes,[{id:1,video:'Scenes/Scene1/SC1.mp4'},{id:2,video:'Scenes/Scene2/SC2.mp4'}]);
  assert.equal(a.trim,'estimated');assert.equal(a.caption_mode,'none');assert.equal(a.overwrite,false);
  assert.deepEqual({...b,output_path:a.output_path,work_dir:a.work_dir,fade_in:undefined},{...a,fade_in:undefined});
});
test('A01 exact executed current result is required, not an approval or older result',()=>{
  const original='Original task', output='Final/revision.mp4';
  const result={toolEvents:['approve:assemble'],reply:'Assemble finished (host-run). '+output};
  const history=[{role:'user',content:original},{role:'assistant',content:result.reply}];
  assertHistoryReturn(history,original,result,output);
  assert.throws(()=>assertHistoryReturn([{role:'user',content:original},{role:'assistant',content:'Approval render '+output}],original,result,output),/Exact current execution/);
  assert.throws(()=>assertHistoryReturn(history,original,{...result,reply:'Assemble finished (host-run). Final/baseline.mp4'},output),/Observed current/);
  assert.throws(()=>assertHistoryReturn([...history,{role:'user',content:'continue'}],original,result,output),/Invented user/);
  assert.throws(()=>assertHistoryReturn(history,'different',result,output),/Original task/);
});
test('A01 rejects absolute, traversal and empty symlinked output/work ancestors',()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'a01-path-')), outside=fs.mkdtempSync(path.join(os.tmpdir(),'a01-outside-'));
  try {
    assert.equal(containedPath(root,'Final/new/out.mp4'),path.join(root,'Final/new/out.mp4'));
    assert.throws(()=>containedPath(root,'../escape'),/Relative confined/);
    assert.throws(()=>containedPath(root,outside),/Relative confined/);
    fs.symlinkSync(outside,path.join(root,'redirect'));
    assert.throws(()=>containedPath(root,'redirect/out.mp4'),/Redirected fixture/);
    fs.symlinkSync(path.join(outside,'missing'),path.join(root,'dangling'));
    assert.throws(()=>containedPath(root,'dangling/out.mp4'));
  } finally { fs.rmSync(root,{recursive:true});fs.rmSync(outside,{recursive:true}); }
});
