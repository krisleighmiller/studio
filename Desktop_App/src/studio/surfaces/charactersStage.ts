import { type AssetRecord, type CharacterProfile, type CharacterRecord } from "../../workflowTypes";
import { getSurfaceHost, characterDrafts, isNewCharacterOpen, setNewCharacterOpen } from "./session";
import { actionButton, block, findAsset, missingPanel, presentSurface, queueLookThumbs, sketchTextarea, sketchTextField } from "./shared";
import { nullable, splitList } from "./surfaceSelectors";
import { buildCharactersModel } from "./surfaceModels";
import type { CharacterIntent, CharacterReferenceImage, CharacterReferenceIntent } from "./surfaceIntents";
import { renderCharacterWardrobe } from "./characterWardrobe";

function paintCharactersStage(emit: (intent: CharacterIntent) => void): HTMLElement {
  const list = document.createElement("div");
  list.className = "workflow-characters";
  const characters: CharacterRecord[] = getSurfaceHost().state?.characters ?? [];
  const toolbar = document.createElement("div");
  toolbar.className = "workflow-inspector-actions";
  toolbar.append(
    actionButton(isNewCharacterOpen() ? "Cancel new character" : "New character", () => {
      setNewCharacterOpen(!isNewCharacterOpen());
      if (!isNewCharacterOpen()) characterDrafts.delete("__new__");
      getSurfaceHost().rerender();
    }),
  );
  list.append(toolbar);
  if (isNewCharacterOpen()) list.append(renderCharacterEditor(null, emit));
  if (!characters.length && !isNewCharacterOpen()) {
    list.append(
      missingPanel(
        "No characters discovered.",
        "Create a profile here. Identity model sheets and voice assets will remain in that character’s folder.",
      ),
    );
  }
  for (const character of characters) {
    list.append(renderCharacterEditor(character, emit));
  }
  queueLookThumbs(list);
  return list;
}

function renderCharacterEditor(
  character: CharacterRecord | null,
  emit: (intent: CharacterIntent) => void,
): HTMLElement {
  const key = character?.id ?? "__new__";
  const busyControl = getSurfaceHost().busyControl;
  const draft = characterDraft(key, character);
  const card = document.createElement("article");
  card.className = "workflow-character-card workflow-character-editor";
  const head = document.createElement("header");
  const heading = document.createElement("h3");
  heading.textContent = character?.name ?? "New character";
  const status = document.createElement("span");
  status.className = "workflow-look-status";
  status.textContent = character
    ? `${character.identity_ready ? "identity ready" : "no identity"} · ${
        character.voice_ready ? "voice ready" : "no voice"
      }`
    : "Profile draft";
  head.append(heading, status);
  card.append(head);

  const fields = document.createElement("div");
  fields.className = "workflow-character-fields";
  const name = sketchTextField(fields, "Character name", draft.name);
  name.readOnly = Boolean(character);
  name.addEventListener("input", () => {
    draft.name = name.value;
  });
  const description = sketchTextarea(
    fields,
    "Description",
    draft.profile.description ?? "",
    3,
  );
  description.addEventListener("input", () => {
    draft.profile.description = nullable(description.value);
  });
  characterListField(
    fields,
    "Appearance",
    draft.profile.appearance ?? [],
    (values) => {
      draft.profile.appearance = values;
    },
  );
  characterListField(
    fields,
    "Default wardrobe",
    draft.profile.wardrobe ?? [],
    (values) => {
      draft.profile.wardrobe = values;
    },
  );
  characterListField(
    fields,
    "Personality",
    draft.profile.personality ?? [],
    (values) => {
      draft.profile.personality = values;
    },
  );
  characterListField(
    fields,
    "Continuity notes",
    draft.profile.continuity_notes ?? [],
    (values) => {
      draft.profile.continuity_notes = values;
    },
  );
  const identityPrompt = sketchTextarea(
    fields,
    "Identity prompt",
    draft.profile.identity_prompt ?? "",
    3,
  );
  identityPrompt.addEventListener("input", () => {
    draft.profile.identity_prompt = nullable(identityPrompt.value);
  });
  card.append(fields);

  const references = character
    ? (character.reference_asset_ids ?? []).map(findAsset).filter((asset): asset is AssetRecord => Boolean(asset))
    : (characterDrafts.get(key)?.references ?? []);
  card.append(renderCharacterReferenceControls(character ?? { key }, references, emit));

  if (character) {
    card.append(renderCharacterModelSheet(character, emit));
    card.append(renderCharacterWardrobe(character, emit));
    const voice = renderCharacterVoice(character);
    if (voice) card.append(voice);
  }

  if (character?.notes?.length) {
    const notes = document.createElement("p");
    notes.className = "workflow-field-hint";
    notes.textContent = character.notes.join(" · ");
    card.append(notes);
  }

  const actions = document.createElement("div");
  actions.className = "workflow-inspector-actions";
  actions.append(
    actionButton(character ? "Save profile" : "Create character", () => {
      emit({
        type: "save",
        characterId: character?.id ?? null,
        name: draft.name,
        profile: { ...draft.profile },
      });
    }),
  );
  if (character) {
    const directory = getSurfaceHost().resolvePath(character.dir) ?? character.dir;
    actions.append(
      actionButton(character.voice_ready ? "Edit voice" : "Create voice", () => {
        getSurfaceHost().hooks?.onOpenVoice(directory);
      }),
      workingButton(
        character.model_sheet_manifest_path || character.identity_ready ? "Update full 12-pose sheet" : "Create full 12-pose sheet",
        "Preparing model sheet…",
        busyControl === `character-model-sheet:${key}:each`,
        () => emit({ type: "create-model-sheet", characterKey: key, approvalMode: "each" }),
      ),
      workingButton(
        "Generate entire 12-pose sheet (one approval)",
        "Preparing model sheet…",
        busyControl === `character-model-sheet:${key}:whole`,
        () => emit({ type: "create-model-sheet", characterKey: key, approvalMode: "whole" }),
      ),
    );
    if (character.profile_path) {
      actions.append(
        actionButton("Open profile", () => {
          const path = getSurfaceHost().resolvePath(character.profile_path);
          if (path) getSurfaceHost().hooks?.onOpenPath(path);
        }),
      );
    }
    if (character.model_sheet_manifest_path) {
      actions.append(
        actionButton("Open model sheet manifest", () => {
          const path = getSurfaceHost().resolvePath(character.model_sheet_manifest_path);
          if (path) getSurfaceHost().hooks?.onOpenPath(path);
        }),
      );
    }
  }
  if (!character) actions.append(workingButton(
    "Create full 12-pose sheet",
    "Preparing model sheet…",
    busyControl === `character-model-sheet:${key}:each`,
    () => emit({ type: "create-model-sheet", characterKey: key, approvalMode: "each" }),
  ));
  card.append(actions);
  return card;
}

