import type { CharacterProfile } from "../../workflowTypes";

export type ScriptDraftIntent =
  | { type: "change"; draft: import("./scriptDraftForm").ScriptDraftText }
  | { type: "save"; draft: import("./scriptDraftForm").ScriptDraftText }
  | { type: "generate"; draft: import("./scriptDraftForm").ScriptDraftText }
  | { type: "load-notes" }
  | { type: "save-notes"; draft: import("./scriptDraftForm").ScriptDraftText }
  | { type: "reload-notes" }
  | { type: "retry-recovery" }
  | { type: "discard" }
  | { type: "cancel" };

export type SketchIntent =
  | { type: "set-strategy"; strategy: string; override: boolean }
  | { type: "set-max-cuts"; maxCuts: number }
  | { type: "alternate" }
  | { type: "remove-cut"; shotId: string }
  | { type: "reopen-sketch"; shotId: string }
  | { type: "resolve-script-dialogue"; shotId: string; choice: "keep" }
  | { type: "backfill" }
  | { type: "draft"; strategy: string }
  | { type: "approve-animatic" }
  | { type: "set-duration"; shotId: string; durationSeconds: number }
  | { type: "set-shot-trim"; shotId: string; trimInSeconds: number | null; trimOutSeconds: number | null; expectedTakeId?: string }
  | { type: "set-shot-transition"; shotId: string; transition: string | null }
  | { type: "reorder-animatic"; shotIds: string[] }
  | { type: "attach-scratch"; shotId: string }
  | { type: "clear-scratch"; shotId: string }
  | { type: "reorder-cuts"; shotIds: string[] }
  | {
      type: "save-sketch";
      cut: number;
      shotId?: string;
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
  | { type: "delete-look-slot"; slotId: string; title: string }
  | { type: "save-description"; slotId: string | null; description: string; category: string }
  | {
      type: "set-source";
      cut: number;
      strategy: string;
      predecessorCut: number | null;
      predecessorScene: number | null;
      assetId: string | null;
    }
  | { type: "bind-frame"; cut: number; predecessorCut: number; predecessorScene: number; frameTimeSeconds: number }
  | { type: "draft-prompt"; cut: number }
  | { type: "save-prompt"; cut: number; prompt: string; negativePrompt: string }
  | { type: "generate"; cut: number; referenceAssetIds?: string[] }
  | { type: "attach-still"; cut: number }
  | { type: "approve"; cut: number; assetId: string }
  | { type: "reject"; cut: number; assetId: string }
  | { type: "edit"; cut: number; assetId: string; brief: string }
  | { type: "delete"; cut: number; assetId: string; path: string };

export type NotesIntent =
  | { type: "generate-dialogue"; cut: number; voiceAssetId: string }
  | { type: "sync-all" }
  | { type: "set-shot-trim"; shotId: string; trimInSeconds: number; trimOutSeconds: number; expectedTakeId: string; sourceEndSeconds?: number }
  | { type: "refresh-takes" }
  | { type: "alternate" }
  | { type: "clear-package"; cut: number }
  | { type: "save-prompt"; cut: number; prompt: string; negativePrompt: string }
  | { type: "sync-cut"; cut: number }
  | { type: "review-handoff"; cut: number }
  | { type: "render"; cut: number }
  | { type: "resume-audio"; cut: number }
  | { type: "select-performance"; cut: number; assetId: string | null; reviewedAssetId?: string }
  | { type: "import-performance"; cut: number }
  | { type: "version-audio"; cut: number; assetId: string; speed: number }
  | { type: "version-pitch"; cut: number; assetId: string; pitch: number }
  | { type: "version-trim"; cut: number; assetId: string; startSeconds: number; endSeconds: number }
  | { type: "prepare-performance"; cut: number; reviewedAssetId: string }
  | { type: "set-take-status"; cut: number; takeId: string; status: string }
  | { type: "delete-take"; cut: number; takeId: string; path: string };

export type TakeReviewIntent =
  | { type: "refresh" }
  | { type: "alternate" }
  | { type: "set-status"; cut: number; takeId: string; status: string }
  | { type: "delete-take"; cut: number; takeId: string; path: string };

export type AssembleIntent =
  | { type: "prepare" }
  | { type: "reorder-sequence"; shotIds: string[] }
  | { type: "set-shot-trim"; shotId: string; trimInSeconds: number; trimOutSeconds: number; expectedTakeId: string; sourceEndSeconds?: number }
  | { type: "prepare-project" }
  | { type: "approve-rendered"; jobId: string }
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

export type CharacterReferenceImage = { id: string; path: string; name?: string };

export type CharacterReferenceIntent =
  | { type: "add-reference"; characterKey: string; assetIds: string[] }
  | { type: "remove-reference"; characterKey: string; assetId: string; assetIds: string[] };

export type CharacterIntent = CharacterReferenceIntent
  | { type: "generate-character-wardrobe"; characterKey: string; prompt: string; referenceAssetIds: string[] }
  | { type: "attach-character-wardrobe"; characterKey: string }
  | { type: "build-wardrobe-plates"; characterKey: string; sceneId: number; wardrobeAssetId: string }
  | { type: "delete-wardrobe-plate"; characterKey: string; assetId: string }
  | { type: "create-model-sheet"; characterKey: string; approvalMode?: "each" | "whole" }
  | { type: "delete-model-sheet-photo"; characterKey: string; assetId: string }
  | {
  type: "save";
  characterId: string | null;
  name: string;
  profile: CharacterProfile;
};
