//! Scene-plan handoff: typed parsing, validation, timing, rendering and a
//! transactional commit for LLM-planned cuts. The LLM owns the creative
//! content; this module owns the YAML contract, the arithmetic, asset
//! discovery, and the files.
pub mod assets;
pub mod commit;
pub mod config;
pub mod coverage;
pub mod emit_parse;
pub mod error;
pub mod extract;
pub mod fixtures;
pub mod generation;
pub mod handoff;
pub mod migrate;
pub mod redo;
pub mod render;
pub mod resolve;
pub mod state;
pub mod strategy;
pub mod timing;
pub mod validate;

pub use assets::AssetRegistry;
pub use commit::{
    commit_model_chunk, plan_chunk, CommitReport, LaneMix, ModelEmit, PlanPaths, PlannedChunk,
};
pub use config::{HostConfig, LaneModels, RenderDurationPolicy};
pub use emit_parse::{parse_scene_plan_emit, EmitOutcome, NoFilesReason, Recovery};
pub use error::{retry_feedback, CommitError, Issue, IssueKind};
pub use generation::RenderRequest;
pub use handoff::{CutId, RenderLane, SceneHandoff};
pub use migrate::Reconcile;
pub use redo::parse_redo;
pub use state::{PlanSection, PlanState, PlanStatus, SectionStatus};
pub use strategy::{SceneStrategy, StrategyChoice};
pub use validate::ValidatedScene;

use crate::chat;
use crate::ctx::HostCtx;
use crate::director::DirectorContext;
use crate::director_actions::{
    encode_pending_action, resolve_under_project, DirectorAction, ValidatedAction,
};
use crate::scene_plan::redo::CutContext;
use crate::scene_plan::state::sidecar_root_from;
use crate::script::{
    aspect_from_text, extract_dialogue_text, extract_scene_section, requested_scene_number,
    section_title,
};
use crate::specialists::{
    build_text_specialist_dossier, capability_model, collect_text_specialist_paths, input_str,
    SpecialistLlm, SpecialistResult,
};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

const STATE_NAME: &str = "scene_plan_state.json";
const MAX_CORRECTION_ATTEMPTS: usize = 2;
const MAX_CUTS_PER_TURN: usize = 3;
const RAW_REPLY_EXCERPT_CHARS: usize = 1_500;

/// Everything a turn needs before talking to the model.
struct Turn {
    brief: String,
    source_scene: u32,
    plan_paths: PlanPaths,
    host_notes: Vec<String>,
    registry: AssetRegistry,
    config: HostConfig,
    state: PlanState,
    section_index: usize,
    source: SourceContext,
    source_spoken: String,
    strategy: StrategyChoice,
    read_labels: Vec<String>,
    missing: Vec<String>,
    dossier: String,
}

pub fn run(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    llm: &SpecialistLlm<'_>,
) -> Result<SpecialistResult, String> {
    let turn = prepare_turn(app, context, validated, project_root, llm)?;
    match parse_redo(&validated.action.inputs, &turn.brief)? {
        Some(cuts) => run_redo(app, llm, turn, &cuts),
        None => run_plan(app, validated, llm, turn),
    }
}

fn plan_paths(
    inputs: &serde_json::Map<String, serde_json::Value>,
    project_root: Option<&str>,
    source_scene: u32,
) -> Result<(PlanPaths, Vec<String>), String> {
    let mut notes = Vec::new();
    let prod_rel = input_str(
        inputs,
        &[
            "production_path",
            "production",
            "prod_path",
            "output_path",
            "save_path",
        ],
    )
    .unwrap_or("prod.md");
    let prod_md = resolve_under_project(prod_rel, project_root).map_err(|e| e.to_string())?;
    let sidecar_rel = input_str(
        inputs,
        &[
            "sidecar_root",
            "sidecar_dir",
            "sidecar_folder",
            "scene_prompts_dir",
            "prompts_dir",
        ],
    )
    .unwrap_or("Scene_Prompts");
    let requested = resolve_under_project(sidecar_rel, project_root).map_err(|e| e.to_string())?;
    let (sidecar_root, stripped) = sidecar_root_from(&requested);
    if let Some(n) = stripped {
        notes.push(format!(
            "sidecar path `{}` ends in Scene{n}; the host owns the per-scene folder, so sidecars are written to `{}/CutM.txt`",
            requested.display(),
            sidecar_root.join(format!("Scene{source_scene}")).display()
        ));
    }
    let state = if let Some(raw) = input_str(inputs, &["plan_state"]) {
        resolve_under_project(raw, project_root).map_err(|e| e.to_string())?
    } else {
        prod_md
            .parent()
            .map(|p| p.join(STATE_NAME))
            .unwrap_or_else(|| PathBuf::from(STATE_NAME))
    };
    Ok((
        PlanPaths {
            prod_md,
            sidecar_root,
            state,
        },
        notes,
    ))
}

/// Continuation carries the *normalized* paths so later turns are consistent.
fn pending_write_action(
    validated: &ValidatedAction,
    paths: &PlanPaths,
    source_scene: u32,
) -> DirectorAction {
    let mut action = validated.action.clone();
    action.inputs.insert(
        "plan_state".into(),
        json!(paths.state.to_string_lossy().to_string()),
    );
    action
        .inputs
        .insert("source_scene".into(), json!(source_scene.to_string()));
    action.inputs.insert(
        "production_path".into(),
        json!(paths.prod_md.to_string_lossy().to_string()),
    );
    action.inputs.insert(
        "sidecar_dir".into(),
        json!(paths.sidecar_root.to_string_lossy().to_string()),
    );
    action
}

