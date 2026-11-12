import type { AssetRecord, CharacterRecord } from "../workflowTypes";

export type ModelSheetRequest = {
  characterId: string;
  action: "run_specialist";
  specialist: "model_sheet";
  inputs: {
    character_files: [string];
    output_path: string;
    character_slug: string;
    identity_text: string;
    reference_images: string[];
  };
};
export type ModelSheetValidationError = {
  validationError: { field: "character" | "profile_path" | "dir" | "visualInput" | "references"; message: string };
};

export function buildModelSheetRequest(
  savedCharacter: CharacterRecord | null | undefined,
  identityBrief: string,
  references: readonly AssetRecord[],
): ModelSheetRequest | ModelSheetValidationError {
  const invalid = (field: ModelSheetValidationError["validationError"]["field"], message: string): ModelSheetValidationError =>
    ({ validationError: { field, message } });
  // Saved paths are canonical project-relative paths. Never normalize a bad token.
  const safePath = (path: string): boolean => Boolean(path) && !/[\\\\\u0000-\u001f\u007f:]/.test(path)
    && path.split("/").every(part => Boolean(part.trim()) && part !== "." && part !== "..");
  if (!savedCharacter || !savedCharacter.id?.trim() || !savedCharacter.name?.trim()) {
    return invalid("character", "Save a named character before building a model sheet.");
  }
  const { dir, profile_path: profilePath } = savedCharacter;
  if (!safePath(dir) || dir.split("/").length !== 2 || !dir.startsWith("Characters/")) {
    return invalid("dir", "Character directory must be a saved project-relative Characters directory.");
  }
  if (!profilePath || !safePath(profilePath) || !profilePath.startsWith(`${dir}/`)) {
    return invalid("profile_path", "Save this character record in its character folder to associate the model sheet.");
  }
  // Visual inputs only; no biography, personality, continuity sheet or prior output is required.
  const visualText = [identityBrief, savedCharacter.profile?.description, ...(savedCharacter.profile?.appearance ?? [])]
    .filter((text): text is string => typeof text === "string" && Boolean(text.trim())).join("\n\n");
  const paths: string[] = [];
  const seen = new Set<string>();
  for (const id of savedCharacter.reference_asset_ids ?? []) {
    if (!id.trim() || seen.has(id)) return invalid("references", "Reference selections contain an empty or duplicate asset ID.");
    seen.add(id);
    const matches = references.filter(reference => reference.id === id);
    if (matches.length !== 1) return invalid("references", `Reference ${id} is missing or ambiguous in current saved assets.`);
    const reference = matches[0];
    if (reference.character_id !== savedCharacter.id
      || !["identity_reference", "body_reference", "expression_reference"].includes(reference.role)
      || !safePath(reference.path) || !reference.path.startsWith(`${dir}/`)
      || !/\.(png|jpe?g|webp)$/i.test(reference.path) || paths.includes(reference.path)) {
      return invalid("references", `Reference ${id} is not a valid image for this character.`);
    }
    paths.push(reference.path);
  }
  if (!visualText.trim() && !paths.length) return invalid("visualInput", "Add a visual Description or Appearance, or select a reference image, then save and retry.");
  return {
    characterId: savedCharacter.id,
    action: "run_specialist",
    specialist: "model_sheet",
    inputs: {
      character_files: [profilePath],
      output_path: `${dir}/model_sheet`,
      character_slug: savedCharacter.name,
      identity_text: visualText,
      reference_images: paths,
    },
  };
}
