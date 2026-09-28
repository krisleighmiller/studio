/** Wire types for `workflow_import` / `workflow_status`. Snake_case matches the host JSON. */

/** Stage ids come from the host recipe. Known editors recognize some of them. */
export type StageId = string;

/** A recipe can name a stage this file has never switched on. */
export const EXAMPLE_RECIPE_STAGE = "color_script" satisfies StageId;

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
  planned_visual_sources: number;
  render_ready_sources: number;
  unresolved_visual_sources: number;
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
  navigation?: StageNavItem[];
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
  recipe?: RecipeView;
  recipes?: RecipeChoice[];
};

export type StageMap = {
  script: StageStatus;
  characters: StageStatus;
  scene_intent: StageStatus;
  scene_look: StageStatus;
  cut_sketches: StageStatus;
  reference_images: StageStatus;
  production_notes: StageStatus;
  render: StageStatus;
  take_review: StageStatus;
  assemble: StageStatus;
  extensions?: Record<string, StageStatus>;
};

const KNOWN_STAGE_FIELDS = [
  "script",
  "characters",
  "scene_intent",
  "scene_look",
  "cut_sketches",
  "reference_images",
  "production_notes",
  "render",
  "take_review",
  "assemble",
] as const;

export function stageStatus(map: StageMap | null | undefined, id: string): StageStatus {
  if (!map) return "missing";
  if ((KNOWN_STAGE_FIELDS as readonly string[]).includes(id)) {
    return map[id as (typeof KNOWN_STAGE_FIELDS)[number]] ?? "missing";
  }
  return map.extensions?.[id] ?? "missing";
}

export type DialogueLine = {
  speaker: string;
  text: string;
};

export type CutId = {
  source_scene: number;
  cut: number;
};

export type AnimaticShot = {
  id: string;
  cut_id: CutId;
  duration_seconds: number;
  scratch_audio_path?: string | null;
};

export type CoverageWarning = {
  id: string;
  kind: string;
  message: string;
};

export type SceneAnimatic = {
  id: string;
  approved?: boolean;
  shots?: AnimaticShot[];
  warnings?: CoverageWarning[];
};

export type VisualSourceStrategy =
  | "preproduction_start_frame"
  | "previous_selected_take_exit_frame"
  | "shared_reference"
  | "no_start_frame"
  | "deferred_manual";

export type VisualSourceSpec = {
  strategy: VisualSourceStrategy;
  predecessor?: CutId | null;
  asset_id?: string | null;
  source_take_id?: string | null;
  frame_time_seconds?: number | null;
  predecessor_shot_id?: string | null;
};

export type TakeRecord = {
  id: string;
  path: string;
  status: string;
  notes?: string | null;
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
  visual_source?: VisualSourceSpec | null;
  takes?: TakeRecord[];
  selected_take?: string | null;
  stable_id?: string;
  dependency_state?: "stale" | "needs_review" | "invalid" | "keep" | null;
  package_needs_review?: boolean;
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
  stable_id?: string;
  animatic?: SceneAnimatic | null;
  post?: ScenePost | null;
};

export type CutVersionStage = "selects" | "assembly" | "fine_cut" | "picture_lock";

export type PostStatus = "not_started" | "in_progress" | "needs_review" | "approved";

export type EditorialPath = "host_ffmpeg" | "external_nle";

export type PostField =
  | "audio_mix"
  | "vfx"
  | "compositing"
  | "captions"
  | "mastering"
  | "qc";

export type VersionShot = {
  shot_id: string;
  cut_label: number;
  take_id: string;
  path: string;
};

export type CutVersion = {
  id: string;
  label: string;
  stage: CutVersionStage;
  assembly_path?: string | null;
  shots: VersionShot[];
  editorial_path?: EditorialPath;
};

export type DeliverableCheck = {
  deliverable_id: string;
  version_id?: string | null;
  passed: boolean;
  resolution_ok: boolean;
  aspect_ok: boolean;
  missing_shots?: { shot_id: string; cut_label: number; reason: string }[];
  messages?: string[];
};