fn cut_range(cuts: &[CutId]) -> String {
    match (cuts.first(), cuts.last()) {
        (Some(a), Some(b)) if a == b => format!("Cut {}", a.cut),
        (Some(a), Some(b)) => format!("Cuts {}–{}", a.cut, b.cut),
        _ => "no cuts".to_string(),
    }
}

fn prepare_turn(
    app: &HostCtx,
    context: &DirectorContext,
    validated: &ValidatedAction,
    project_root: Option<&str>,
    llm: &SpecialistLlm<'_>,
) -> Result<Turn, String> {
    let inputs = &validated.action.inputs;
    let brief = input_str(inputs, &["brief", "prompt", "question", "message"])
        .unwrap_or(llm.user_question)
        .trim()
        .to_string();
    if brief.is_empty() {
        return Err("`scene_plan` needs a brief/prompt (or a non-empty user message).".into());
    }
    let source_scene = input_str(inputs, &["source_scene", "scene"])
        .and_then(|s| s.parse::<u32>().ok())
        .or_else(|| requested_scene_number(&brief))
        .unwrap_or(1);
    let (plan_paths, mut host_notes) = plan_paths(inputs, project_root, source_scene)?;
    let paths = collect_text_specialist_paths(context, inputs);
    app.emit(
        "director-progress",
        format!("Scene Plan reading {} file(s)...", paths.len()),
    );
    let (dossier, read_labels, missing) =
        build_text_specialist_dossier(&paths, project_root, context);
    let source = source_context(&brief, &paths, context, project_root);
    let source_spoken = input_str(inputs, &["source_spoken", "dialogue", "spoken"])
        .map(str::to_string)
        .or_else(|| source.dialogue_text.clone())
        .unwrap_or_default();
    let aspect = input_str(inputs, &["aspect_ratio", "format"])
        .map(str::to_string)
        .or_else(|| aspect_from_text(&brief))
        .or_else(|| aspect_from_text(&dossier));
    let strategy = StrategyChoice::choose(
        input_str(inputs, &["strategy", "scene_strategy", "shot_strategy"]),
        &brief,
    );

    let registry = match project_root {
        Some(root) => AssetRegistry::discover(Path::new(root)),
        None => AssetRegistry::default(),
    };
    app.emit(
        "director-progress",
        format!(
            "Scene Plan found {} character(s), {} lip-sync ready: {}; strategy {}",
            registry.characters.len(),
            registry
                .characters
                .iter()
                .filter(|c| c.lipsync_ready())
                .count(),
            registry
                .characters
                .iter()
                .filter(|c| c.lipsync_ready())
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            strategy.describe()
        ),
    );
    let lane_models = LaneModels {
        text_to_video: capability_model(context, "video.text_only").map(|m| m.model_id),
        image_to_video: capability_model(context, "video.start_frame").map(|m| m.model_id),
        reference_to_video: capability_model(context, "video.character_reference")
            .or_else(|| capability_model(context, "video.location_prop_other_reference"))
            .map(|m| m.model_id),
        audio_driven_i2v: capability_model(context, "video.lip_sync_text_narration")
            .map(|m| m.model_id),
    };
    let config = HostConfig::default().with_lane_models(&lane_models);

    // --- state: load, migrate, reconcile with disk, persist if anything moved
    let (mut state, state_notes, migrated) = read_or_create_state(&plan_paths, aspect)?;
    host_notes.extend(state_notes);
    let rec = migrate::reconcile(&plan_paths, &mut state);
    host_notes.extend(rec.summary_lines());
    if migrated || rec.changed {
        commit::write_atomic(&plan_paths.state, state.to_json()?.as_bytes())
            .map_err(|e| format!("could not persist reconciled scene_plan state: {e}"))?;
        host_notes.push(format!(
            "state reconciled and saved to `{}`{}",
            plan_paths.state.display(),
            if rec.clean() {
                ""
            } else {
                " (conflicts remain, see above)"
            }
        ));
    }
    let title = source
        .title
        .clone()
        .unwrap_or_else(|| format!("Scene {source_scene}"));
    let section_index = state.ensure_section(source_scene, &title, &title);
    plan_paths
        .check(&state, section_index)
        .map_err(|e| format!("Scene Plan refused to continue: {e}"))?;
    for note in &host_notes {
        app.emit("director-progress", format!("Scene Plan: {note}"));
    }
    Ok(Turn {
        brief,
        source_scene,
        plan_paths,
        host_notes,
        registry,
        config,
        state,
        section_index,
        source,
        source_spoken,
        strategy,
        read_labels,
        missing,
        dossier,
    })
}

fn read_or_create_state(
    paths: &PlanPaths,
    aspect: Option<String>,
) -> Result<(PlanState, Vec<String>, bool), String> {
    if paths.state.is_file() {
        let text = fs::read_to_string(&paths.state)
            .map_err(|e| format!("Could not read {}: {e}", paths.state.display()))?;
        let (state, notes) = PlanState::load(&text).map_err(|e| {
            format!(
                "Could not read scene_plan state at {}: {e}",
                paths.state.display()
            )
        })?;
        let migrated = !notes.is_empty();
        return Ok((state, notes, migrated));
    }
    Ok((
        PlanState::new(&paths.sidecar_root, aspect),
        Vec::new(),
        false,
    ))
}

