import type { ProjectState } from "../workflowTypes";
import "./generationPrompt.css";

export type PromptSpec = { title: string; prompt: string; context: string; field: "brief" | "prompt"; direct?: boolean };
const briefTitles: Record<string, string> = {
  workflow_draft_sketches: "Draft cut sketches",
  workflow_draft_start_frame_prompt: "Draft cut start-frame prompt",
  workflow_sync_production_notes: "Build production video prompt",
  workflow_edit_start_frame: "Edit cut start frame",
};

/** These are editable creative inputs, not a substitute for the host's exact operation card. */
export function generationPromptSpec(cmd: string, args: Record<string, unknown>, state: ProjectState | null): PromptSpec | null {
  const scene = state?.scenes.find(s => s.id === args.sceneId);
  const cut = scene?.cuts.find(c => c.id.cut === args.cut);
  const slot = scene?.look_board?.slots?.find(s => s.id === args.slotId);
  const title = briefTitles[cmd] ?? ({
    workflow_generate_look: "Generate scene look image",
    workflow_generate_start_frame: "Generate cut start frame",
    workflow_prepare_render: "Prepare cut render",
  } as Record<string, string>)[cmd];
  if (!title) return null;
  const source = cut?.visual_source;
  const ids = new Set([source?.asset_id, cut?.approved_start_frame, cut?.selected_performance_asset_id]);
  const refs = state?.assets?.filter(a => ids.has(a.id) || (a.scene_id === scene?.id && (
    (slot && slot.asset_ids?.includes(a.id)) || (!cut && !slot) || cut?.sketch?.reference_roles?.includes(a.role)
  ))) ?? [];
  const context = [
    scene ? `Scene ${scene.id}: ${scene.title}\nScript source: ${scene.source_path ?? "not recorded"}\nMaximum cuts: ${scene.max_cuts ?? "native default"}` : "Project-wide request",
    `Reference/input context (the host confirms actual uploads):\n${refs.map(a => `${a.id} · ${a.role} · ${a.path}`).join("\n") || "No registered references in this context."}`,
    scene?.intent ? `Scene intent: ${JSON.stringify(scene.intent)}` : "",
    cut ? `Cut ${cut.id.cut} (${cut.stable_id})\nSketch: ${JSON.stringify(cut.sketch ?? {})}\nDialogue: ${JSON.stringify(cut.dialogue ?? [])}\nVisual source: ${JSON.stringify(source ?? null)}\nProduction note: ${cut.production_note_path ?? "none"}\nSpeech mode: ${cut.speech_mode ?? "none"}` : "",
    slot ? `Look slot: ${slot.title} (${slot.role})\nNegative prompt: ${args.negativePrompt ?? slot.negative_prompt ?? "none"}` : "",

    cmd === "workflow_prepare_render" || cmd === "workflow_generate_start_frame" ? "Prepare saves a changed prompt through the existing project prompt saver before requesting a fresh host approval. Cancel changes nothing." : "",
    args.scriptSource ? "Preparing this request sends the chat-model draft request immediately; it may incur provider charges. Cancel sends nothing. Each scene detail requires its own confirmation." : "The host validates saved inputs and shows exact executable inputs on its separate approval card. Preparation is not permission to generate.",
  ].filter(Boolean).join("\n\n");
  const field = briefTitles[cmd] ? "brief" : "prompt";
  const prompt = String(args[field] ?? (cmd === "workflow_generate_start_frame" ? cut?.start_frame_prompt
    : cmd === "workflow_prepare_render" ? cut?.prompt : slot?.prompt) ?? (briefTitles[cmd]
      ? `${title} for Scene ${scene?.id ?? "selected"}${cut ? ` Cut ${cut.id.cut}` : ""}. Use the saved script, planning and selected reference context; preserve existing creative constraints.` : ""));
  return { title, prompt, context, field, direct: Boolean(args.scriptSource) };
}

let cancelActive: (() => void) | null = null;
export function cancelGenerationPrompt(): void { cancelActive?.(); }
export function generationPromptOpen(): boolean { return cancelActive !== null; }

/** Native Chromium modal: no host request exists until Prepare is explicitly chosen. */
export function editGenerationPrompt(spec: { title: string; prompt: string; context: string; direct?: boolean }): Promise<string | null> {
  if (cancelActive) return Promise.resolve(null);
  return new Promise(resolve => {
    const previous = document.activeElement as HTMLElement | null;
    const dialog = document.createElement("dialog");
    dialog.className = "studio-generation-prompt";
    dialog.dataset.generationPrompt = "";
    dialog.setAttribute("aria-labelledby", "studio-generation-prompt-title");
    const title = document.createElement("h2"); title.id = "studio-generation-prompt-title"; title.textContent = spec.title;
    const help = document.createElement("p"); help.textContent = spec.direct ? "Prepare request sends this chat-model request now and may incur provider charges. Cancel sends nothing. Review each request separately." : "Edit before preparing. Cancel sends nothing. Each submission prepares a fresh request; earlier approval IDs are never reused for edited text. Media execution still needs separate authorization.";
    const context = document.createElement("pre"); context.dataset.promptContext = ""; context.textContent = spec.context;
    const form = document.createElement("form");
    const label = document.createElement("label"); label.textContent = "Task prompt / creative instructions";
    const text = document.createElement("textarea"); text.name = "prompt"; text.rows = 9; text.value = spec.prompt; text.autofocus = true;
    label.append(text);
    const cancel = document.createElement("button"); cancel.type = "button"; cancel.textContent = "Cancel";
    const submit = document.createElement("button"); submit.type = "submit"; submit.textContent = "Prepare request";
    let settled = false;
    const finish = (value: string | null) => {
      if (settled) return; settled = true; cancelActive = null;
      dialog.close(); dialog.remove();
      if (previous?.isConnected) previous.focus();
      resolve(value);
    };
    cancelActive = () => finish(null);
    cancel.onclick = () => finish(null);
    dialog.addEventListener("cancel", event => { event.preventDefault(); finish(null); });
    dialog.addEventListener("close", () => finish(null));
    form.addEventListener("submit", event => { event.preventDefault(); submit.disabled = true; finish(text.value); });
    form.append(label, cancel, submit); dialog.append(title, help, context, form); document.body.append(dialog);
    dialog.showModal(); text.focus();
  });
}
