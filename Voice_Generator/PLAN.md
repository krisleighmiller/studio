# Voice Profile Studio — Implementation Plan

## Purpose

Create a standalone local application for creating and managing one voice profile for one character.

The application will support:

1. Recording a user's own voice.
2. Browsing and sampling Atlas Cloud's built-in voices.
3. Designing a voice from structured characteristics and generating several candidates through Atlas Cloud text-to-speech models.
4. Saving the selected audio clips and technical metadata.
5. Updating an existing character reference or creating a new voice metadata record when no character reference exists.

The application itself does not require an LLM or an agent system prompt. The only generative operation is sending structured voice requests to Atlas Cloud text-to-speech models.

## Scope and Constraints

- Atlas Cloud is the only external provider for the initial version.
- The built-in voice catalog must be discovered from Atlas Cloud's current model documentation/API data.
- No voice catalog, model list, or provider capability list will be manually maintained.
- The Atlas API key must remain outside the Tauri webview/UI layer.
- Audio previews and approved clips must be saved locally.
- The application must preserve the exact Atlas model, voice ID, parameters, prompt, and sample text used for every generated clip.
- A voice profile belongs to one character and may contain one or more approved audio clips.
- The human-readable voice description belongs in the character reference when one exists.
- The technical voice metadata belongs in a separate machine-readable profile file.

## Non-Goals

The initial version will not:

- Use an LLM to interview the user.
- Create or revise character personality, backstory, or general character information.
- Support providers other than Atlas Cloud.
- Automatically select a voice without user approval.
- Perform narration production, lip-sync, audio editing, or mixing.
- Treat a generated sample as a persistent provider voice ID unless Atlas explicitly returns one.
- Maintain a hand-written voice catalog.

## High-Level Architecture

```text
Tauri desktop application
  ├── Webview UI
  ├── Recorder
  ├── Atlas voice catalog browser
  ├── Voice sample player
  ├── Voice design form
  ├── Preview comparison
  └── Voice profile editor

Tauri core/backend
  ├── Atlas API client
  ├── Atlas catalog/schema discovery
  ├── Provider/model request translation
  ├── Audio download and local storage
  ├── Catalog and metadata database
  └── Character reference updater

Local storage
  ├── Catalog cache
  ├── Generated previews
  ├── Approved character clips
  └── Voice profile metadata
```

Atlas API calls and filesystem operations should run through Tauri commands or the Tauri core/backend. The Atlas API key must never be exposed to the webview.

## User Workflows

### Workflow A — Record a Voice

1. Select a character or create a new character target.
2. Choose `Recorded voice`.
3. Display a standard recording script.
4. Request microphone permission.
5. Record through a native Tauri command using the host's Linux audio stack (`pw-record`, with `arecord` fallback). The webview's `MediaRecorder` API is not the primary Linux path because WebKitGTK microphone permissions and WebRTC support are unreliable.
6. Allow playback, retake, and approval.
7. Normalize or convert the approved recording into the application's supported format.
8. Save one or more approved clips in the character's voice directory.
9. Create or update the voice profile metadata.
10. Update the character reference with the voice profile path and clip path.

The app should not require a fixed recording length, but should provide a recommended range and show the recorded duration.

### Workflow B — Select a Built-In Atlas Voice

1. Select a character or create a new character target.
2. Choose `Built-in voice`.
3. Load the current Atlas model and voice catalog.
4. Show only models documented as supporting built-in voices.
5. Filter using documented metadata, including where available:
   - provider/model
   - voice name
   - gender
   - language
   - other Atlas-documented fields
6. Display the model's quality, capability, and cost information when available.
7. Choose a standard sample script.
8. Generate samples for selected voices.
9. Cache each generated sample locally.
10. Let the user compare samples in the application.
11. Select one voice.
12. Optionally generate and approve a second canonical sample using the same voice.
13. Save the selected voice ID, model ID, exact parameters, sample text, and audio paths.
14. Update the character reference.

The application must not invent characteristics such as warmth, age, accent, or vocal texture for built-in voices unless Atlas documents them or the application clearly labels them as user-created observations.

### Workflow C — Design a Voice

1. Select a character or create a new character target.
2. Choose `Designed voice`.
3. Collect one shared set of structured voice characteristics:
   - gender
   - language
   - accent or region
   - apparent age
   - pitch
   - vocal texture
   - pace
   - energy
   - emotional baseline
   - user-provided style description
