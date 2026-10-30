import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { createSpendConfirmationHandler } from '../src/spendConfirmation';
const payload=()=>({projectRoot:'/fixture',provider:'atlas-cloud',category:'media',operation:'atlas_generate_video',model:'fixture-video',quote:{currency:'USD',requestHash:'a'.repeat(64),ledgerRevision:'revision',disclosure:'One exact operation'},args:{model:'fixture-video',params:{prompt:'Local fixture'}}});
function deferred<T>(){let resolve!:(v:T)=>void;let reject!:(e:unknown)=>void;const promise=new Promise<T>((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}
async function flush(){for(let i=0;i<8;i++)await Promise.resolve();}
beforeEach(()=>vi.useFakeTimers());afterEach(()=>vi.useRealTimers());
it('awaits each observation before scheduling another and stops after settlement',async()=>{
 const dispatch=deferred<string>(),read=deferred<void>();let n=0;
 const invoke=vi.fn(async(c:string)=>c==='confirm_unbounded_spend'?'token':dispatch.promise);
 const progress=vi.fn(async()=>{n++;if(n===1)await read.promise;});const settled=vi.fn();
 const handle=createSpendConfirmationHandler({invoke,ask:async()=>true,projectRoot:()=>'/fixture',status:vi.fn(),onProgress:progress,onSettled:settled});
 const pending=handle(payload());await flush();await vi.advanceTimersByTimeAsync(500);expect(progress).toHaveBeenCalledTimes(1);
 await vi.advanceTimersByTimeAsync(2000);expect(progress).toHaveBeenCalledTimes(1);
 read.resolve();await flush();await vi.advanceTimersByTimeAsync(500);expect(progress).toHaveBeenCalledTimes(2);
 dispatch.resolve('saved');await pending;const count=progress.mock.calls.length;await vi.advanceTimersByTimeAsync(3000);expect(progress).toHaveBeenCalledTimes(count);expect(settled).toHaveBeenCalledTimes(1);expect(invoke.mock.calls.filter(([c])=>c==='media_generate_confirmed')).toHaveLength(1);
});
it('invalidates an already pending observation at settlement without invalidating final scope guard',async()=>{
 const dispatch=deferred<string>(),read=deferred<void>();let pollGuard! :()=>boolean;let finalGuard!:()=>boolean;
 const handle=createSpendConfirmationHandler({invoke:async(c)=>c==='confirm_unbounded_spend'?'token':dispatch.promise,ask:async()=>true,projectRoot:()=>'/fixture',projectEpoch:()=>1,status:vi.fn(),onProgress:async(g)=>{pollGuard=g;await read.promise;},onSettled:(_o,g)=>{finalGuard=g;}});
 const pending=handle(payload());await flush();await vi.advanceTimersByTimeAsync(500);expect(pollGuard()).toBe(true);
 dispatch.resolve('saved');await pending;expect(pollGuard()).toBe(false);expect(finalGuard()).toBe(true);read.resolve();await flush();await vi.advanceTimersByTimeAsync(1000);
});
it.each(['different-root','same-root-new-epoch'])('discards in-flight callbacks and status after %s',async(kind)=>{
 let root='/fixture',epoch=1;const dispatch=deferred<string>(),read=deferred<void>();let guard!:()=>boolean;const status=vi.fn(),submitted=vi.fn(),settled=vi.fn();
 const handle=createSpendConfirmationHandler({invoke:async(c)=>c==='confirm_unbounded_spend'?'token':dispatch.promise,ask:async()=>true,projectRoot:()=>root,projectEpoch:()=>epoch,status,onProgress:async(g)=>{guard=g;await read.promise;},onSubmitted:submitted,onSettled:settled});
 const pending=handle(payload());await flush();await vi.advanceTimersByTimeAsync(500);expect(guard()).toBe(true);epoch++;if(kind==='different-root')root='/other';expect(guard()).toBe(false);status.mockClear();dispatch.resolve('saved');await pending;expect(submitted).not.toHaveBeenCalled();expect(settled).not.toHaveBeenCalled();expect(status).not.toHaveBeenCalled();read.resolve();await flush();
});
it('revalidates epoch after token issuance before dispatch',async()=>{
 let epoch=1;const token=deferred<string>();const invoke=vi.fn(async(c:string)=>c==='confirm_unbounded_spend'?token.promise:'MUST NOT DISPATCH');
 const handle=createSpendConfirmationHandler({invoke,ask:async()=>true,projectRoot:()=>'/fixture',projectEpoch:()=>epoch,status:vi.fn()});
 const pending=handle(payload());await flush();expect(invoke).toHaveBeenCalledTimes(1);epoch++;token.resolve('token');await pending;expect(invoke.mock.calls.filter(([c])=>c==='media_generate_confirmed')).toHaveLength(0);
});
it('rejects epoch change during native dialog before token issuance',async()=>{
 let epoch=1;const ask=deferred<boolean>();const invoke=vi.fn();const handle=createSpendConfirmationHandler({invoke,ask:()=>ask.promise,projectRoot:()=>'/fixture',projectEpoch:()=>epoch,status:vi.fn()});const pending=handle(payload());await flush();epoch++;ask.resolve(true);await pending;expect(invoke).not.toHaveBeenCalled();
});
it('performs one failure readback without success callback or automatic dispatch retry',async()=>{
 const invoke=vi.fn(async(c:string)=>{if(c==='confirm_unbounded_spend')return'token';throw Error('provider-failed: exact local failure');});const status=vi.fn(),submitted=vi.fn(),settled=vi.fn();const handle=createSpendConfirmationHandler({invoke,ask:async()=>true,projectRoot:()=>'/fixture',status,onSubmitted:submitted,onSettled:settled});await handle(payload());expect(settled).toHaveBeenCalledTimes(1);expect(settled.mock.calls[0][0]).toMatchObject({success:false});expect(submitted).not.toHaveBeenCalled();expect(status.mock.calls.at(-1)?.[0]).toContain('provider-failed: exact local failure');expect(invoke).toHaveBeenCalledTimes(2);
});
it('actually tolerates an observation error while pending and preserves original result',async()=>{
 const dispatch=deferred<string>(),status=vi.fn(),progress=vi.fn(async()=>{throw Error('snapshot failed');});const invoke=vi.fn(async(c:string)=>c==='confirm_unbounded_spend'?'token':dispatch.promise);const handle=createSpendConfirmationHandler({invoke,ask:async()=>true,projectRoot:()=>'/fixture',status,onProgress:progress});const pending=handle(payload());await flush();await vi.advanceTimersByTimeAsync(500);expect(progress).toHaveBeenCalledTimes(1);dispatch.resolve('saved');await pending;expect(status.mock.calls.at(-1)?.[0]).toBe('Media generation submitted: saved');expect(invoke).toHaveBeenCalledTimes(2);
});
it('deduplicates concurrent requests without discarding intentional later requests',async()=>{
 const ask=deferred<boolean>(),invoke=vi.fn(async(c:string)=>c==='confirm_unbounded_spend'?'token':'saved'),status=vi.fn();const handle=createSpendConfirmationHandler({invoke,ask:()=>ask.promise,projectRoot:()=>'/fixture',status});const first=handle(payload());await flush();await handle(payload());expect(status.mock.calls.at(-1)?.[0]).toContain('duplicate');ask.resolve(true);await first;expect(invoke).toHaveBeenCalledTimes(2);await handle(payload());expect(invoke).toHaveBeenCalledTimes(4);
});
