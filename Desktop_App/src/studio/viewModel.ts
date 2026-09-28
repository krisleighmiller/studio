import type { AssetRecord, CutRecord, ProjectReport, ProjectState, SceneRecord, StageId } from "../workflowTypes";
import type { DirectorSelection, WorkAreaId } from "./selection";
import { describeDirectorContext, type DirectorContextModel } from "./selectors/directorContext";
import { buildFinishModel, type FinishModel } from "./selectors/finish";
import { buildJobsModel, type JobFilter, type JobsModel } from "./selectors/jobs";
import { buildProjectSummary, type ProjectSummary } from "./selectors/project";
import { buildSceneSummaries, type SceneSummary } from "./selectors/scenes";
import { buildWorkspace, type WorkspaceModel } from "./selectors/workspace";

export type StudioViewModel = {
  project: ProjectSummary;
  scenes: SceneSummary[];
  workspace: WorkspaceModel;
  director: DirectorContextModel;
  jobs: JobsModel;
  finish: FinishModel;
};

export function buildStudioView(input: {
  state: ProjectState;
  report: ProjectReport | null;
  selection: DirectorSelection;
  scene: SceneRecord | null;
  cut: CutRecord | null;
  stage: StageId | null;
  workArea: WorkAreaId | null;
  lookSlotId: string | null;
  jobFilter: JobFilter | "all";
  busy: string | null;
  resolvePath: (path: string | null | undefined) => string | null;
}): StudioViewModel {
  const assets: AssetRecord[] = input.state.assets ?? [];
  const jobs = input.state.jobs ?? [];
  return {
    project: buildProjectSummary({
      state: input.state,
      report: input.report,
      workArea: input.workArea,
    }),
    scenes: buildSceneSummaries({
      state: input.state,
      reports: input.report?.scenes ?? [],
      assets,
      resolvePath: input.resolvePath,
    }),
    workspace: buildWorkspace({
      scene: input.scene,
      cut: input.cut,
      stage: input.stage,
      lookSlotId: input.lookSlotId,
      busy: input.busy,
    }),
    director: describeDirectorContext(
      input.state,
      input.selection,
      input.report?.recipe?.stages.find((stage) => stage.id === input.selection.stage)?.label ?? null,
    ),
    jobs: buildJobsModel({
      jobs,
      scenes: input.state.scenes,
      assets,
      filter: input.jobFilter,
    }),
    finish: buildFinishModel({
      scene: input.scene,
      jobs,
      deliverables: input.state.profile?.deliverables ?? [],
      busyReason: input.busy,
      state: input.state,
    }),
  };
}