4. Use the approved preset narration text for every candidate.
5. Send the shared voice brief and narration to the fixed initial target set:
   - Seed Audio 1.0
   - xAI TTS v1
   - Gemini 2.5 Pro TTS
   - MiniMax Speech 2.6
6. Translate the shared brief into each model's documented request schema.
7. Save and display one candidate from each target model.
8. Let the user compare and select one.
9. Save the final prompt, settings, model ID, request parameters, and approved audio clips.
10. Add the human-readable voice description to the character reference.
11. Update or create the technical voice profile metadata.

The application must distinguish between:

- A reproducible voice configuration.
- A provider-generated persistent voice profile or voice ID.

The latter must never be claimed unless Atlas returns a reusable identifier.

## Atlas Discovery and Catalog Synchronization

The application will discover Atlas capabilities dynamically.

### Discovery sequence

1. Retrieve the current Atlas text-to-audio model library page.
2. Identify the linked text-to-audio model pages dynamically.
3. Retrieve each current model page and schema/capability information.
4. Identify which models document built-in voices.
5. Retrieve documented voice IDs and supported fields where the model page exposes them.
6. Normalize the data into the local catalog database.
7. Preserve the raw Atlas response for troubleshooting and future schema support.

The sync should run:

- on first application launch;
- when the user requests a refresh;
- optionally at a configurable interval.

The local catalog is a cache of Atlas data, not an independently maintained source of truth.

### Catalog data

The normalized catalog should retain:

- Atlas model ID
- model display name
- media type
- supported source/mode
- built-in voice support
- voice ID
- voice display name
- documented gender
- documented languages
- model-supported parameters
- pricing information, when available
- raw Atlas response reference
- last synchronization timestamp

Unknown provider fields must not be discarded from the raw response.

## Sample Generation

The application should provide standard scripts for comparing voices. A sample script should include:

- conversational speech;
- a longer narration passage;
- a question;
- a sentence with numbers or technical terms;
- a sentence with the character's name.

The user should be able to edit or replace the script before generation.

Every generated clip must record:

- sample text;
- Atlas model ID;
- voice ID, if applicable;
- request parameters;
- prompt, if applicable;
- generation timestamp;
- returned prediction/job ID;
- local output path;
- cost, when returned by Atlas.

Generated samples should be cached and reused when the request is identical.

## Storage Layout

Character assets should remain inside the character's existing directory:

```text
Characters/
  Kris/
    kris_thornvale_ref.md
    voice/
      Kris_voice_profile.yaml
      Kris_voice_sample_01.wav
      Kris_voice_sample_02.wav
      previews/
        ...
```

Application-wide data should be stored separately:

```text
Voice_Generator/
  PLAN.md
  app/
  data/
    voice-studio.db
    atlas-catalog-cache.json
  cache/
    samples/
```

The application will use Tauri. The frontend framework, local database implementation, and exact database location remain open until implementation begins.

## Voice Profile Metadata

Each character should have at most one active voice profile, although that profile may contain multiple approved clips.

Example:

```yaml
profile_version: 1
character: Kris Thornvale
source_mode: built_in_voice
provider: atlascloud
model_id: ""
voice_id: ""
voice_name: ""
language: ""
gender: ""
voice_description: ""
settings: {}
prompt: ""
approved_audio:
  - path: "voice/Kris_voice_sample_01.wav"
    purpose: canonical_sample
    sample_text: ""
  - path: "voice/Kris_voice_sample_02.wav"
    purpose: alternate_sample
    sample_text: ""
created_at: ""
updated_at: ""
```

For recorded voices, provider and model fields may be null.

For designed voices, `voice_description`, `prompt`, and `settings` must preserve the exact reproducible configuration used to generate the approved clip.

## Character Reference Integration

When a character reference exists, update only the voice-related fields:

```markdown
- Voice reference clip: `voice/Kris_voice_sample_01.wav`
- Voice provider: Atlas Cloud
- Voice or profile ID: See `voice/Kris_voice_profile.yaml`
- Asset notes: Canonical voice sample for this character
```

For a designed voice, include the human-readable description in the character reference.

Do not insert the full technical request, raw API response, or preview history into the character reference.

