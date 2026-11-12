import { block, sketchTextarea, sketchTextField } from "./shared";
import type { ScriptDraftIntent } from "./surfaceIntents";

export type ScriptDraftText = { notes: string; title: string; script: string };
export type ScriptDraft = ScriptDraftText & { validationError?: string; busy?: boolean; busyAction?: ScriptDraftIntent["type"]; recoveryStatus?: string; recoveryFailed?: boolean };

export function updateDraftRecoveryStatus(form: HTMLElement, draft: ScriptDraft): void {
  const status = form.querySelector<HTMLElement>("[data-draft-recovery]");
  if (!status) return;
  status.textContent = draft.recoveryStatus ?? "Local recovery starts when you edit. Only Save script registers a script.";
  status.setAttribute("role", draft.recoveryFailed ? "alert" : "status");
}

/** Present local authoring controls; all effects and session updates belong to the caller. */
export function renderScriptDraftForm(draft: ScriptDraft, emit: (intent: ScriptDraftIntent) => void, projectRoot?: string): HTMLElement {
  const form = document.createElement("form");
  form.className = "workflow-script-draft";
  form.setAttribute("aria-label", "Local script draft");
  form.setAttribute("aria-busy", String(Boolean(draft.busy)));
  form.append(block("workflow-field-hint", "Local authoring: Load notes reads a file only. Generate script from notes asks for native approval to send Title and Notes to your selected text model. Edit the result, then explicitly Save script."));
  const title = sketchTextField(form, "Title", draft.title);
  const notes = sketchTextarea(form, "Notes", draft.notes, 5);
  const script = sketchTextarea(form, "Script", draft.script, 16);
  title.name = "title";
  notes.name = "notes";
  script.name = "script";
  const feedback = block("workflow-field-hint", draft.validationError ?? "");
  feedback.setAttribute("role", "alert");
  feedback.hidden = !draft.validationError;
  feedback.addEventListener("contextmenu", () => {
    const selection = window.getSelection();
    if (!selection || !feedback.textContent) return;
    const range = document.createRange();
    range.selectNodeContents(feedback);
    selection.removeAllRanges();
    selection.addRange(range);
  });
  form.append(feedback);
  const recovery = block("workflow-field-hint", "");
  recovery.dataset.draftRecovery = "";
  recovery.style.marginTop = "8px";
  form.append(recovery);
  const notesFileContract = block("workflow-field-hint", `Autosave is local recovery only for ${projectRoot ?? "the current project"}. Save notes writes <project root>/Notes.md. Reload saved notes reads that file.`);
  notesFileContract.dataset.notesFileContract = "";
  notesFileContract.style.marginTop = "8px";
  form.append(notesFileContract);
  updateDraftRecoveryStatus(form, draft);

  for (const input of [title, notes, script]) {
    input.disabled = Boolean(draft.busy);
    input.addEventListener("input", () => {
      if (draft.busy) return;
      emit({ type: "change", draft: { notes: notes.value, title: title.value, script: script.value } });
    });
  }
  for (const [label, type] of [["Load notes", "load-notes"], ["Save notes", "save-notes"], ["Reload saved notes", "reload-notes"], ["Generate script from notes", "generate"], ["Save script", "save"], ["Retry draft recovery", "retry-recovery"], ["Discard draft", "discard"], ["Cancel", "cancel"]] as const) {
    // Unlike actionButton, this renderer must not read the surface host singleton.
    const button = document.createElement("button");
    button.type = "button";
    button.className = "ghost-button";
    const working = Boolean(draft.busy && draft.busyAction === type);
    button.textContent = working && type === "generate" ? "Generating script…" : label;
    button.disabled = Boolean(draft.busy);
    button.classList.toggle("is-working", working);
    button.setAttribute("aria-busy", String(working));
    button.addEventListener("click", () => {
      if (button.disabled) return;
      if (type === "save" || type === "generate" || type === "save-notes") emit({ type, draft: { notes: notes.value, title: title.value, script: script.value } });
      else emit({ type });
    });
    form.append(button);
  }
  form.addEventListener("submit", event => event.preventDefault());
  return form;
}
