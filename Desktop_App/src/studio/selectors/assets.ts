import type { AssetRecord, ProjectState } from "../../workflowTypes";
import { LOOK_ROLES } from "../../workflowTypes";
import { AUDIO_ROLE_LABELS, isAudioRole } from "./audio";

/** The project assets panel is filtered by audio role: the four canonical
 *  display sections (Dialogue / Music / Ambience / Effects) plus an
 *  everything view and an audio-only view. */
export type AssetFilterId = "all" | "audio" | "dialogue" | "music" | "ambience" | "effects";

export const ASSET_FILTERS: { id: AssetFilterId; label: string }[] = [
  { id: "all", label: "All" },
  { id: "audio", label: "Audio" },
  { id: "dialogue", label: "Dialogue" },
  { id: "music", label: "Music" },
  { id: "ambience", label: "Ambience" },
  { id: "effects", label: "Effects" },
];

const SECTION_ROLES: Record<Exclude<AssetFilterId, "all" | "audio">, string[]> = {
  dialogue: ["dialogue", "voice_over"],
  music: ["music"],
  ambience: ["ambience", "room_tone"],
  effects: ["sound_effect", "foley"],
};

export type AssetRowModel = {
  id: string;
  name: string;
  role: string;
  roleLabel: string;
  sceneId: number | null;
  status: string;
};

export type AssetsModel = {
  filter: AssetFilterId;
  counts: Record<AssetFilterId, number>;
  rows: AssetRowModel[];
};

/** Human label for any asset role: audio roles via the audio table, look/reference
 *  roles via LOOK_ROLES, everything else falls back to the raw value. */
export function assetRoleLabel(role: string): string {
  const audio = AUDIO_ROLE_LABELS[role];
  if (audio) return audio;
  const look = LOOK_ROLES.find((item) => item.id === role);
  if (look) return look.label;
  return role;
}

function matches(asset: AssetRecord, filter: AssetFilterId): boolean {
  if (filter === "all") return true;
  if (filter === "audio") return isAudioRole(asset.role);
  return SECTION_ROLES[filter].includes(asset.role);
}

export function buildAssetsModel(state: ProjectState, filter: AssetFilterId): AssetsModel {
  const assets = state.assets ?? [];
  const counts = {} as Record<AssetFilterId, number>;
  for (const facet of ASSET_FILTERS) {
    counts[facet.id] = assets.filter((asset) => matches(asset, facet.id)).length;
  }
  const rows = assets
    .filter((asset) => matches(asset, filter))
    .map((asset) => ({
      id: asset.id,
      name: asset.path.split(/[\\/]/).pop() || asset.id,
      role: asset.role,
      roleLabel: assetRoleLabel(asset.role),
      sceneId: asset.scene_id ?? null,
      status: asset.status ?? "unknown",
    }))
    .sort((a, b) => a.roleLabel.localeCompare(b.roleLabel) || (a.sceneId ?? 0) - (b.sceneId ?? 0));
  return { filter, counts, rows };
}