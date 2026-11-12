import { invoke, openDialog } from "../../host";

export type ScriptNotesResult = { sourcePath: string; text: string } | "cancelled" | { readError: string };

/** Acquire local notes only; no project mutation or draft generation. */
export async function loadScriptNotes(): Promise<ScriptNotesResult> {
  let sourcePath: string | string[] | null = null;
  try {
    sourcePath = await openDialog({
      title: "Load Script Notes", directory: false, multiple: false,
      filters: [{ name: "Script notes", extensions: ["md", "markdown", "txt"] }],
    });
    if (!sourcePath) return "cancelled";
    if (Array.isArray(sourcePath)) return { readError: "Select exactly one notes file." };
    if (!/\.(md|markdown|txt)$/i.test(sourcePath)) {
      return { readError: "Choose a .md, .markdown, or .txt notes file." };
    }
    const text = await invoke<string>("read_text_file", { path: sourcePath });
    return { sourcePath, text };
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    return {
      readError: typeof sourcePath === "string"
        ? `Could not read "${sourcePath}". Check that the file exists and is readable, then try again. ${detail}`
        : `Could not open the notes picker. Try again. ${detail}`,
    };
  }
}