function workingButton(label: string, workingLabel: string, working: boolean, onClick: () => void): HTMLButtonElement {
  const button = actionButton(working ? workingLabel : label, onClick);
  button.classList.toggle("is-working", working);
  button.setAttribute("aria-busy", String(working));
  return button;
}

function characterDraft(
  key: string,
  character: CharacterRecord | null,
): { name: string; profile: CharacterProfile } {
  const existing = characterDrafts.get(key);
  if (existing) return existing;
  const profile = character?.profile ?? {};
  const draft = {
    name: character?.name ?? "",
    profile: {
      description: profile.description ?? null,
      appearance: [...(profile.appearance ?? [])],
      wardrobe: [...(profile.wardrobe ?? [])],
      personality: [...(profile.personality ?? [])],
      continuity_notes: [...(profile.continuity_notes ?? [])],
      identity_prompt: profile.identity_prompt ?? null,
    },
  };
  characterDrafts.set(key, draft);
  return draft;
}

function characterListField(
  parent: HTMLElement,
  title: string,
  values: string[],
  onInput: (values: string[]) => void,
): HTMLTextAreaElement {
  const input = sketchTextarea(parent, title, values.join("\n"), 2);
  input.placeholder = "One item per line";
  input.addEventListener("input", () => {
    onInput(splitList(input.value));
  });
  return input;
}

const CHARACTER_IMAGE_ROLES = new Set([
  "identity_reference",
  "body_reference",
  "expression_reference",
]);

