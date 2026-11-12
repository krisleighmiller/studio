import fixture from './sc05-import-result.json';
import native from './sb16-native-reopen.json';
const empty = structuredClone(fixture); empty.state.scenes = []; empty.report.scenes = [];
let current = empty;
window.calls = [];
window.loadPopulated = async () => { current = fixture; const desk = await import('/src/workflowDesk.ts'); await desk.refreshWorkflowDesk(); };
window.workbench = {
  invoke: async (command, args) => {
    window.calls.push({ command, args });
    if (command === 'load_settings') return {};
    if (command === 'list_dir') return [{ name: 'local.txt', path: '/disposable/sb19/local.txt', isDir: false }];
    if (command === 'workflow_import') return structuredClone(current);
    if (command === 'classify_file') return 'text';
    if (command === 'read_text_file') return 'Editor local text';
    throw Error('Forbidden native/provider call: ' + command);
  },
  openProject: async () => '/disposable/sb19', openDialog: async () => '/disposable/sb19/local.txt',
  rememberRecentProject: async () => {}, onMenu: () => () => {}, onDirectorProgress: () => () => {}, onSpendConfirmationRequired: () => () => {},
};
await import('/src/main.ts');
window.mountPromptModal = async () => {
  const { renderProductionNoteCard } = await import('/src/studio/surfaces/notesStage.ts');
  const dialog = document.createElement('dialog'); dialog.id = 'clipboard-modal';
  dialog.append(renderProductionNoteCard(native.approved.state.scenes[0].cuts[0], () => { throw Error('No prompt action permitted'); }));
  document.body.append(dialog); dialog.showModal();
};
window.fixtureReady = true;
