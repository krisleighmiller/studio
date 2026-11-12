import type { SceneRecord } from "../../workflowTypes";
import { actionButton, block, presentSurface } from "./shared";
import { getSurfaceHost } from "./session";
import { buildScriptModel } from "./surfaceModels";

/** Saved beats with an explicit route to the registered source in Files. */
function paintScriptStage(scene: SceneRecord, generate?: () => void, busy = false): HTMLElement {
  const card = document.createElement("article");
  card.className = "workflow-script";
  const source = scene.source_path;
  if (source && getSurfaceHost()?.state?.scripts?.some(script => script.path === source)) {
    const edit = actionButton("Edit script", () => {
      const host = getSurfaceHost();
      const path = host.resolvePath(source);
      if (path) host.hooks?.onOpenPath(path);
    });
    edit.title = `Edit ${source} in Files. Save (Ctrl+S) updates Studio. Imported originals are independent copies.`;
    const details = actionButton("Generate details", () => generate?.());
    details.disabled = busy || !generate;
    details.title = "Draft missing scene intent and look suggestions for this saved script. Existing details are preserved; confirm the batch once.";
    card.append(edit, details, block("muted", "Generate missing scene intent and look suggestions for this script. Existing details are kept; review the batch before sending."), block("muted", `Source: ${source} · Save in Files to update Studio.`));
  }
  const bits: [string, string][] = [];
  if (scene.script_dialogue_review) {
    const format = (lines: { speaker: string; text: string }[]) => lines.map(line => `${line.speaker}: ${line.text}`).join("\n") || "(none)";
    card.append(block("workflow-banner warn", `Script dialogue revised in ${source ?? "saved script"}. Previous:\n${format(scene.script_dialogue_review.previous)}\nCurrent:\n${format(scene.script_dialogue_review.current)}\nCut sketches and production-note cues were retained. Review each cut in Sketches; no dialogue was automatically copied.`));
  };
  if (scene.location) bits.push(["Location", scene.location]);
  for (const line of scene.action ?? []) bits.push(["Action", line]);
  for (const line of scene.sound ?? []) bits.push(["Sound", line]);
  for (const line of scene.dialogue ?? []) bits.push([line.speaker, line.text]);
  for (const line of scene.transitions ?? []) bits.push(["Transition", line]);
  if (bits.length === 0 && scene.source_excerpt) {
    const pre = document.createElement("pre");
    pre.className = "workflow-excerpt";
    pre.textContent = scene.source_excerpt;
    card.append(pre);
    return card;
  }
  if (bits.length === 0) {
    card.append(block("empty-state", "No script beats found for this scene."));
    return card;
  }
  for (const [label, text] of bits) {
    const row = document.createElement("p");
    row.innerHTML = `<strong></strong> <span></span>`;
    row.querySelector("strong")!.textContent = `${label}:`;
    row.querySelector("span")!.textContent = text;
    card.append(row);
  }
  return card;
}

export function renderScriptStage(scene: SceneRecord, generate?: () => void, busy = false): HTMLElement {
  return presentSurface(paintScriptStage(scene, generate, busy), buildScriptModel({ scene }), false);
}