fn run_plan(
    app: &HostCtx,
    validated: &ValidatedAction,
    llm: &SpecialistLlm<'_>,
    mut turn: Turn,
) -> Result<SpecialistResult, String> {
    let section_index = turn.section_index;
    if turn.state.sections[section_index].status == SectionStatus::Done {
        return Ok(SpecialistResult::done(already_planned_message(
            &turn.state.sections[section_index],
        )));
    }
    turn.state.sections[section_index].status = SectionStatus::InProgress;
    if turn.source_spoken.trim().is_empty() {
        app.emit("director-progress", "Scene Plan: no dialogue found in the source section; cuts will be planned as silent/b-roll.".to_string());
    }
    let user_block = scene_plan_user_block(&turn);
    let system = format!(
        "{}\n\n---\n\n{}",
        scene_plan_host_preamble(),
        scene_plan_contract()
    );
    let mut messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user_block }),
    ];
    let mut last_failure = String::new();

    for attempt in 0..=MAX_CORRECTION_ATTEMPTS {
        app.emit(
            "director-progress",
            format!(
                "Scene Plan thinking (attempt {} of {})...",
                attempt + 1,
                MAX_CORRECTION_ATTEMPTS + 1
            ),
        );
        let reply = chat::chat_completion(llm.api, llm.api_key, llm.model, &messages)?;
        let outcome = parse_scene_plan_emit(&reply);
        app.emit(
            "director-progress",
            format!(
                "Scene Plan reply: {} chars; parsed as {}",
                reply.chars().count(),
                outcome.describe()
            ),
        );
        let (files, summary, recovery) = match outcome {
            EmitOutcome::Files {
                files,
                summary,
                recovery,
            } => (files, summary, recovery),
            EmitOutcome::Ask(question) => {
                last_failure =
                    format!("the model asked a question instead of writing files:\n{question}");
                if attempt < MAX_CORRECTION_ATTEMPTS {
                    push_correction(&mut messages, &reply, ASK_NUDGE);
                    continue;
                }
                return Ok(SpecialistResult::done(format!("Scene Plan asked a question instead of writing files. Nothing was written.\n\n{question}")));
            }
            EmitOutcome::NoFiles(reason) => {
                last_failure = reason.describe();
                if attempt < MAX_CORRECTION_ATTEMPTS {
                    push_correction(&mut messages, &reply, &reason.nudge());
                    continue;
                }
                return Err(no_files_report(&reason, &reply, attempt + 1));
            }
        };
        match commit_model_chunk(
            &turn.plan_paths,
            &turn.config,
            &mut turn.state,
            section_index,
            &turn.source_spoken,
            Some(&turn.registry),
            &ModelEmit { files },
        ) {
            Ok(report) => {
                let mut message = commit_message(
                    summary,
                    &report,
                    &turn.plan_paths,
                    recovery.as_ref(),
                    &turn.host_notes,
                    Some(turn.strategy),
                );
                if report.section_done {
                    return Ok(SpecialistResult::done(message));
                }
                let pending = pending_write_action(validated, &turn.plan_paths, turn.source_scene);
                message.push('\n');
                message.push_str(&encode_pending_action(&pending));
                return Ok(SpecialistResult::cont(message));
            }
            Err(CommitError::Rejected(issues)) => {
                last_failure = retry_feedback(&issues);
                if attempt < MAX_CORRECTION_ATTEMPTS {
                    push_correction(&mut messages, &reply, &format!("Workbench host rejected that chunk; nothing was written. Fix only these issues and emit the corrected <workbench_file path=\"prod.md\"> again:\n\n{}", retry_feedback(&issues)));
                    continue;
                }
                return Err(format!("Scene Plan: the model's chunk was rejected on all {} attempt(s); nothing was written.\n\n{}", attempt + 1, retry_feedback(&issues)));
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    Err(format!(
        "Scene Plan stopped without committing a chunk. Last failure:\n{last_failure}"
    ))
}

fn run_redo(
    app: &HostCtx,
    llm: &SpecialistLlm<'_>,
    mut turn: Turn,
    cuts: &[u32],
) -> Result<SpecialistResult, String> {
    let section_index = turn.section_index;
    let prod_text = fs::read_to_string(&turn.plan_paths.prod_md).map_err(|e| {
        format!(
            "Scene {} has no master file to redo cuts in ({}): {e}",
            turn.source_scene,
            turn.plan_paths.prod_md.display()
        )
    })?;
    if redo::ensure_cut_coverage(
        &mut turn.state.sections[section_index],
        &prod_text,
        &turn.source_spoken,
    )? {
        commit::write_atomic(&turn.plan_paths.state, turn.state.to_json()?.as_bytes())?;
        app.emit(
            "director-progress",
            format!(
                "Scene Plan: reconstructed per-cut dialogue coverage for Scene {} from the master",
                turn.source_scene
            ),
        );
    }
    let contexts = redo::cut_contexts(
        &turn.state.sections[section_index],
        &prod_text,
        &turn.source_spoken,
        cuts,
    )?;
    let user_block = redo_user_block(&turn, &contexts);
    let system = format!(
        "{}\n\n---\n\n{}",
        scene_plan_host_preamble(),
        scene_plan_contract()
    );
    let mut messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user_block }),
    ];
    let labels = contexts
        .iter()
        .map(|c| c.id.code())
        .collect::<Vec<_>>()
        .join(", ");

    for attempt in 0..=MAX_CORRECTION_ATTEMPTS {
        app.emit(
            "director-progress",
            format!(
                "Scene Plan redoing {labels} (attempt {} of {})...",
                attempt + 1,
                MAX_CORRECTION_ATTEMPTS + 1
            ),
        );
        let reply = chat::chat_completion(llm.api, llm.api_key, llm.model, &messages)?;
        let outcome = parse_scene_plan_emit(&reply);
        let (files, summary, recovery) = match outcome {
            EmitOutcome::Files { files, summary, recovery } => (files, summary, recovery),
            EmitOutcome::Ask(question) if attempt < MAX_CORRECTION_ATTEMPTS => {
                push_correction(&mut messages, &reply, ASK_NUDGE);
                let _ = question;
                continue;
            }
            EmitOutcome::Ask(question) => return Ok(SpecialistResult::done(format!("Scene Plan asked a question instead of redoing {labels}. Nothing was written.\n\n{question}"))),
            EmitOutcome::NoFiles(reason) if attempt < MAX_CORRECTION_ATTEMPTS => {
                push_correction(&mut messages, &reply, &reason.nudge());
                continue;
            }
            EmitOutcome::NoFiles(reason) => return Err(no_files_report(&reason, &reply, attempt + 1)),
        };
        match redo::commit_redo(&turn.plan_paths, &turn.config, &mut turn.state, section_index, &turn.source_spoken, Some(&turn.registry), cuts, &ModelEmit { files }) {
            Ok(report) => return Ok(SpecialistResult::done(commit_message(summary, &report, &turn.plan_paths, recovery.as_ref(), &turn.host_notes, Some(turn.strategy)))),
            Err(CommitError::Rejected(issues)) if attempt < MAX_CORRECTION_ATTEMPTS => {
                push_correction(&mut messages, &reply, &format!("Workbench host rejected that redo; nothing was written. Fix only these issues and re-emit exactly {labels}:\n\n{}", retry_feedback(&issues)));
            }
            Err(CommitError::Rejected(issues)) => return Err(format!("Scene Plan: the redo of {labels} was rejected on all {} attempt(s); nothing was written.\n\n{}", attempt + 1, retry_feedback(&issues))),
            Err(err) => return Err(err.to_string()),
        }
    }
    Err(format!("Scene Plan stopped without redoing {labels}."))
}

