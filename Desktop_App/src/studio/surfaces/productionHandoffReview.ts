import "../../studio/generationPrompt.css";

export type HandoffPreview = {
  path: string;
  oldCues: { speaker: string; text: string }[];
  newCues: { speaker: string; text: string }[];
  oldTiming: { speech_duration_seconds?: number; render_duration_seconds?: number } | null;
  newTiming: { speech_duration_seconds?: number; render_duration_seconds?: number } | null;
  oldScreenSeconds: number;
  newScreenSeconds: number;
  oldReferences: { path: string; role: string }[];
  newReferences: { path: string; role: string }[];
  prompt: string;
  negativePrompt: string | null;
  changed: boolean;
  revision: string;
  oldDocument?: string;
  newDocument?: string;
  oldMaster?: string | null;
  newMaster?: string | null;
};

/** Preview only. The host validates the revision again before any write. */
export function reviewProductionHandoff(preview: HandoffPreview, scene: number, cut: number): Promise<boolean> {
  return new Promise(resolve => {
    const previous = document.activeElement as HTMLElement | null;
    const dialog = document.createElement("dialog");
    dialog.className = "studio-generation-prompt";
    dialog.dataset.productionHandoffReview = "";
    const title = document.createElement("h2");
    title.textContent = `Refresh Scene ${scene} Cut ${cut} production note?`;
    const detail = document.createElement("pre");
    detail.dataset.handoffComparison = "";
    const cues = (lines: HandoffPreview["oldCues"]) => lines.map(line => `${line.speaker}: ${line.text}`).join("\n") || "(none)";
    const references = (refs: HandoffPreview["oldReferences"]) => refs.map(ref => `${ref.role}: ${ref.path}`).join("\n") || "(none)";
    detail.textContent = `Canonical note: ${preview.path}\n\nCURRENT CUES\n${cues(preview.oldCues)}\n\nPROPOSED CUES\n${cues(preview.newCues)}\n\nCURRENT estimated speech / screen / render: ${preview.oldTiming?.speech_duration_seconds ?? "unknown"} / ${preview.oldScreenSeconds} / ${preview.oldTiming?.render_duration_seconds ?? "unknown"} s\nPROPOSED estimated speech / screen / render: ${preview.newTiming?.speech_duration_seconds ?? "unknown"} / ${preview.newScreenSeconds} / ${preview.newTiming?.render_duration_seconds ?? "unknown"} s\n\nCURRENT REFERENCES\n${references(preview.oldReferences)}\n\nPROPOSED REFERENCES\n${references(preview.newReferences)}\n\nRETAINED authored prompt:\n${preview.prompt}\n\nRETAINED negative prompt:\n${preview.negativePrompt ?? "(none)"}\n\nThis replaces the host-owned cues, timing and source references in this one note. It does not rewrite the authored prompt or another scene. Cancel leaves the note unchanged.`;
    const exact = document.createElement("details");
    const exactLabel = document.createElement("summary");
    exactLabel.textContent = "Inspect full note and production master before/after (including any freeform text)";
    const exactText = document.createElement("pre");
    exactText.textContent = `CURRENT SIDECAR\n${preview.oldDocument ?? "(not available)"}\n\nPROPOSED SIDECAR\n${preview.newDocument ?? "(not available)"}\n\nCURRENT MASTER\n${preview.oldMaster ?? "(none)"}\n\nPROPOSED MASTER\n${preview.newMaster ?? "(none)"}`;
    exact.append(exactLabel, exactText);
    const cancel = document.createElement("button"); cancel.type = "button"; cancel.textContent = "Keep current note";
    const apply = document.createElement("button"); apply.type = "button"; apply.textContent = "Refresh this note";
    apply.disabled = !preview.changed;
    let settled = false;
    const finish = (accepted: boolean) => {
      if (settled) return;
      settled = true;
      dialog.close(); dialog.remove();
      if (previous?.isConnected) previous.focus();
      resolve(accepted);
    };
    cancel.onclick = () => finish(false);
    apply.onclick = () => finish(true);
    dialog.addEventListener("cancel", event => { event.preventDefault(); finish(false); });
    dialog.addEventListener("close", () => finish(false));
    dialog.append(title, detail, exact, cancel, apply); document.body.append(dialog);
    dialog.showModal(); cancel.focus();
  });
}
