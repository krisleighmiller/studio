import type { CharacterRecord } from "../../workflowTypes";
import type { CharacterIntent } from "./surfaceIntents";
import { getSurfaceHost } from "./session";
import { actionButton, block } from "./shared";
import { orderedReferencePicker } from "../orderedReferencePicker";

/** Wardrobe is selected explicitly; existing plates remain visible across scenes. */
export function renderCharacterWardrobe(character: CharacterRecord, emit: (intent: CharacterIntent) => void): HTMLElement {
  const host = getSurfaceHost();
  const state = host.state;
  const section = document.createElement("section");
  section.dataset.wardrobeCharacter = character.id;
  const heading = document.createElement("h4");
  heading.textContent = "Character wardrobe";
  const prompt = document.createElement("textarea");
  prompt.setAttribute("aria-label", "Wardrobe description");
  prompt.placeholder = "Describe an outfit";
  const references = orderedReferencePicker((state?.assets ?? [])
    .filter(asset => asset.status !== "rejected" && /\.(png|jpe?g|webp|gif)$/i.test(asset.path))
    .map(asset => ({ id: asset.id, label: asset.path })));
  const generate = actionButton("Generate wardrobe image", () => emit({ type: "generate-character-wardrobe", characterKey: character.id, prompt: prompt.value, referenceAssetIds: references.selected() }));
  const attach = actionButton("Attach wardrobe image", () => emit({ type: "attach-character-wardrobe", characterKey: character.id }));
  generate.disabled = attach.disabled = Boolean(host.busy);
  section.append(heading, block("workflow-field-hint", "Describe or attach an outfit. Reference images are optional and only included when selected. Generation requires approval and may incur charges."), prompt, references.element, generate, attach);
  const outfits = (state?.assets ?? []).filter(a => a.role === "wardrobe_reference" && a.character_id === character.id && a.status !== "rejected");
  for (const outfit of outfits) {
    const figure = document.createElement("figure");
    figure.className = "scene-look-alt";
    const img = document.createElement("img");
    img.dataset.lookPath = host.resolvePath(outfit.path) ?? outfit.path;
    img.alt = `${character.name} wardrobe image`;
    const caption = document.createElement("figcaption");
    caption.textContent = outfit.path;
    figure.append(img, caption, actionButton("Open", () => host.hooks?.onOpenPath(host.resolvePath(outfit.path) ?? outfit.path)));
    section.append(figure);
  }
  section.append(block("workflow-field-hint", "Legacy scene wardrobe plates: choose a scene and its approved wardrobe to build missing views."));
  const select = document.createElement("select");
  select.setAttribute("aria-label", `Approved wardrobe for ${character.name}`);
  const addOption = (label: string, value: string) => {
    const option = document.createElement("option");
    option.textContent = label;
    option.value = value;
    select.append(option);
  };
  addOption("Choose a scene and approved wardrobe", "");
  for (const scene of state?.scenes ?? []) {
    for (const slot of scene.look_board?.slots ?? []) {
      if (slot.role !== "wardrobe_reference") continue;
      const id = slot.approved_asset_id ?? [...(slot.asset_ids ?? [])].reverse().find(id => state?.assets?.some(a => a.id === id && a.status === "approved" && a.role === "wardrobe_reference"));
      const asset = state?.assets?.find(a => a.id === id && a.status === "approved" && a.role === "wardrobe_reference");
      if (asset) addOption(`Scene ${scene.id} · ${scene.title} · ${slot.title || "Wardrobe"} · ${asset.path}`, `${scene.id}|${asset.id}`);
    }
  }
  if (select.options.length === 1) {
    section.append(block("workflow-field-hint", "No approved scene wardrobe is available. Approve a Wardrobe image in Reference Images first."));
  }
  const build = actionButton("Build wardrobe plates", () => {
    const separator = select.value.indexOf("|");
    if (separator < 0) return;
    emit({ type: "build-wardrobe-plates", characterKey: character.id, sceneId: Number(select.value.slice(0, separator)), wardrobeAssetId: select.value.slice(separator + 1) });
  });
  build.disabled = true;
  select.addEventListener("change", () => { build.disabled = !select.value || Boolean(host.busy); });
  section.append(select, build);
  const plates = (state?.assets ?? []).filter(a => a.role === "wardrobe_plate" && a.character_id === character.id && a.status !== "rejected");
  if (!plates.length) section.append(block("workflow-field-hint", "No wardrobe plates for this character yet."));
  for (const plate of plates) {
    const figure = document.createElement("figure");
    figure.className = "scene-look-alt";
    const img = document.createElement("img");
    img.dataset.lookPath = host.resolvePath(plate.path) ?? plate.path;
    img.alt = `${character.name} wardrobe plate`;
    const caption = document.createElement("figcaption");
    caption.textContent = `Scene ${plate.scene_id ?? "?"} · ${(plate.notes ?? "plate").replace(/_/g, " ")} · ${plate.path}`;
    figure.append(img, caption, actionButton("Open", () => host.hooks?.onOpenPath(host.resolvePath(plate.path) ?? plate.path)), actionButton("Delete", () => emit({ type: "delete-wardrobe-plate", characterKey: character.id, assetId: plate.id })));
    section.append(figure);
  }
  return section;
}