fn redo_user_block(turn: &Turn, contexts: &[CutContext]) -> String {
    let section = &turn.state.sections[turn.section_index];
    let mut out = String::new();
    out.push_str("## Specialist\nScene Plan (`scene_plan`) — REDO MODE\n\n");
    out.push_str("## User request\n");
    out.push_str(&turn.brief);
    out.push_str("\n\n## Host instructions\n");
    out.push_str(&format!(
        "Re-emit **exactly** these blocks of source Scene {}, in this order, and nothing else: {}. Keep each cut's `source_scene`/`cut`. Each cut must carry exactly the dialogue listed below — word for word — but *how* it is staged is yours to change (lip-sync, voice-over over a different image, silent insert, new framing). Use the user's direction above. Do not invent model ids, output paths, durations, or reference asset paths — the host fills those.\n\n",
        section.source_scene,
        contexts.iter().map(|c| format!("`### {}`", c.id)).collect::<Vec<_>>().join(", ")
    ));
    out.push_str(&format!(
        "## Scene strategy: {}\n{}\n\n",
        turn.strategy.describe(),
        turn.strategy.strategy.guidance()
    ));
    out.push_str(&provider_buckets_block(&turn.config));
    out.push_str(&turn.registry.prompt_block());
    out.push('\n');
    for c in contexts {
        out.push_str(&format!("## {}\n", c.id));
        out.push_str(&format!(
            "Dialogue this cut must carry: {}\n",
            if c.dialogue.is_empty() {
                "none (silent cut — no cues)".to_string()
            } else {
                format!("\"{}\"", c.dialogue)
            }
        ));
        if let Some(p) = &c.prev_closing_state {
            out.push_str(&format!("Previous cut ends with: {p}\n"));
        }
        if let Some(n) = &c.next_opening_state {
            out.push_str(&format!("Next cut opens with: {n}\n"));
        }
        out.push_str("Current handoff (replace it):\n```yaml\n");
        out.push_str(&c.previous_yaml);
        out.push_str("\n```\n\n");
    }
    if let Some(section_text) = turn.source.section_text.as_deref() {
        out.push_str("## Source section\n\n```markdown\n");
        out.push_str(section_text);
        out.push_str("\n```\n\n");
    }
    out.push_str("## Output reminder\nEmit one `<workbench_file path=\"prod.md\">` containing only the blocks named above, then `<workbench_summary>`.\n");
    out
}

fn provider_buckets_block(config: &HostConfig) -> String {
    let mut out = String::from("## Provider render buckets (host-enforced)\n");
    for lane in [
        RenderLane::TextToVideo,
        RenderLane::ImageToVideo,
        RenderLane::ReferenceToVideo,
        RenderLane::AudioDrivenI2v,
    ] {
        let model = config.lane_models.get(lane).unwrap_or("default");
        out.push_str(&format!(
            "- `{}` ({model}): {}\n",
            handoff::label(&lane),
            describe_policy(config.policy_for(lane))
        ));
    }
    out.push_str("The host snaps each cut to a bucket of the resolved lane. Split a cut if the spoken line cannot fit the largest bucket.\n\n");
    out
}

