# Studio — Current workflow guide

F1 and **Help → Documentation** open this bundled guide locally. Reading help, manually editing/saving scripts, importing files and saving character profiles do not call a model provider. Generate script from notes is a separate opt-in text request with native confirmation. Media generation is separate and requires the existing approvals.

## Find the workflow you need

- [1. Start a script from an idea or notes](#recovery-1)
- [2. Import an external script](#recovery-2)
- [3. Find the file editor](#recovery-3)
- [4. Add a character reference image](#recovery-4)
- [5. Create or update a model sheet](#recovery-5)
- [6. Register a file already in the project](#recovery-6)
- [7. Recover when instructions do not match](#recovery-7)

Open a folder with **File → Open Project…** (`Ctrl+O`). It opens in **Studio**. Studio's center is **Production workflow**, not the file editor. Plan, Produce and Finish organize the recipe stages; select a scene when scenes exist. Script entry controls are also available beside the scene list. Jobs records work separately from Director chat.

<h2 id="recovery-1">1. Start a script from an idea or notes</h2>

In Studio's **Plan** area choose **Create script**. A blank project shows **Start your script** instead of requiring a scene first.

1. Enter **Title** (used for the saved filename), then edit **Notes** and **Script**.
2. **Load notes** reads a local `.md`, `.markdown`, or `.txt` file into Notes only, preserving your existing Script. Typed notes and file-loaded notes use the same next step; typing never calls a model.
3. For creative expansion, select your configured text/chat model in the chat model dropdown (manage APIs and pinned text models in **Settings**), then choose **Generate script from notes**. The native confirmation identifies the exact API/model, endpoint and Title/Notes being disclosed, unknown provider cost, and whether it will replace an existing editable Script. **Cancel** that confirmation to keep the draft without sending. There is no fallback provider.
4. Review and edit **Script**, then choose **Save script**. The host validates the text and saves it under `Scripts/`, rescans, and selects a scene from that script. **Cancel** closes the draft without saving it.

Generation returns an unsaved editable screenplay with Scene N headings; it never writes project files, generates images/media or uses the retired Script Assistant. Missing configuration, provider errors or invalid output retain your input and manual script. You can also write Script manually and Save without a model.

**If saving fails:** keep the draft open, read the error, supply nonempty Script text with a Scene N heading and a safe filename, then retry. An existing different file is not silently overwritten; choose another Title. Do not delete project state to fix a script.

### Supported script format and working example

Script save/import/registration supports **.md, .markdown and .txt** (case-insensitive). PDF, DOCX and Fountain are not supported by these controls; export their contents as one of the supported text formats first. Scenes require numbered **Scene N** headings; Markdown heading markers are optional. An ordinary title or screenplay slugline alone is not a scene heading.

Save this example as `two-friends-saying-hello.md`, or paste it into Script:

```text
# Two friends

## Scene 1 — Hello
- **Location:** A quiet doorway
- **Action:** Alice waves to Bob.
- **Dialogue:**
  - **Alice:** Hello, Bob!
  - **Bob:** Hello, Alice!

## Scene 2 — Goodbye
- **Action:** The friends wave goodbye.
- **Dialogue:**
  - **Bob:** See you soon!
```

If there are no scenes after a save/import, check those headings in **Files**, save the corrected text, then use [project-file registration](#recovery-6) to rescan. The parser does not need an API key.

<h2 id="recovery-2">2. Import an external script</h2>

Choose **Import script** in Plan or beside the scene list. Pick one supported text file from disk. This copies/registers it under the open project's `Scripts/`, refreshes the workflow and selects a scene from that script. The original source file stays unchanged. Reimporting identical bytes reuses the existing file; a different file with the same destination name is refused rather than overwriting it.

**If nothing appears:** check for Scene N headings, a supported extension and readable file/project permissions. Correct the file and retry; do not repeatedly import an unchanged invalid file. Canceling the picker leaves the workflow unchanged. If the project changed while choosing a file, choose again in the intended project.

**Open File…** is not Import script: opening an editor tab neither copies the file into `Scripts/` nor registers scenes.

<h2 id="recovery-3">3. Find the file editor</h2>

Select **Files** on the left rail. Its center heading is **File editor** and it contains Explorer, editor tabs and text/media viewers. Use **File → Open File…** (`Ctrl+Shift+O`) or the visible **Open** button in the Explorer header to pick an existing file. Open a project first; `Ctrl+O` remains **Open Project…**. Opening a file shows its tab in Files; it is not a Studio import. The right-click Open File paths remain available too.

**File → New File** (`Ctrl+N`) creates a separate untitled text buffer without replacing existing tabs. Save it with **Save** (`Ctrl+S`) or **Save As…** (`Ctrl+Shift+S`). Explorer's existing New File button instead names/creates a file at the project root. **File → Close File** (`Ctrl+W`) closes the active editor tab, retaining the existing unsaved-change confirmation. Cancel keeps that tab and its contents. **Studio** returns to Production workflow; switching surfaces preserves open file tabs and unsaved text.

**If the center will not accept text:** you may be looking at Studio's read-only script beats. Use **Edit script** there or switch to Files and open the actual source. Saving a supported direct child of the project's `Scripts/` refreshes Studio automatically while retaining source identity; failed refresh reports that the disk save succeeded but Studio still shows its last imported version. Opening arbitrary files, or saving outside that supported Scripts location, does not register scenes. Use [Use as project script](#recovery-6) when registration is needed; it reads saved disk bytes rather than an unsaved editor buffer.

<h2 id="recovery-4">4. Add a character reference image</h2>

Open **Characters** in Plan. Select a character or start a new character profile. Fill **Character name**, **Description**, **Appearance** and **Identity prompt** as needed. **Create character** saves a new profile; **Save profile** saves edits to an existing one. These are local controls, not a character-writing chat interview.

Choose **Add reference image** under **Reference images**, then select a PNG, JPG or JPEG. For a new unsaved character the selection is held in the draft until Create character; an existing character's selection is acquired into its project directory and saved. Uploaded references guide appearance; they are not generated model-sheet outputs or proof of identity readiness.

**If the image is missing:** check the image format/readability, retry the picker and save the new character. Canceling the picker adds nothing. **Remove** (accessible name “Remove reference image” plus filename) removes that selection without deleting the original source image. Do not attach photos to Director as a substitute for saving the character's reference selection.

<h2 id="recovery-5">5. Create or update a model sheet</h2>

In **Characters**, a saved visual **Description** or **Appearance**, selected reference images, or both are sufficient for Create and Update. **Identity prompt** is optional additional visual wording; a full identity sheet, biography, personality, continuity notes or previously generated identity is not required. Save a named character once to associate outputs with its folder, and save any actual unsaved input/reference edits before retrying. Choose **Create full 12-pose sheet**, or **Update full 12-pose sheet** when a pack already exists. The full pack contains three neutral closeups, four standing turnarounds, one front T-pose, and four expression closeups. This directly prepares the typed model-sheet operation from the saved character and references; it does not populate or send Director chat.

The operation uses image generation defaults in **Settings**. **Create full 12-pose sheet** / **Update full 12-pose sheet** keeps approval per image. **Generate entire 12-pose sheet (one approval)** instead asks once for up to 12 separate image calls for the saved character, with unknown/unbounded total cost disclosed; it then advances through unfinished poses without another dialog for each image. A text-only pack still generates its first identity plate before conditioning the rest on that saved image. Every provider request still receives a fresh exact one-use host token; this is not one multi-pose image or an authorization to retry failures or switch models. **Do not approve if you only want to inspect the request.** Decline/cancel to stop before execution. Generation may upload references and incur provider charges only after the required permissions.

**If blocked:** a new/dirty draft reports Save required; use Create character or Save profile before retrying. If there is no visual input, add a visual Description/Appearance or select a reference image and save. Check selected references and image-model defaults when validation fails. Read the native error rather than resending a chat prompt. Pending/submitted work is not a saved still. Review saved outputs in the character's Model sheet gallery; each saved photo has a **Delete** control that asks for confirmation, moves only that photo to the project's recoverable trash, and reopens its shot for later regeneration. **Open model sheet manifest** is available once a manifest exists. Updates use the existing runner's continuation/revision behavior, not a new generation framework.

<h2 id="recovery-6">6. Register a file already in the project</h2>

A root-level `two-friends-saying-hello.md` can be visible in Files while absent from Plan: automatic discovery scans **Scripts/**, not arbitrary files at the project root.

- In **Files**, right-click the supported project file and choose **Use as project script**.
- Or choose **Use existing project file** in Plan/beside the scene list and select the file inside the open project.

The host copies/registers supported files into `Scripts/` without changing the source; files already in Scripts/ are reused. It then rescans and selects a scene from that source, not an unrelated scene. This is registration, not a destructive move or a single-script replacement of all other scripts.

**If Plan is empty:** add Scene N headings, save disk edits in Files, and repeat registration. A file outside the open project must use Import script instead. If refresh fails after saving/copying, keep the saved file, fix access or formatting and register it again (or reopen the project to rescan). Deleting a script file or removing a Scene N heading hides the affected scenes from the active workflow after refresh; their saved production work and source identity remain in workflow state for recovery if the source is restored. Deleting a scene prompt folder likewise hides its scene while retaining its prior work. Do not erase state or manually manufacture scene records.

<h2 id="recovery-7">7. Recover when instructions do not match</h2>

Press **F1** from Studio or Files to reopen this current bundled guide; the existing Documentation tab is reused. No network or chat model is needed. Use the links above for the six workflow recovery paths and this seventh help-recovery path.

The legacy prompt-only **Script Assistant**, **Character Creation** and **Production Helper** routes are **retired**. Use Create script/Import script/Use as project script and the Characters profile/reference controls instead. Old prompt Markdown files may remain as historical documents; they are not runtime contracts. Old architecture-specific instructions are not the current workflow. Model sheets use the direct Create full 12-pose sheet/Update full 12-pose sheet controls; do not seek an identity-building chat handoff. Scene planning and retained media operations use the current host-owned paths and existing permissions, not the archived specialist prompt files.

If help and the visible controls still disagree, record the control label, selected Studio/Files mode, project-relative file path and exact error for a bug report. Do not troubleshoot by deleting project state, restoring retired routes or approving paid generation. Notes never trigger automatic rewriting merely because a chat model is configured: Generate requires explicit native approval, and saving is a separate action.
