import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {containedPath} from '../tests/helpers/ui.mjs';

test('fixture confinement rejects absolute, traversal and symlinked output/work ancestors',()=>{
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