fn scene_plan_user_block(turn: &Turn) -> String {
    let section = &turn.state.sections[turn.section_index];
    let next = section.next_cut_id();
    let paths = &turn.plan_paths;
    let mut out = String::new();
    out.push_str("## Specialist\nScene Plan (`scene_plan`)\n\n");
    out.push_str("## User request\n");
    out.push_str(&turn.brief);
    out.push_str("\n\n## Host instructions\n");
    out.push_str(&format!(
        "You are planning generated cuts of **source Scene {}**. Head each block `### Scene {} — Cut M` and set `source_scene: {}` plus `cut: M`; the first block this turn is `### {next}`. Plan at most {MAX_CUTS_PER_TURN} cuts in this reply; the host will call you again for the rest. Do not ask questions. Emit exactly one `<workbench_file path=\"prod.md\">`. Do not emit sidecar files.\n\n",
        section.source_scene, section.source_scene, section.source_scene
    ));
    out.push_str(&format!("## Scene strategy: {}\n{}\nLip-sync readiness below is a capability, not an instruction: choose per line whether the performing face is on camera (lip-sync), the line plays over something else (voice_over), or the beat is silent.\n\n", turn.strategy.describe(), turn.strategy.strategy.guidance()));
    out.push_str(&format!(
        "Master production file: `{}`\n",
        paths.prod_md.display()
    ));
    out.push_str(&format!(
        "Sidecars rendered by host: `{}/CutM.txt`\n",
        paths.sidecar_dir_for(section.source_scene).display()
    ));
    if let Some(aspect) = turn.state.aspect_ratio.as_deref() {
        out.push_str(&format!("Production aspect ratio: `{aspect}`\n"));
    }
    out.push('\n');
    out.push_str(&provider_buckets_block(&turn.config));
    out.push_str(&turn.registry.prompt_block());
    out.push('\n');
    if let Some(section_text) = turn.source.section_text.as_deref() {
        out.push_str("## Source section to cover\n\n```markdown\n");
        out.push_str(section_text);
        out.push_str("\n```\n\n");
    }
    if !turn.source_spoken.trim().is_empty() {
        let remaining = coverage::remaining_text(&turn.source_spoken, section.covered_word_count);
        out.push_str("## Source dialogue still to cover — quote verbatim, in order, starting with the first line\n\n```text\n");
        out.push_str(remaining.trim());
        out.push_str("\n```\n\n");
    } else {
        out.push_str(
            "## Source dialogue\nNone detected. Plan silent cuts (`speech.mode: none`).\n\n",
        );
    }
    if !turn.read_labels.is_empty() {
        out.push_str("## Files provided by host\n");
        for item in &turn.read_labels {
            out.push_str(&format!("- `{item}`\n"));
        }
        out.push('\n');
    }
    if !turn.missing.is_empty() {
        out.push_str("## Paths that could not be loaded\n");
        for item in &turn.missing {
            out.push_str(&format!("- {item}\n"));
        }
        out.push('\n');
    }
    out.push_str("## Full dossier\n\n");
    out.push_str(&turn.dossier);
    out.push_str(&format!(
        "\n## Output reminder\nEmit one `<workbench_file path=\"prod.md\">` … `</workbench_file>` with `### Scene {} — Cut M` blocks (at most {MAX_CUTS_PER_TURN}, starting at Cut {}), each with exactly one fenced YAML handoff, then `<workbench_summary>`.\n",
        section.source_scene, section.next_cut
    ));
    out
}

fn already_planned_message(section: &PlanSection) -> String {
    format!(
        "Scene {} is already fully planned: {} cut(s) ({}) in `{}`, sidecars under `{}`. Its dialogue is fully covered ({} words). To change one cut, ask `redo cut X` with your direction; to re-plan the scene, remove its cuts from the master and sidecar folder and delete the Scene {} section from the state.",
        section.source_scene,
        section.cuts.len(),
        cut_range(&section.cut_ids()),
        section.production_path.as_deref().unwrap_or("(master file unknown)"),
        section.sidecar_dir.as_deref().unwrap_or("(unknown)"),
        section.covered_word_count,
        section.source_scene
    )
}

