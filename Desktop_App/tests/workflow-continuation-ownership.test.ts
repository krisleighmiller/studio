import { expect, it, vi } from 'vitest';
import ts from 'typescript';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
const source=readFileSync('src/main.ts','utf8');
const ast=ts.createSourceFile('main.ts',source,ts.ScriptTarget.Latest,true);
const declaration=ast.statements.find(n=>ts.isFunctionDeclaration(n)&&n.name?.text==='publishMediaContinuation');
if(!declaration)throw Error('Missing production continuation publisher');
const compiled=ts.transpileModule(declaration.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText;
function fixture(){
 const a={id:'A',messages:[] as any[]},b={id:'B',messages:[] as any[]};
 const context={projectRoot:'/original',workflowShotStableId:'mom'};
 const owner={tab:a,context,serial:7,epoch:4,root:'/original'};
 const s:any={chatTabs:[a,b],deferredMediaOwners:new Map([['owner',owner]]),directorPresentationSerial:7,projectEpoch:4,projectRoot:'/original',projectTransitionInFlight:false,directorStopRequested:false,uid:()=> 'resume-message',renderChat:vi.fn(),activeChat:()=>{throw Error('Active chat is not ownership');}};
 vm.createContext(s);vm.runInContext(compiled,s);
 const result={mediaResult:'saved',continuation:{workflowContinuation:true,continueSuggested:false,reply:'Resume original Mom shot. [[workbench_approve:realfixturetoken]]'}};
 return {a,b,s,result,publish:()=>s.publishMediaContinuation(result,'owner')};
}
it('publishes once into the original owner and preserves its scope, not active-chat selection',()=>{
 const {a,b,s,publish}=fixture();publish();publish();
 expect(a.messages).toHaveLength(1);expect(b.messages).toHaveLength(0);
 expect(a.messages[0].context).toEqual({projectRoot:'/original',workflowShotStableId:'mom'});
 expect(s.renderChat).toHaveBeenCalledTimes(1);
});
it.each(['closed-owner','newer-operation','same-root-new-epoch','changed-root','transition','stop'])('drops continuation after %s without creating an actionable card',kind=>{
 const {a,b,s,publish}=fixture();
 if(kind==='closed-owner')s.chatTabs=[b];
 if(kind==='newer-operation')s.directorPresentationSerial++;
 if(kind==='same-root-new-epoch')s.projectEpoch++;
 if(kind==='changed-root')s.projectRoot='/replacement';
 if(kind==='transition')s.projectTransitionInFlight=true;
 if(kind==='stop')s.directorStopRequested=true;
 publish();expect(a.messages).toHaveLength(0);expect(b.messages).toHaveLength(0);expect(s.renderChat).not.toHaveBeenCalled();
});
it('rejects missing ownership and malformed or planner-continuing payloads',()=>{
 const {a,s,result}=fixture();s.publishMediaContinuation(result,undefined);
 s.publishMediaContinuation({continuation:{reply:'forged'}},'owner');
 s.publishMediaContinuation({continuation:{...result.continuation,continueSuggested:true}},'owner');
 expect(a.messages).toHaveLength(0);
});
