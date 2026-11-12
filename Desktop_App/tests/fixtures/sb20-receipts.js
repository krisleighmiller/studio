import native from './sb10-native-resume.json';
import * as desk from '/src/workflowDesk.ts';
import '/src/styles.css';
let project = '/disposable/sb20/A';
window.calls = []; window.handoffs = [];
const waiting = [];
window.workbench = {
  invoke: async (cmd, args) => {
    window.calls.push({cmd, args});
    if (cmd === 'workflow_import') return structuredClone(native.initial);
    if (cmd === 'workflow_generate_script' || cmd === 'workflow_prepare_render') return new Promise((resolve, reject) => waiting.push({resolve, reject, project: args.projectRoot}));
    throw Error('Forbidden command: '+cmd);
  },
};
const root = document.querySelector('#desk');
desk.bindWorkflowDesk({root, projectRoot: () => project, chatModel: () => ({apiId:'fake',modelId:'fake'}), onOpenPath: () => {}, onAddToChat: () => {}, onOpenVoice: () => {}, onComposeChat: () => {}, onSelectionChange: () => {}, onStatus: () => {}, onPendingPlan: text => window.handoffs.push(text)});
window.replaceProject = async path => { desk.clearWorkflowDesk(); project = path; await desk.loadWorkflowDesk(path); };
window.finishRequest = (kind, owner = project) => {
  const i = waiting.findIndex(r => r.project === owner); if (i < 0) throw Error('No pending fake host');
  const [r] = waiting.splice(i,1);
  if (kind === 'error') r.reject(Error('Fixture host refused this exact request'));
  else if (kind === 'cancel') r.resolve({cancelled: true});
  else if (kind === 'pending') r.resolve(structuredClone(native.prepared));
  else r.resolve({script:'Scene 1 — Arrival\nA local returned draft.'});
};
await desk.loadWorkflowDesk(project);
window.fixtureReady = true;