fn commit_message(
    summary: Option<String>,
    report: &CommitReport,
    paths: &PlanPaths,
    recovery: Option<&Recovery>,
    host_notes: &[String],
    strategy: Option<StrategyChoice>,
) -> String {
    let scene = report.source_scene;
    let mut m = summary.unwrap_or_else(|| {
        if report.redo {
            format!(
                "Scene Plan replaced {} of Scene {scene} ({}).",
                cut_range(&report.cuts),
                report.mix
            )
        } else {
            format!(
                "Scene Plan committed {} cut(s) for Scene {scene} ({}; mix {}).",
                report.cuts.len(),
                cut_range(&report.cuts),
                report.mix
            )
        }
    });
    m.push_str("\n\n---\nSaved:\n");
    let verb = if report.redo { "replaced" } else { "appended" };
    m.push_str(&format!(
        "- master `{}` (Scene {scene} {} {verb})\n",
        paths.prod_md.display(),
        cut_range(&report.cuts)
    ));
    for sidecar in &report.sidecars {
        m.push_str(&format!("- sidecar `{}`\n", sidecar.display()));
    }
    m.push_str(&format!("- state `{}`\n", paths.state.display()));
    if let Some(s) = strategy {
        m.push_str(&format!(
            "\nStrategy: {}. Mix this turn: {}.\n",
            s.describe(),
            report.mix
        ));
    }
    if !host_notes.is_empty() {
        m.push_str("\nHost notes:\n");
        for note in host_notes {
            m.push_str(&format!("- {note}\n"));
        }
    }
    if let Some(recovery) = recovery {
        m.push_str(&format!("\nNote: {}.\n", recovery.describe()));
    }
    if !report.warnings.is_empty() {
        m.push_str("\nWarnings:\n");
        for warning in &report.warnings {
            m.push_str(&format!("- {warning}\n"));
        }
    }
    if report.redo {
        m.push_str(&format!(
            "\nScene {scene} dialogue coverage is unchanged ({} of {} words).\n",
            report.covered_word_count, report.total_word_count
        ));
    } else if report.total_word_count == 0 {
        m.push_str(&format!("\nScene {scene} has no source dialogue; {} cut(s) planned. The host gates on dialogue only; action, sound and transition beats are not tracked.\n", report.section_cuts.len()));
    } else if report.section_done {
        m.push_str(&format!(
            "\nScene {scene} dialogue is fully covered ({} of {} words across {} cut(s)). The host gates on dialogue coverage only; action, sound and transition beats are not tracked.\n",
            report.covered_word_count, report.total_word_count, report.section_cuts.len()
        ));
    } else {
        m.push_str(&format!(
            "\nScene {scene} dialogue: {} of {} words covered, {} remain. Continuing automatically.\n",
            report.covered_word_count, report.total_word_count, report.total_word_count.saturating_sub(report.covered_word_count)
        ));
    }
    m
}

const ASK_NUDGE: &str = r#"Workbench host: do not ask questions on this scene-plan turn. Choose reasonable placeholder details from the script you were given and emit <workbench_file path="prod.md"> containing the `### Scene N — Cut M` Markdown/YAML now."#;

fn push_correction(messages: &mut Vec<serde_json::Value>, reply: &str, correction: &str) {
    messages.push(json!({ "role": "assistant", "content": reply }));
    messages.push(json!({ "role": "user", "content": correction }));
}

fn no_files_report(reason: &NoFilesReason, reply: &str, attempts: usize) -> String {
    let excerpt: String = reply.chars().take(RAW_REPLY_EXCERPT_CHARS).collect();
    let truncated_note = if reply.chars().count() > RAW_REPLY_EXCERPT_CHARS {
        " (truncated)"
    } else {
        ""
    };
    format!(
        "Scene Plan produced no usable files after {attempts} attempt(s). Nothing was written to prod.md, sidecars or state.
         Reason: {}
         Reply length: {} chars
         Raw reply{truncated_note}:
---
{}
---",
        reason.describe(),
        reply.chars().count(),
        excerpt.trim()
    )
}

struct SourceContext {
    title: Option<String>,
    section_text: Option<String>,
    dialogue_text: Option<String>,
}

fn describe_policy(policy: &RenderDurationPolicy) -> String {
    match policy {
        RenderDurationPolicy::Buckets { seconds } => format!(
            "{} s",
            seconds
                .iter()
                .map(|s| format!("{s}"))
                .collect::<Vec<_>>()
                .join(" / ")
        ),
        RenderDurationPolicy::Continuous {
            min_seconds,
            max_seconds,
            step_seconds,
        } => format!("{min_seconds}–{max_seconds} s in {step_seconds} s steps"),
    }
}

fn scene_plan_host_preamble() -> String {
    format!(
        r#"## Workbench host — scene plan runner

You are running inside the Video Production Workbench desktop host under the Director.
You have no tools. The host reads sources for you and writes the files you emit.

### Output protocol (required, no exceptions)

<workbench_file path="prod.md">
### Scene N — Cut M

#### Atlas Generation Handoff

```yaml
...one handoff per cut...
```
</workbench_file>

<workbench_summary>
One or two sentences: cuts planned, dialogue covered, what remains.
</workbench_summary>

Rules:
- N is the *source* scene of the script you were given; M is the generated cut within it, numbered from the value the host tells you.
- Never ask questions on this turn. Choose reasonable placeholder details from the script.
- Do not emit sidecar files; the host renders them from your YAML.
- Plan at most {MAX_CUTS_PER_TURN} cuts per reply and then stop; the host calls you again for the rest.
- Do not wrap the file block in a code fence and do not HTML-escape the tags.
- Everything outside the two blocks is ignored."#
    )
}

fn scene_plan_contract() -> &'static str {
    r#"You are writing canonical cut handoffs for the Workbench host.

The LLM owns creative decisions: how to divide a scene into cuts, action, framing, visual continuity, prompt wording, and how to depict the script. The Rust host owns schema, serialization, timing math, asset discovery, validation, file staging, and state.

Required shape:

### Scene 1 — Cut 1

#### Atlas Generation Handoff

```yaml
source_scene: 1
cut: 1
title: Short title
scene_type: character_performance
continuity:
  group: scene1_train
  relation_to_previous: new_setup
  opening_state: "..."
  closing_state: "..."
  carry_forward: []
editing:
  pace: natural
  transition_in: cut
  transition_out: cut
  cut_guidance: null
speech:
  mode: source_video_lipsync
  cues:
    - speaker: Tom
      text: "Dialogue copied from the source."
      on_camera: true
      delivery: calm, solemn
prompt: |-
  Creative visual prompt.
negative_prompt: "bad framing, distorted faces"
```