export type ScenePost = {
  versions?: CutVersion[];
  picture_locked_id?: string | null;
  editorial_path?: EditorialPath;
  external_nle_note?: string | null;
  audio_mix?: PostStatus;
  vfx?: PostStatus;
  compositing?: PostStatus;
  captions?: PostStatus;
  mastering?: PostStatus;
  qc?: PostStatus;
  last_check?: DeliverableCheck | null;
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

export type JobAttempt = {
  id: string;
  status?: string;
  error?: string | null;
  output_asset_ids?: string[];
  started_at?: string | null;
  completed_at?: string | null;
};

export type JobScope = {
  kind: string;
  id: string;
};

export type JobRecord = {
  id: string;
  kind: string;
  provider?: string | null;
  model?: string | null;
  status?: string;
  error?: string | null;
  output_path?: string | null;
  raw_provider_id?: string | null;
  output_asset_ids?: string[];
  input_asset_ids?: string[];
  attempts?: JobAttempt[];
  created_at?: string | null;
  started_at?: string | null;
  completed_at?: string | null;
  scope?: JobScope | null;
  progress?: string | null;
  cost_minor?: number | null;
  estimated_cost_minor?: number | null;
  currency?: string | null;
  retry_of?: string | null;
};

/** Host registry id. A new recipe is valid once the host table accepts it. */
export type RecipeId = string;

export const NARRATIVE_RECIPE = "narrative_dialogue" satisfies RecipeId;
export const MONTAGE_RECIPE = "montage" satisfies RecipeId;

/** @deprecated Use RecipeId. Kept so older imports still name the registry id. */
export type ProductionRecipe = RecipeId;

export type StageNavItem = {
  id: string;
  label: string;
  work_area: "plan" | "produce" | "finish";
  status: string;
  available: boolean;
  disabled_reason?: string | null;
  depends_on: string[];
};

export type RecipeStageView = {
  id: string;
  label: string;
  work_area?: "plan" | "produce" | "finish";
  depends_on: string[];
  required_roles: string[];
};

export type RecipeView = {
  id: string;
  label: string;
  stages: RecipeStageView[];
};

export type RecipeChoice = {
  id: string;
  label: string;
};

export type RequirementKind =
  | "cast"
  | "location"
  | "prop"
  | "wardrobe"
  | "sound"
  | "graphic"
  | "risky_shot"
  | "deferred"
  | "voice"
  | "plate";

export type RequirementStatus = "missing" | "draft" | "present" | "approved" | "deferred";

export type DeliverableRecord = {
  id: string;
  name: string;
  audience?: string | null;
  platforms?: string[];
  runtime?: string | null;
  aspect_ratio?: string | null;
  resolution?: string | null;
  frame_rate?: string | null;
  language?: string | null;
};

export type ProductionProfile = {
  recipe: ProductionRecipe;
  deliverables?: DeliverableRecord[];
  audience?: string | null;
  platforms?: string[];
  runtime?: string | null;
  aspect_ratios?: string[];
  resolution?: string | null;
  frame_rate?: string | null;
  language?: string | null;
  medium?: string | null;
  style_notes?: string | null;
  audio_plan?: string | null;
  continuity_rules?: string[];
  safety_constraints?: string[];
  budget?: string | null;
  schedule?: string | null;
  rights_notes?: string | null;
  /** Present when the profile names a recipe the built-in table does not contain. */
  recipe_document?: {
    id: string;
    label: string;
    stages: Array<{
      id: string;
      label: string;
      work_area: string;
      depends_on?: string[];
      required_roles?: string[];
    }>;
  } | null;
};

export type RequirementRecord = {
  id: string;
  kind: RequirementKind;
  name: string;
  status: RequirementStatus;
  scene_ids?: number[];
  character_id?: string | null;
  notes?: string | null;
  status_override?: boolean;
  user_added?: boolean;
};

export type ProjectBudget = {
  /** ISO 4217 code for every amount in this budget. */
  currency: string;
  /** Hard ceiling in minor units (cents). Absent means no limit. */
  limit_minor?: number | null;
  /** Spend level that should surface a warning before the limit. */
  warning_at_minor?: number | null;
  /** When true, the ceiling cannot be bypassed by the Director. */
  hard_stop?: boolean;
};

export type ProjectState = {
  version: number;
  project_name?: string | null;
  scripts?: { path: string; title?: string | null; aspect_ratio?: string | null }[];
  characters: CharacterRecord[];
  scenes: SceneRecord[];
  assets?: AssetRecord[];
  jobs?: JobRecord[];
  /** Structured project budget. Absent means unbudgeted (no limit, no warning). */
  budget?: ProjectBudget | null;
  profile?: ProductionProfile | null;
  requirements?: RequirementRecord[];
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

export const VISUAL_SOURCE_STRATEGIES: {
  id: VisualSourceStrategy;
  label: string;
}[] = [
  { id: "preproduction_start_frame", label: "Generated or attached start frame" },
  {
    id: "previous_selected_take_exit_frame",
    label: "Previous selected take’s exit frame",
  },
  { id: "shared_reference", label: "Approved shared scene reference" },
  { id: "no_start_frame", label: "No start frame" },
  { id: "deferred_manual", label: "Deferred manual source" },
];

export const REQUIREMENT_KINDS: { id: RequirementKind; label: string }[] = [
  { id: "cast", label: "Cast" },
  { id: "voice", label: "Voice" },
  { id: "location", label: "Location" },
  { id: "prop", label: "Prop" },
  { id: "wardrobe", label: "Wardrobe" },
  { id: "plate", label: "Plate" },
  { id: "sound", label: "Sound" },
  { id: "graphic", label: "Graphic" },
  { id: "risky_shot", label: "Risky shot" },
  { id: "deferred", label: "Deferred" },
];

export const REQUIREMENT_STATUSES: { id: RequirementStatus; label: string }[] = [
  { id: "missing", label: "Missing" },
  { id: "draft", label: "Draft" },
  { id: "present", label: "Present" },
  { id: "approved", label: "Approved" },
  { id: "deferred", label: "Deferred" },
];
