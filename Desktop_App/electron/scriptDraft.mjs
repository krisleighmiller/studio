/** Native-main-only orchestration. Renderer never receives or approves a snapshot id. */
export async function generateScriptDraft(invoke, ask, current, args) {
  const prepared = await invoke("prepare_script_draft", args);
  let approved = false;
  try {
    if (current()) approved = await ask(prepared.message, { title: "Generate script from notes", kind: "warning" });
  } catch (error) {
    await invoke("finish_script_draft", { id: prepared.id, approved: false });
    throw error;
  }
  const result = await invoke("finish_script_draft", { id: prepared.id, approved: approved === true && current() });
  if (!current()) return { cancelled: true };
  return result;
}
