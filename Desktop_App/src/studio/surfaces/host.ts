import type { ask, openDialog, toMediaUrlAsync } from "../../host";
import type {
  CutRecord,
  ProjectReport,
  ProjectState,
  RecipeView,
  RequirementKind,
  RequirementRecord,
  SceneRecord,
} from "../../workflowTypes";

/** Studio surfaces mutate the project only through these existing host commands. */
export type SurfaceHost = {
  busy: string | null;
  busyControl: string | null;
  error: string | null;
  chatReady: boolean;
  state: ProjectState | null;
  report: ProjectReport | null;
  selectedCut: number | null;
  selectedSceneId: number | null;
  selectedRequirementId: string | null;
  run: (label: string, cmd: string, args: Record<string, unknown>) => Promise<boolean>;
  requireChat: () => { apiId: string; modelId: string } | null;
  rerender: () => void;
  setError: (message: string) => void;
  rememberScene: (scene: SceneRecord, cut: CutRecord) => void;
  recipeRequires: (role: string) => boolean;
  resolvePath: (path: string | null | undefined) => string | null;
  ask: typeof ask;
  pickFile: typeof openDialog;
  mediaUrl: typeof toMediaUrlAsync;
  continuityDependents: (cut: CutRecord) => string[];
  reviewExportPath: (scene: SceneRecord) => string | null;
  activeRecipe: () => RecipeView | null;
  requirementKindLabel: (kind: RequirementKind) => string;
  requirementBlocks: (requirement: RequirementRecord) => boolean;
  blockerNames: (sceneId: number) => string[];
  hooks: {
    onOpenPath: (path: string) => void;
    onOpenVoice: (characterDirectory: string) => void;
    onComposeChat: (text: string, attachment?: string | null) => void;
    onAddToChat: (path: string) => void;
  } | null;
};
