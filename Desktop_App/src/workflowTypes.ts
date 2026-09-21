/** Wire types for `workflow_import` / `workflow_status`. Snake_case matches the host JSON. */

export type StageId =
  | "script"
  | "characters"
  | "scene_intent"
  | "scene_look"
  | "cut_sketches"
  | "reference_images"
  | "production_notes"
  | "render"
  | "take_review"
  | "assemble";

export type StageStatus = "missing" | "present" | "draft" | "approved";

export type SpeechMode = "none" | "voice_over" | "source_video_lipsync";

export type MixCounts = {
  lipsync: number;
  voice_over: number;
  silent: number;
};

export type SketchMix = {
  performance: number;
  other_dialogue: number;
  silent: number;
};

export type MixVerdict = {
  status: "ok" | "warn" | "reject";
  strategy: SceneStrategy;
  mix: SketchMix;
  messages: string[];
};

export type SceneStrategy = "performance_heavy" | "cinematic_balanced" | "atmosphere_heavy";

export type ShotRole =
  | "performance"
  | "reaction"
  | "insert"
  | "atmosphere"
  | "establishing"
  | "transition"
  | "action"
  | "detail";

export type MouthPolicy =
  | "speaking_visible"
  | "closed_or_not_visible"
  | "off_camera"
  | "no_people";

export type SceneIntent = {
  narrative_purpose?: string | null;
  emotional_purpose?: string | null;
  character_focus?: string[];
  visual_contrast?: string | null;
  important_props?: string[];
  continuity_constraints?: string[];
  sound_and_transition?: string[];
  audience_must_know?: string | null;
};

export type SketchDialogue = {
  speaker: string;
  text: string;
  heard_as?: string | null;
};

export type CutSketch = {
  title?: string | null;
  shot_role?: ShotRole | null;
  subject?: string | null;
  speaker?: string | null;
  mouth_policy?: MouthPolicy | null;
  pace?: string | null;
  visual_prompt?: string | null;
  negative_prompt?: string | null;
  reference_roles?: string[];
  transition_idea?: string | null;
  dialogue?: SketchDialogue[];
  on_camera?: string[];
};

export type SceneReport = {
  id: number;
  title: string;
  cuts_found: number;
  mix: MixCounts;
  approved_start_frames: number;
  production_notes: boolean;
  scene_intent: boolean;
  scene_look: boolean;
  cut_sketches: boolean;
  can_backfill_sketches: boolean;
  effectively_performance_heavy: boolean;
  strategy?: SceneStrategy;
  sketch_mix?: SketchMix;
  mix_verdict?: MixVerdict;
  warnings: string[];
  summary: string;
};

export type ProjectReport = {
  project_name?: string | null;
  script_count: number;
  character_count: number;
  identity_ready: number;
  voice_ready: number;
  scenes: SceneReport[];
  warnings: string[];
  summary: string;
};

export type StageMap = Record<StageId, StageStatus>;

export type DialogueLine = {
  speaker: string;
  text: string;
};

export type CutId = {
  source_scene: number;
  cut: number;
};

export type CutRecord = {
  id: CutId;
  title?: string | null;
  dialogue?: DialogueLine[];
  speech_mode?: SpeechMode | null;
  scene_type?: string | null;
  subject?: string | null;
  render_lane?: string | null;
  prompt?: string | null;
  negative_prompt?: string | null;
  production_note_path?: string | null;
  identity_ref_paths?: string[];
  approved_start_frame?: string | null;
  start_frame_prompt?: string | null;
  start_frame_negative_prompt?: string | null;
  sketch?: CutSketch | null;
  takes?: unknown[];
  selected_take?: string | null;
};

export type SceneRecord = {
  id: number;
  title: string;
  source_path?: string | null;
  source_excerpt?: string | null;
  location?: string | null;
  action?: string[];
  sound?: string[];
  transitions?: string[];
  dialogue?: DialogueLine[];
  beats?: unknown[];
  intent?: SceneIntent | null;
  intent_approved?: boolean;
  strategy?: SceneStrategy | null;
  strategy_override?: boolean;
  look_board?: LookBoard | null;
  cuts: CutRecord[];
  stages: StageMap;
};

