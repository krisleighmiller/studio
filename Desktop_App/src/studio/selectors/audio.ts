import type { ProjectState } from "../../workflowTypes";

export type AudioRoleState = {
  role: string;
  present: boolean;
  approved: boolean;
  assetCount: number;
};

export type AudioStatusModel = {
  required: string[];
  roles: AudioRoleState[];
  missing: string[];
  allApproved: boolean;
  blocksFinish: boolean;
};

export const AUDIO_ROLES: string[] = [
  "dialogue",
  "voice_over",
  "music",
  "ambience",
  "room_tone",
  "sound_effect",
  "foley",
  "source_audio",
  "scratch_audio",
  "mix",
];

export const AUDIO_ROLE_LABELS: Record<string, string> = {
  dialogue: "Dialogue",
  voice_over: "Voice over",
  music: "Music",
  ambience: "Ambience",
  room_tone: "Room tone",
  sound_effect: "Sound effect",
  foley: "Foley",
  source_audio: "Source audio",
  scratch_audio: "Scratch audio",
  mix: "Mix",
};

const AUDIO_KEYWORDS: Array<[string, string[]]> = [
  ["dialogue", ["dialogue", "dialog", "speech"]],
  ["voice_over", ["voice over", "voiceover", "voice-over", "narration", "narrator"]],
  ["music", ["music", "score", "soundtrack", "theme"]],
  ["ambience", ["ambience", "ambient", "atmosphere"]],
  ["room_tone", ["room tone", "roomtone"]],
  ["sound_effect", ["sound effect", "soundeffect", "sfx"]],
  ["foley", ["foley"]],
  ["source_audio", ["source audio", "sourceaudio"]],
  ["scratch_audio", ["scratch audio", "scratch"]],
  ["mix", ["final mix", "mixdown", "master"]],
];

export function isAudioRole(role: string): boolean {
  return (
    AUDIO_ROLES.includes(role) ||
    role === "audio_take" ||
    role === "voice_sample" ||
    role === "voice_profile"
  );
}

export function audioRoleLabel(role: string): string {
  return AUDIO_ROLE_LABELS[role] ?? role;
}

/** Scan an audio-plan string for role keywords (mirrors host `parse_audio_roles`). */
export function parseAudioRoles(plan: string): string[] {
  const text = plan.toLowerCase();
  return AUDIO_KEYWORDS.filter(([, keywords]) => keywords.some((kw) => text.includes(kw))).map(
    ([role]) => role,
  );
}

/** Mirror of the host `scene_audio_status`: what audio a scene requires, has,
 *  is missing, and whether that blocks Finish. */
export function buildAudioStatus(state: ProjectState, sceneId: number): AudioStatusModel | null {
  const scene = (state.scenes ?? []).find((candidate) => candidate.id === sceneId);
  if (!scene) return null;

  const plan = state.profile?.audio_plan ?? "";
  const required = parseAudioRoles(plan);
  const hasDialogue = (scene.cuts ?? []).some((cut) => (cut.dialogue ?? []).length > 0);
  if (!required.includes("dialogue") && hasDialogue) required.push("dialogue");
  const sortedRequired = [...new Set(required)].sort();

  const sceneAssets = (state.assets ?? []).filter(
    (asset) => asset.scene_id === sceneId && isAudioRole(asset.role),
  );
  const present = [...new Set(sceneAssets.map((asset) => asset.role))].sort();

  const allRoles = [...new Set([...sortedRequired, ...present])].sort();
  const roles: AudioRoleState[] = allRoles.map((role) => {
    const matches = sceneAssets.filter((asset) => asset.role === role);
    const approved = matches.length > 0 && matches.every((asset) => asset.status === "approved");
    return {
      role,
      present: matches.length > 0,
      approved,
      assetCount: matches.length,
    };
  });

  const missing = sortedRequired.filter((role) => !present.includes(role));
  const allApproved = roles.every((entry) => !entry.present || entry.approved);
  return {
    required: sortedRequired,
    roles,
    missing,
    allApproved,
    blocksFinish: missing.length > 0 || !allApproved,
  };
}

/** Compact one-line summary of a scene's audio status, or `null` when nothing
 *  is required or present (so callers can omit the line). */
export function describeAudioStatus(status: AudioStatusModel | null): string | null {
  if (!status) return null;
  if (status.required.length === 0 && status.roles.every((role) => !role.present)) return null;
  if (!status.blocksFinish) return "Audio complete";
  const parts: string[] = [];
  if (status.missing.length) {
    parts.push(`missing ${status.missing.map(audioRoleLabel).join(", ")}`);
  }
  const unapproved = status.roles
    .filter((role) => role.present && !role.approved)
    .map((role) => audioRoleLabel(role.role));
  if (unapproved.length) parts.push(`unapproved ${unapproved.join(", ")}`);
  return `Audio: ${parts.join("; ")}`;
}

/** Four display sections (the plan's Dialogue / Music / Ambience / Effects),
 *  each collapsing the finer-grained audio roles into one entry list. */
export type AudioSectionId = "dialogue" | "music" | "ambience" | "effects";

export type AudioSectionEntry = {
  role: string;
  label: string;
  present: boolean;
  approved: boolean;
  required: boolean;
};

export type AudioSection = {
  id: AudioSectionId;
  label: string;
  entries: AudioSectionEntry[];
};

export const AUDIO_SECTIONS: { id: AudioSectionId; label: string; roles: string[] }[] = [
  { id: "dialogue", label: "Dialogue", roles: ["dialogue", "voice_over"] },
  { id: "music", label: "Music", roles: ["music"] },
  { id: "ambience", label: "Ambience", roles: ["ambience", "room_tone"] },
  { id: "effects", label: "Effects", roles: ["sound_effect", "foley"] },
];

export function audioSections(status: AudioStatusModel): AudioSection[] {
  const present = new Map(
    status.roles.filter((role) => role.present).map((role) => [role.role, role.approved]),
  );
  return AUDIO_SECTIONS.map((section) => {
    const roles = new Set<string>();
    for (const role of status.required) if (section.roles.includes(role)) roles.add(role);
    for (const entry of present.keys()) if (section.roles.includes(entry)) roles.add(entry);
    const entries = [...roles].map((role) => ({
      role,
      label: audioRoleLabel(role),
      present: present.has(role),
      approved: present.get(role) ?? false,
      required: status.required.includes(role),
    }));
    return { id: section.id, label: section.label, entries };
  }).filter((section) => section.entries.length > 0);
}