/** Presentation only; selection effects belong to the controller. */
export function renderCharacterReferenceControls(
  characterOrDraft: CharacterRecord | { key: string },
  references: readonly CharacterReferenceImage[],
  emit: (intent: CharacterReferenceIntent) => void,
): HTMLElement {
  const key = "id" in characterOrDraft ? characterOrDraft.id : characterOrDraft.key;
  const section = document.createElement("section");
  section.className = "workflow-character-references";
  section.dataset.characterKey = key;
  const heading = document.createElement("h4");
  heading.textContent = "Reference images";
  section.append(heading, block("workflow-field-hint", "Uploaded reference images guide appearance; they are not generated identity stills."));
  section.append(actionButton("Add reference image", () => emit({
    type: "add-reference", characterKey: key, assetIds: references.map(reference => reference.id),
  })));
  const gallery = document.createElement("div");
  gallery.className = "workflow-character-reference-gallery";
  for (const reference of references) {
    const figure = document.createElement("figure");
    figure.dataset.assetId = reference.id;
    const name = reference.name ?? reference.path.split(/[\\/]/).pop() ?? reference.id;
    const image = document.createElement("img");
    image.alt = `Reference image · ${name}`;
    image.dataset.lookPath = getSurfaceHost().resolvePath(reference.path) ?? reference.path;
    const caption = document.createElement("figcaption");
    caption.textContent = name;
    const saved = "id" in characterOrDraft;
    const remove = actionButton(saved ? "Delete" : "Remove", () => emit({
      type: "remove-reference", characterKey: key, assetId: reference.id,
      assetIds: references.filter(item => item.id !== reference.id).map(item => item.id),
    }));
    remove.setAttribute("aria-label", saved
      ? `Delete saved reference image ${name} from project`
      : `Remove staged reference image ${name}`);
    figure.append(image, caption, remove);
    gallery.append(figure);
  }
  section.append(gallery);
  return section;
}

function renderCharacterModelSheet(character: CharacterRecord, emit: (intent: CharacterIntent) => void): HTMLElement {
  const section = document.createElement("section");
  section.className = "workflow-character-model-sheet";
  const heading = document.createElement("h4");
  const assets = (character.asset_ids ?? [])
    .map(findAsset)
    .filter(
      (asset): asset is AssetRecord =>
        Boolean(asset && CHARACTER_IMAGE_ROLES.has(asset.role) && asset.notes !== "uploaded reference image" && !(character.reference_asset_ids ?? []).includes(asset.id)),
    );
  heading.textContent = assets.length
    ? `Model sheet · ${assets.length} saved still${assets.length === 1 ? "" : "s"}`
    : "Model sheet";
  section.append(heading);
  if (!assets.length) {
    section.append(
      block(
        "empty-state",
        "No saved model-sheet stills. Use Create full 12-pose sheet to create the complete pack.",
      ),
    );
    return section;
  }
  const gallery = document.createElement("div");
  gallery.className = "workflow-character-model-sheet-gallery";
  for (const asset of assets) {
    const figure = document.createElement("figure");
    const image = document.createElement("img");
    image.alt = `${character.name} ${asset.notes ?? asset.role}`;
    image.dataset.lookPath = getSurfaceHost().resolvePath(asset.path) ?? asset.path;
    image.addEventListener("click", () => {
      getSurfaceHost().hooks?.onOpenPath(getSurfaceHost().resolvePath(asset.path) ?? asset.path);
    });
    const caption = document.createElement("figcaption");
    caption.textContent = asset.notes?.trim() || asset.role.replace(/_/g, " ");
    const remove = actionButton("Delete", () => emit({
      type: "delete-model-sheet-photo", characterKey: character.id, assetId: asset.id,
    }));
    remove.setAttribute("aria-label", `Delete model-sheet photo ${caption.textContent}`);
    figure.append(image, caption, remove);
    gallery.append(figure);
  }
  section.append(gallery);
  return section;
}

function renderCharacterVoice(character: CharacterRecord): HTMLElement | null {
  const assets = (character.asset_ids ?? [])
    .map(findAsset)
    .filter((asset): asset is AssetRecord => Boolean(asset));
  const sample = assets.find((asset) => asset.role === "voice_sample");
  const profile = assets.find((asset) => asset.role === "voice_profile");
  if (!sample && !profile) return null;
  const line = document.createElement("p");
  line.className = "workflow-field-hint";
  const file = sample?.path.split("/").pop() || profile?.path.split("/").pop() || "voice";
  const detail = profile?.notes?.trim();
  line.textContent = detail ? `Voice · ${file} · ${detail}` : `Voice · ${file}`;
  return line;
}

export function renderCharactersStage(onIntent: (intent: CharacterIntent) => void): HTMLElement {
  const model = buildCharactersModel({
    characters: getSurfaceHost().state?.characters ?? [],
    creating: isNewCharacterOpen(),
    busy: getSurfaceHost().busy,
    error: getSurfaceHost().error,
  });
  return presentSurface(paintCharactersStage(onIntent), model, false);
}
