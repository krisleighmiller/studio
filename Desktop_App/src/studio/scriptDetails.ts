import type { SceneRecord } from "../workflowTypes";

export type DetailTask = { sceneId: number; sceneStableId: string; scriptSource: string; cmd: "workflow_draft_intent" | "workflow_draft_look" };

/** Only absent records qualify: even an empty saved record may be user-authored. */
export function scriptDetailTasks(scenes: SceneRecord[], source: string): DetailTask[] {
  return scenes.filter(s => s.source_path === source && !s.source_missing && !!s.stable_id).flatMap(s => {
    const base = { sceneId: s.id, sceneStableId: s.stable_id!, scriptSource: source };
    return [
      ...(!s.intent && !s.intent_approved ? [{ ...base, cmd: "workflow_draft_intent" as const }] : []),
      ...(!s.look_board ? [{ ...base, cmd: "workflow_draft_look" as const }] : []),
    ];
  });
}

export async function runScriptDetailTasks(tasks: DetailTask[], current: () => boolean,
  run: (task: DetailTask, index: number, total: number) => Promise<boolean>): Promise<"complete" | "stopped"> {
  for (let i = 0; i < tasks.length; i++) {
    if (!current() || !await run(tasks[i], i, tasks.length) || !current()) return "stopped";
  }
  return "complete";
}
