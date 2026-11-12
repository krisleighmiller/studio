import native from './sb10-native-resume.json';
const config = await fetch('/sb23-config').then(r=>r.json());
window.calls = [];
window.workbench = {
  invoke: async (cmd,args) => {
    window.calls.push({cmd,args});
    if(cmd==='load_settings')return {};
    if(cmd==='list_dir')return [];
    if(cmd==='workflow_import')return structuredClone(native.initial);
    if(cmd==='workflow_prepare_assemble')return {...structuredClone(native.initial),pendingAction:'Exact disposable assembly [[workbench_approve:SB23UI]]'};
    if(cmd==='director_cancel')return 'Cancelled';
    if(cmd==='classify_file')return 'text';
    if(cmd==='read_text_file'){
      const r=await fetch('/sb23-read?path='+encodeURIComponent(args.path));
      if(!r.ok)throw Error('Fixture path refused');return r.text();
    }
    throw Error('Forbidden native/provider command: '+cmd);
  },
  openProject:async()=>config.project,
  openDialog:async()=>config.history,
  rememberRecentProject:async()=>{},onMenu:()=>()=>{},onDirectorProgress:()=>()=>{},onSpendConfirmationRequired:()=>()=>{},
};
await import('/src/main.ts');
window.prepare = async()=>{
  const {getSurfaceHost}=await import('/src/studio/surfaces/session.ts');
  return getSurfaceHost().run('Assemble UI fixture','workflow_prepare_assemble',{sceneId:1});
};
window.config=config;
window.fixtureReady=true;
