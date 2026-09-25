import { MOUTH_POLICIES, SHOT_ROLES, type MixCounts, type MouthPolicy, type ShotRole, type SpeechMode } from "../workflowTypes";

export function speechLabel(mode: SpeechMode | null | undefined): string {
  switch (mode) {
    case "source_video_lipsync":
      return "lip-sync";
    case "voice_over":
      return "voice-over";
    case "none":
      return "silent";
    default:
      return "unspecified";
  }
}

export function mixLabel(mix: MixCounts): string {
  return `${mix.lipsync} LS / ${mix.voice_over} VO / ${mix.silent} silent`;
}

export function roleLabel(role: ShotRole | null | undefined): string {
  return SHOT_ROLES.find((item) => item.id === role)?.label ?? "—";
}

export function mouthLabel(policy: MouthPolicy | null | undefined): string {
  return MOUTH_POLICIES.find((item) => item.id === policy)?.label ?? "—";
}
