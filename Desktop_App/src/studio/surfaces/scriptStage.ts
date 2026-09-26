import type { SceneRecord } from "../../workflowTypes";
import { block, presentSurface } from "./shared";
import { buildScriptModel } from "./surfaceModels";

/** Phase 6 script surface: read-only script beats. No intents (nothing to do here). */
function paintScriptStage(scene: SceneRecord): HTMLElement {
  const card = document.createElement("article");
  card.className = "workflow-script";
  const bits: [string, string][] = [];
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

export function renderScriptStage(scene: SceneRecord): HTMLElement {
  return presentSurface(paintScriptStage(scene), buildScriptModel({ scene }), false);
}
