import type { ScriptDraft, ScriptDraftText } from "./surfaces/scriptDraftForm";

// One combined, project-keyed local snapshot. Never scanned/registered as a script.
// Synchronous writes acknowledge each edit before a renderer can close; no unload race.
const prefix = "studio.local-script-draft.v1:";
const unsaved = new Map<string, ScriptDraftText>();
const unreadable = new Set<string>();
const text = (draft: ScriptDraftText): ScriptDraftText => ({ title: draft.title, notes: draft.notes, script: draft.script });
const failure = "Draft recovery save failed. Latest text is only in memory; keep Studio open and retry, or copy it before quitting.";

export function recoverScriptDraft(root: string): ScriptDraft | null {
  const retained = unsaved.get(root);
  if (retained) return { ...retained, recoveryStatus: failure, recoveryFailed: true };
  try {
    const raw = localStorage.getItem(prefix + root);
    if (raw === null) return null;
    const record = JSON.parse(raw);
    if (record.version !== 1 || record.projectRoot !== root ||
        ![record.title, record.notes, record.script].every(value => typeof value === "string")) {
      throw new Error("Invalid recovery record");
    }
    unreadable.delete(root);
    return { ...text(record), recoveryStatus: "Recovered local draft. Not registered as a project script." };
  } catch {
    unreadable.add(root);
    return { title: "", notes: "", script: "", recoveryFailed: true,
      recoveryStatus: "Could not read local draft. Stored recovery was not changed. Copy any text before quitting; retry or explicitly discard the unreadable draft." };
  }
}

export function persistScriptDraft(root: string, draft: ScriptDraft): void {
  unsaved.set(root, text(draft));
  try {
    if (unreadable.has(root)) throw new Error("Unreadable recovery retained");
    localStorage.setItem(prefix + root, JSON.stringify({ version: 1, projectRoot: root, ...text(draft) }));
    unsaved.delete(root);
    draft.recoveryFailed = false;
    draft.recoveryStatus = "Recovery copy updated locally. No project file was written. Cancel keeps this recovery copy.";
  } catch {
    draft.recoveryFailed = true;
    draft.recoveryStatus = failure;
  }
}

/** Remove only after explicit discard; saving a script does not erase unsaved notes. */
export function discardScriptDraft(root: string, draft: ScriptDraft): boolean {
  try {
    localStorage.removeItem(prefix + root);
    unsaved.delete(root);
    unreadable.delete(root);
    return true;
  } catch {
    draft.recoveryFailed = true;
    draft.recoveryStatus = "Could not clear local draft recovery. Text retained; retry discard when local storage is available.";
    return false;
  }
}