When no character reference exists, save the profile metadata and audio clips, then allow the user to provide or create the character reference later.

## Application Screens

### Character Target

- Select an existing character directory.
- Create a new character target.
- Display the current voice profile, if one exists.

### Voice Source

- Recorded voice
- Built-in Atlas voice
- Designed voice
- Revisit existing voice profile

### Built-In Voice Browser

- Search and filter dynamically discovered voices.
- Group by model.
- Display documented voice metadata.
- Generate, cache, and play samples.
- Compare candidates.
- Select a voice.

### Voice Designer

- Select a compatible Atlas model.
- Display model-specific controls.
- Enter optional descriptive text.
- Generate three to five candidates.
- Compare and select a candidate.

### Recorder

- Select microphone.
- Record.
- Play back.
- Retake.
- Approve.
- Save.

### Profile Review

- Show the selected voice and source mode.
- Show provider/model/voice ID.
- Show voice description.
- Show approved clips.
- Show exact output paths.
- Confirm before writing files.

## Tauri Command Boundaries

The webview should call typed Tauri commands rather than calling Atlas directly. Tauri commands should hide Atlas request schemas, API credentials, local paths, and filesystem operations from the UI.

Likely command groups:

```text
catalog_get
catalog_refresh
voices_list
preview_generate
preview_status
recording_save
characters_list
voice_profile_get
voice_profile_save
character_reference_update
```

The exact command names and payload types are implementation details and may change.

## Safety and Operational Requirements

- Do not expose `ATLASCLOUD_API_KEY` to the Tauri webview.
- Require explicit user action before paid preview generation.
- Display estimated cost when Atlas provides enough information.
- Never overwrite an existing approved clip without confirmation.
- Preserve previous profile versions or provide a rollback mechanism.
- Do not delete recordings or approved clips automatically.
- Clearly label previews versus approved clips.
- Do not claim a persistent voice ID when only a generated audio sample exists.
- Record Atlas job IDs for troubleshooting.

## Implementation Phases

### Phase 1 — Storage and Character Integration

- Establish directory conventions.
- Define the voice profile metadata schema.
- Read existing character directories.
- Update voice fields in existing character references.
- Create profiles for characters without a current profile.

### Phase 2 — Atlas Discovery

- Implement model discovery.
- Implement schema/capability discovery.
- Detect built-in voice support.
- Normalize and cache the catalog.
- Preserve raw Atlas responses.

### Phase 3 — Built-In Voice Browser

- Build dynamic filtering.
- Add standard sample scripts.
- Generate and cache previews.
- Add playback and comparison.
- Save selected voices and metadata.

### Phase 4 — Recorder

- Add native Tauri microphone recording through the Linux audio stack.
- Add playback and retakes.
- Normalize approved recordings.
- Save recordings and profile metadata.

### Phase 5 — Designed Voice

- Implement model-specific structured controls.
- Translate controls into documented Atlas requests.
- Generate three to five candidates.
- Save exact prompts and parameters.
- Add the approved voice description to the character reference.

### Phase 6 — Profile Review and Reliability

- Add profile review before saving.
- Add overwrite protection.
- Add version history or rollback.
- Add cost and job-status reporting.
- Test catalog refresh when Atlas changes model schemas.

## Open Decisions Before Coding

1. Which frontend framework should the Tauri webview use?
2. Where should application-wide data live?
3. What audio format should be canonical for recordings and generated samples?
4. Should the app support importing an existing character directory by path?
5. How should failed or incomplete Atlas jobs be represented?
6. How many previous profile versions should be retained?
7. Should previews expire automatically, or remain cached until manually deleted?
8. Should recording use the webview `MediaRecorder` API first, or begin with a native Tauri audio plugin?

## Definition of Done

The first usable version is complete when a user can:

1. Select or create a character target.
2. Discover the current Atlas built-in voice catalog without editing application source data.
3. Filter and preview documented built-in voices.
4. Record and approve a personal voice clip.
5. Design a voice using model-supported controls.
6. Generate and compare multiple designed-voice candidates.
7. Select one voice.
8. Save one or more clips and complete technical metadata.
9. Update an existing character reference with the voice profile information.
10. Reopen the application and revisit the saved profile without losing its Atlas settings or audio assets.
