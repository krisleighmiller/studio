import { invoke } from "../host";

export type SaveProjectScriptResult = string | { saveError: string };
export type ScriptSourceKind = "external-import" | "existing-project-file";
export type IngestScriptFileResult = string | { ingestError: string };

/** Copy/register a local script only; external reads require a native dialog grant. */
export async function ingestScriptFile(
  projectRoot: string,
  sourcePath: string,
  sourceKind: ScriptSourceKind,
): Promise<IngestScriptFileResult> {
  try {
    return await invoke<string>("ingest_script_file", { projectRoot, sourcePath, sourceKind });
  } catch (error) {
    return { ingestError: error instanceof Error ? error.message : String(error) };
  }
}

/** Host validates and saves locally; no picker, import, or workflow mutation. */
export async function saveProjectScript(
  projectRoot: string,
  filename: string,
  text: string,
): Promise<SaveProjectScriptResult> {
  try {
    return await invoke<string>("save_project_script", { projectRoot, filename, text });
  } catch (error) {
    return { saveError: error instanceof Error ? error.message : String(error) };
  }
}