export type LookSlot = {
  id: string;
  role: string;
  title?: string;
  prompt?: string | null;
  negative_prompt?: string | null;
  asset_ids?: string[];
  approved_asset_id?: string | null;
};

export type LookBoard = {
  notes?: string[];
  asset_ids?: string[];
  slots?: LookSlot[];
};

export type AssetRecord = {
  id: string;
  path: string;
  role: string;
  status?: string;
  scene_id?: number | null;
  cut_id?: CutId | null;
  character_id?: string | null;
  source_job_id?: string | null;
  parent_asset_id?: string | null;
  notes?: string | null;
};

export type CharacterRecord = {
  id: string;
  name: string;
  dir: string;
  identity_ready: boolean;
  voice_ready: boolean;
  profile_path?: string | null;
  model_sheet_manifest_path?: string | null;
  profile?: CharacterProfile;
  asset_ids?: string[];
  notes?: string[];
};

export type CharacterProfile = {
  description?: string | null;
  appearance?: string[];
  wardrobe?: string[];
  personality?: string[];
  continuity_notes?: string[];
  identity_prompt?: string | null;
};

export type JobRecord = {
  id: string;
  kind: string;
  provider?: string | null;
  model?: string | null;
  status?: string;
  error?: string | null;
  output_path?: string | null;
  output_asset_ids?: string[];
};

export type ProjectState = {
  version: number;
  project_name?: string | null;
  scripts?: { path: string; title?: string | null; aspect_ratio?: string | null }[];
  characters: CharacterRecord[];
  scenes: SceneRecord[];
  assets?: AssetRecord[];
  jobs?: JobRecord[];
  notes?: string[];
};

export type WorkflowImportResult = {
  state: ProjectState;
  report: ProjectReport;
  summary: string;
  persistedPath?: string | null;
  note?: string | null;
  pendingAction?: string | null;
};

export const SHOT_ROLES: { id: ShotRole; label: string }[] = [
  { id: "performance", label: "Performance" },
  { id: "reaction", label: "Reaction" },
  { id: "insert", label: "Insert" },
  { id: "atmosphere", label: "Atmosphere" },
  { id: "establishing", label: "Establishing" },
  { id: "transition", label: "Transition" },
  { id: "action", label: "Action" },
  { id: "detail", label: "Detail" },
];

export const MOUTH_POLICIES: { id: MouthPolicy; label: string }[] = [
  { id: "speaking_visible", label: "Speaking visible" },
  { id: "closed_or_not_visible", label: "Closed / not visible" },
  { id: "off_camera", label: "Off camera" },
  { id: "no_people", label: "No people" },
];

export const LOOK_ROLES: { id: string; label: string }[] = [
  { id: "interior_reference", label: "Interior" },
  { id: "exterior_reference", label: "Exterior" },
  { id: "location_reference", label: "Location" },
  { id: "wardrobe_reference", label: "Wardrobe" },
  { id: "prop_reference", label: "Prop" },
  { id: "mood_reference", label: "Mood" },
  { id: "lighting_reference", label: "Lighting" },
  { id: "composition_reference", label: "Composition" },
  { id: "group_reference", label: "Group" },
];

export const SCENE_STRATEGIES: { id: SceneStrategy; label: string }[] = [
  { id: "cinematic_balanced", label: "Cinematic balanced" },
  { id: "performance_heavy", label: "Performance heavy" },
  { id: "atmosphere_heavy", label: "Atmosphere heavy" },
];

export const WORKFLOW_STAGES: { id: StageId; label: string }[] = [
  { id: "script", label: "Script" },
  { id: "characters", label: "Characters" },
  { id: "scene_intent", label: "Scene Intent" },
  { id: "scene_look", label: "Scene Look" },
  { id: "cut_sketches", label: "Cut Sketches" },
  { id: "reference_images", label: "Reference Images" },
  { id: "production_notes", label: "Production Notes" },
  { id: "render", label: "Render" },
  { id: "take_review", label: "Take Review" },
  { id: "assemble", label: "Assemble" },
];
