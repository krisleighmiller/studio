import type { CharacterProfile } from "../../workflowTypes";

export type SketchIntent =
  | { type: "set-strategy"; strategy: string; override: boolean }
  | { type: "alternate" }
  | { type: "backfill" }
  | { type: "draft"; strategy: string }
  | { type: "approve-animatic" }
  | { type: "set-duration"; shotId: string; durationSeconds: number }
  | { type: "reorder-animatic"; shotIds: string[] }
  | { type: "attach-scratch"; shotId: string }
  | { type: "clear-scratch"; shotId: string }
  | { type: "reorder-cuts"; shotIds: string[] }
  | {
      type: "save-sketch";
      cut: number;
      sketch: {
        title: string | null;
        shot_role: string | null;
        subject: string | null;
        speaker: string | null;
        mouth_policy: string | null;
        pace: string | null;
        visual_prompt: string | null;
        negative_prompt: string | null;
        reference_roles: string[];
        transition_idea: string | null;
        dialogue: { speaker: string; heard_as?: string | null; text: string }[];
        on_camera: string[];
      };
    };

export type ReferenceIntent =
  | {
      type: "set-source";
      cut: number;
      strategy: string;
      predecessorCut: number | null;
      assetId: string | null;
    }
  | { type: "bind-frame"; cut: number; predecessorCut: number; frameTimeSeconds: number }
  | { type: "draft-prompt"; cut: number }
  | { type: "save-prompt"; cut: number; prompt: string; negativePrompt: string }
  | { type: "generate"; cut: number }
  | { type: "attach-still"; cut: number }
  | { type: "approve"; cut: number; assetId: string }
  | { type: "reject"; cut: number; assetId: string }
  | { type: "edit"; cut: number; assetId: string; brief: string }
  | { type: "delete"; cut: number; assetId: string; path: string };

export type NotesIntent =
  | { type: "sync-all" }
  | { type: "refresh-takes" }
  | { type: "alternate" }
  | { type: "clear-package"; cut: number }
  | { type: "save-prompt"; cut: number; prompt: string; negativePrompt: string }
  | { type: "sync-cut"; cut: number }
  | { type: "render"; cut: number }
  | { type: "set-take-status"; cut: number; takeId: string; status: string }
  | { type: "delete-take"; cut: number; takeId: string; path: string };

export type TakeReviewIntent =
  | { type: "refresh" }
  | { type: "alternate" }
  | { type: "set-status"; cut: number; takeId: string; status: string };

export type AssembleIntent =
  | { type: "prepare" }
  | { type: "set-version-stage"; versionId: string; stage: string; label: string }
  | { type: "mark-version" }
  | { type: "lock-picture"; versionId: string }
  | { type: "set-editorial"; path: string; note: string }
  | { type: "set-post-status"; field: string; status: string; label: string }
  | { type: "check-deliverable"; deliverableId: string };

export type ProfileIntent =
  | { type: "save"; profile: Record<string, unknown> }
  | { type: "set-requirement"; requirementId: string; status: string; name: string }
  | { type: "add-requirement"; name: string; kind: string; sceneId: number | null };

export type CharacterIntent = {
  type: "save";
  characterId: string | null;
  name: string;
  profile: CharacterProfile;
};
