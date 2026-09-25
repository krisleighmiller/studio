import { invoke } from "../host";
import type { WorkflowImportResult } from "../workflowTypes";

export type StudioCommand = {
  label: string;
  cmd: string;
  args: Record<string, unknown>;
};

/** The only Studio path that calls the host. The desk refreshes state from the result. */
export function createStudioController(deps: {
  projectRoot: () => string | null;
  onBusy: (label: string | null) => void;
  onResult: (result: WorkflowImportResult) => void;
  onError: (message: string) => void;
  beforeRun?: () => void;
}) {
  return {
    async run(command: StudioCommand): Promise<boolean> {
      deps.beforeRun?.();
      const root = deps.projectRoot();
      if (!root) {
        deps.onError("Open a project first.");
        return false;
      }
      deps.onBusy(command.label);
      try {
        const result = await invoke<WorkflowImportResult>(command.cmd, {
          projectRoot: root,
          ...command.args,
        });
        deps.onBusy(null);
        deps.onResult(result);
        return true;
      } catch (error) {
        deps.onBusy(null);
        deps.onError(error instanceof Error ? error.message : String(error));
        return false;
      }
    },
  };
}