Rules:
- `source_scene` and `cut` must match the heading `### Scene N — Cut M`.
- Cue `text` must quote the source dialogue verbatim and in order, starting with the first uncovered line. You may stop partway; the host tracks what remains.
- Lip-sync readiness is a *capability*, not an instruction. Use `speech.mode: source_video_lipsync` when the speaker's mouth visibly performs the line on camera. One on-camera speaker per lip-sync cut. Name the speaker; the host attaches their reference image.
- Otherwise a line may play as `voice_over`: over a reaction, an insert, atmosphere, or over the speaker's own visible but non-speaking face (`on_camera: true`; say the face is not speaking in the prompt and put `lip movement` in negative_prompt). Never let a voice-over cut *look* like lip-sync.
- Silent cuts (`speech.mode: none`) are expected for establishing shots, inserts, reactions, and action/sound/transition beats. Vary shot types according to the scene strategy the host gives you.
- Speakers without a listed reference cannot be lip-synced: keep them off camera or in voice-over.
- `editing.pace` is `tight`, `natural`, or `held`. The host snaps duration to a provider bucket. Split a cut if the spoken line cannot fit the largest bucket of the resolved lane.
- Do **not** emit `provider`, `stage`, `speech.language`, cue `id`, `generation_requirements`, `reference_assets`, `estimated_screen_duration_seconds`, handle seconds, or `target_duration_seconds`. The host derives those.
- Do not emit sidecar files. Do not ask questions.
"#
}

fn source_context(
    brief: &str,
    paths: &[String],
    context: &DirectorContext,
    project_root: Option<&str>,
) -> SourceContext {
    let requested_scene = requested_scene_number(brief);
    let mut candidates = Vec::new();
    for raw in paths {
        if let Ok(path) = resolve_under_project(raw, project_root).map_err(|e| e.to_string()) {
            if let Ok(text) = fs::read_to_string(path) {
                candidates.push(text);
            }
        }
    }
    if let Some(excerpt) = context.focused_editor_excerpt.as_deref() {
        candidates.push(excerpt.to_string());
    }
    candidates.push(brief.to_string());

    for text in &candidates {
        let section = requested_scene
            .and_then(|n| extract_scene_section(text, n))
            .or_else(|| Some(text.trim().to_string()).filter(|s| !s.is_empty()));
        if let Some(section_text) = section {
            let dialogue = extract_dialogue_text(&section_text);
            if !dialogue.trim().is_empty() {
                return SourceContext {
                    title: section_title(&section_text),
                    section_text: Some(section_text),
                    dialogue_text: Some(dialogue),
                };
            }
        }
    }
    SourceContext {
        title: None,
        section_text: candidates.into_iter().find(|s| !s.trim().is_empty()),
        dialogue_text: None,
    }
}

#[cfg(test)]
mod adapter_tests {
    use super::*;
    use crate::script::{extract_dialogue_text, extract_scene_section, section_title};

    const SCRIPT: &str = "Scene 1 — The last train\n\n\
- **Location:** A two-carriage night train approaching Willowby Halt.\n\
- **Action:** Rain streams down the carriage windows. Pip Vale, six, small and hungry, clutches his coat and peers at Tom’s purse.\n\
- **Dialogue:**\n\
- **Tom:** We shall be there before supper.\n\
- **Pip:** Will there be cocoa?\n\
- **Tom:** Aunt Marian promised clean sheets and hot cocoa.\n\
- **Pip:** And jam?\n\
- **Nell:** If Aunt Marian has any.\n\
- **Pip:** Then we must get there quickly.\n\
- **Sound:** Train wheels clatter. Rain strikes the glass. The hanging lamp creaks overhead.\n\
- **Action:** The train slows. Its brakes sigh. The children gather the parcel and prepare to leave.\n\
- **Dialogue:**\n\
- **Guard, outside:** Willowby Halt!\n\
- **Transition:** Match cut from the train’s warm yellow window to the red lantern burning in the rain.\n\n\
Scene 2 — The halt\n\n\
- **Dialogue:**\n\
- **Nell:** Here we are.\n";

    const EXPECTED_DIALOGUE: &str = "We shall be there before supper. Will there be cocoa? Aunt Marian promised clean sheets and hot cocoa. And jam? If Aunt Marian has any. Then we must get there quickly. Willowby Halt!";

    #[test]
    fn flat_bullet_dialogue_is_extracted_in_order() {
        let section = extract_scene_section(SCRIPT, 1).unwrap();
        assert_eq!(extract_dialogue_text(&section), EXPECTED_DIALOGUE);
        assert_eq!(
            section_title(&section).as_deref(),
            Some("Scene 1 — The last train")
        );
        assert_eq!(
            extract_dialogue_text(&extract_scene_section(SCRIPT, 2).unwrap()),
            "Here we are."
        );
    }

    #[test]
    fn brief_parsing() {
        let brief = "Please create the production notes for Scene 1- The last train.\nFormat 9:16\nStyle storybook realism";
        assert_eq!(requested_scene_number(brief), Some(1));
        assert_eq!(aspect_from_text(brief).as_deref(), Some("9:16"));
    }

