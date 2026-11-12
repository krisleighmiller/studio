use workbench_host::director_actions::{parse_actions, planner_system_prompt, ActionKind, SPECIALISTS};

const RETIRED: &[&str] = &["script", "character_creation", "production_helper"];

#[test]
fn lp03_no_runtime_or_build_dependency_on_archived_contracts() {
    use std::{fs, path::Path};
    // Explicit inventory includes the bypassed scene-plan and unused media contracts.
    let contracts = [
        ("script-assist-system-prompt.md", "SCRIPT_ASSISTANT_PROMPT"),
        ("character-creation-system-prompt.md", "CHARACTER_CREATION_PROMPT"),
        ("vid-prompt-system-prompt.md", "SCENE_PLAN_PROMPT"),
        ("model-sheet-system-prompt.md", "MODEL_SHEET_PROMPT"),
        ("still-image-system-prompt.md", "STILL_IMAGE_PROMPT"),
        ("vid-gen-system-prompt.md", "SCENE_GENERATION_PROMPT"),
        ("text-sync-system-prompt.md", "TEXT_SYNC_PROMPT"),
        ("post-prod-system-prompt.md", "ASSEMBLE_PROMPT"),
        ("production-helper-system-prompt.md", "PRODUCTION_HELPER_PROMPT"),
    ];
    fn collect(path: &Path, files: &mut Vec<std::path::PathBuf>) {
        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                collect(&entry.unwrap().path(), files);
            }
        } else if path.is_file() {
            files.push(path.to_owned());
        }
    }
    let host = Path::new(env!("CARGO_MANIFEST_DIR"));
    let app = host.parent().unwrap();
    let mut files = Vec::new();
    for dir in [host.join("src"), app.join("src"), app.join("electron"), app.join("scripts")] {
        collect(&dir, &mut files);
    }
    // Root configuration/build entrypoints too; tests/evidence/history are not consumers.
    for root in [host, app] {
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() { files.push(path); }
        }
    }
    let mut violations = Vec::new();
    let mut scanned = 0;
    for path in files {
        let Ok(source) = fs::read_to_string(&path) else { continue };
        scanned += 1;
        for (filename, constant) in contracts {
            for token in [filename, constant] {
                if source.contains(token) { violations.push(format!("{}: {token}", path.display())); }
            }
        }
        // Catches dynamic directory reads, includes and fallback resolution as well
        // as filename aliases; Markdown documentation can retain historical prose.
        if path.extension().is_some_and(|ext| ext != "md") {
            for token in ["System_Prompts", "text_specialist_prompt", "run_text_specialist(", "run_production_helper(", "TEXT_RUNNER_HOST_PREAMBLE"] {
                if source.contains(token) { violations.push(format!("{}: {token}", path.display())); }
            }
        }
    }
    assert!(scanned > 100, "bounded production/build inventory unexpectedly empty: {scanned}");
    println!("LP03 inspected {scanned} production/build files against {} archived filename/constant pairs", contracts.len());
    assert!(violations.is_empty(), "legacy production consumers:\n{}", violations.join("\n"));
}

#[test]
fn lp02_legacy_validation_still_reaches_retirement_dispatch() {
    use workbench_host::director_actions::{validate_action, DirectorAction};
    let root = tempfile::tempdir().unwrap();
    for retired in RETIRED {
        let action: DirectorAction = serde_json::from_value(serde_json::json!({
            "action": "run_specialist", "specialist": retired, "inputs": {}
        })).unwrap();
        let validated = validate_action(action, Some(root.path().to_str().unwrap())).unwrap();
        assert_eq!(validated.action.specialist.as_deref(), Some(*retired));
    }
}

#[test]
fn lp02_planner_advertises_only_supported_operations() {
    let prompt = planner_system_prompt();
    println!("ACTUAL_PLANNER_PROMPT_BEGIN\n{prompt}\nACTUAL_PLANNER_PROMPT_END");
    println!("ACTUAL_CAPABILITIES: {SPECIALISTS:?}");
    let declaration = prompt.lines().find_map(|line| line.strip_prefix("Specialists: ")).unwrap();
    let advertised: Vec<_> = declaration.split(", ").collect();
    for retired in RETIRED {
        assert!(!advertised.contains(retired), "retired declaration: {retired}");
        assert!(!SPECIALISTS.contains(retired), "retired capability metadata: {retired}");
        assert!(!prompt.contains(&format!("→ {retired} ")), "retired routing: {retired}");
        assert!(!prompt.contains(&format!("### {retired} inputs")), "retired input instructions: {retired}");
    }
    for supported in ["scene_intent", "scene_look", "cut_sketches", "scene_plan", "model_sheet", "still_image", "scene_generation", "text_sync", "lip_sync", "assemble", "text_write"] {
        assert!(advertised.contains(&supported), "missing supported declaration: {supported}");
        assert!(SPECIALISTS.contains(&supported));
    }
    let examples = parse_actions(&prompt).unwrap();
    let operations: Vec<_> = examples.iter().filter(|a| a.action == ActionKind::RunSpecialist).map(|a| a.specialist.as_deref().unwrap()).collect();
    assert!(operations.len() >= 10, "must inspect real generated examples");
    for retired in RETIRED {
        assert!(!operations.contains(retired), "retired action example: {retired}");
    }
    for supported in ["scene_plan", "model_sheet"] {
        assert!(operations.contains(&supported), "missing supported example: {supported}");
    }
    // Deliberately do not ban ordinary prose mentioning retired operations as unavailable.
}