    fn test_turn(dir: &Path) -> (Turn, fixtures::FixtureProject) {
        let fx = fixtures::write_fixture_project(dir);
        let registry = AssetRegistry::discover(dir);
        let mut state = fixtures::plan_state(1);
        state.sections[0].next_cut = 3;
        let config = HostConfig::default().with_lane_models(&LaneModels {
            text_to_video: Some("google/veo-3".into()),
            ..Default::default()
        });
        let turn = Turn {
            brief: "brief".into(),
            source_scene: 1,
            plan_paths: PlanPaths {
                prod_md: dir.join("prod.md"),
                sidecar_root: dir.join("Scene_Prompts"),
                state: dir.join("state.json"),
            },
            host_notes: Vec::new(),
            registry,
            config,
            state,
            section_index: 0,
            source: SourceContext {
                title: Some("Scene 1 — The last train".into()),
                section_text: Some(SCRIPT.into()),
                dialogue_text: Some(EXPECTED_DIALOGUE.into()),
            },
            source_spoken: EXPECTED_DIALOGUE.into(),
            strategy: StrategyChoice::choose(None, "make it atmosphere-heavy"),
            read_labels: Vec::new(),
            missing: Vec::new(),
            dossier: String::new(),
        };
        (turn, fx)
    }

    #[test]
    fn user_block_carries_dual_identity_assets_buckets_and_strategy() {
        let dir = tempfile::tempdir().unwrap();
        let (turn, fx) = test_turn(dir.path());
        let block = scene_plan_user_block(&turn);
        assert!(
            block.contains("the first block this turn is `### Scene 1 — Cut 3`"),
            "{block}"
        );
        assert!(block.contains(&format!("reference_image: `{}`", fx.tom_closeup.display())));
        assert!(block.contains("`text_to_video` (google/veo-3): 4 / 6 / 8 s"));
        assert!(block.contains("## Scene strategy: atmosphere-heavy (inferred from the request)"));
        assert!(block.contains("a capability, not an instruction"));
        assert!(block.contains("Scene_Prompts/Scene1/CutM.txt"));
    }

    #[test]
    fn contract_no_longer_mandates_lipsync() {
        let c = scene_plan_contract();
        assert!(!c.contains("must, when on camera, get `speech.mode: source_video_lipsync`"));
        assert!(!c.contains("`voice_over` cuts must not show a lip-sync-ready speaker on camera"));
        assert!(c.contains("capability"));
        assert!(c.contains("Silent cuts (`speech.mode: none`) are expected"));
        assert!(c.contains("Do **not** emit `provider`"));
        assert!(!c.contains("generation_path: audio_driven_i2v"));
        assert!(!c.contains("estimated_screen_duration_seconds:"));
    }

    #[test]
    fn redo_user_block_lists_exact_dialogue_and_neighbors() {
        let dir = tempfile::tempdir().unwrap();
        let (turn, _fx) = test_turn(dir.path());
        let ctx = vec![CutContext {
            id: CutId::new(1, 2),
            dialogue: "Will there be cocoa?".into(),
            previous_yaml: "source_scene: 1\ncut: 2".into(),
            prev_closing_state: Some("Tom looks down.".into()),
            next_opening_state: Some("Nell turns to the window.".into()),
        }];
        let block = redo_user_block(&turn, &ctx);
        assert!(block.contains("REDO MODE"));
        assert!(block.contains("Re-emit **exactly** these blocks of source Scene 1, in this order, and nothing else: `### Scene 1 — Cut 2`"));
        assert!(block.contains("Dialogue this cut must carry: \"Will there be cocoa?\""));
        assert!(block.contains("Previous cut ends with: Tom looks down."));
        assert!(block.contains("Next cut opens with: Nell turns to the window."));
    }

    #[test]
    fn commit_message_shows_mix_strategy_and_redo_wording() {
        let paths = PlanPaths {
            prod_md: PathBuf::from("/p/Prod_Notes/Scene3.md"),
            sidecar_root: PathBuf::from("/p/Prod_Notes"),
            state: PathBuf::from("/p/Prod_Notes/scene_plan_state.json"),
        };
        let report = CommitReport {
            source_scene: 3,
            cuts: vec![CutId::new(3, 4)],
            section_cuts: (1..=9).collect(),
            sidecars: vec![paths.sidecar_root.join("Scene3/Cut4.txt")],
            section_done: true,
            plan_done: true,
            covered_word_count: 62,
            total_word_count: 62,
            remaining_source_speech: String::new(),
            warnings: Vec::new(),
            mix: LaneMix {
                lipsync: 0,
                voice_over: 1,
                silent: 0,
            },
            redo: true,
        };
        let m = commit_message(
            None,
            &report,
            &paths,
            None,
            &[],
            Some(StrategyChoice::choose(None, "")),
        );
        assert!(
            m.contains(
                "Scene Plan replaced Cut 4 of Scene 3 (0 lip-sync / 1 voice-over / 0 silent)."
            ),
            "{m}"
        );
        assert!(m.contains("(Scene 3 Cut 4 replaced)"));
        assert!(m.contains("Strategy: cinematic-balanced (host default"));
        assert!(m.contains("Scene 3 dialogue coverage is unchanged (62 of 62 words)."));
        assert!(!m.contains("Continuing automatically"));
    }

    #[test]
    fn no_files_report_is_structured_and_bounded() {
        let report = no_files_report(
            &NoFilesReason::NoTags {
                looks_like: "prose",
            },
            &"x".repeat(5_000),
            3,
        );
        assert!(
            report.contains("after 3 attempt(s)")
                && report.contains("(truncated)")
                && report.len() < 2_000
        );
    }
}
