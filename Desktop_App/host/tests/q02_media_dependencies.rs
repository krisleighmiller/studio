//! Q02 regression: actual selected media decodability / audio truth / dependency gap.
//! Uses real local ffmpeg fixtures; no live provider / budget / schema changes.
use std::{fs, process::Command};
use workbench_host::workflow::{post, state::{ProjectState, CutRecord, TakeRecord, TakeStatus, ProductionProfile, DeliverableRecord, AssetRecord, AssetStatus, AssetRole, AssemblyShotSnapshot}};
use workbench_host::workflow::qc::{QCSeverity, check_audio_decode, probe_media, probe_audio_levels};

fn ok(cmd: &mut Command) -> Vec<u8> {
    let p = cmd.output().unwrap();
    assert!(p.status.success(), "{:?}: {}", cmd, String::from_utf8_lossy(&p.stderr));
    p.stdout
}

#[test]
fn q02_media_dependencies_regression() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Preproduction")).unwrap();
    fs::create_dir_all(root.join("Scenes/Scene1/Cut1")).unwrap();

    // Real valid take (video + audio)
    let take_path = root.join("Scenes/Scene1/Cut1/take.mp4");
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","color=c=blue:s=160x90:r=24:d=2",
        "-f","lavfi","-i","sine=frequency=440:duration=2",
        "-shortest","-c:v","libx264","-pix_fmt","yuv420p","-c:a","aac",
    ]).arg(&take_path));

    // Short video (0.4s) + long audio (2s) fixture for window failure test
    let short_video_path = root.join("Scenes/Scene1/Cut1/short_video.mp4");
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","color=c=red:s=160x90:r=24:d=0.4",
        "-f","lavfi","-i","sine=frequency=440:duration=2",
        "-c:v","libx264","-pix_fmt","yuv420p","-c:a","aac",
    ]).arg(&short_video_path));

    // Corrupt copy (truncated) for mutation test
    let corrupt_path = root.join("Scenes/Scene1/Cut1/take_corrupt.mp4");
    fs::copy(&take_path, &corrupt_path).unwrap();
    let bytes_corrupt = fs::read(&corrupt_path).unwrap();
    fs::write(&corrupt_path, &bytes_corrupt[..bytes_corrupt.len()/4]).unwrap();

    // Audio-only source (no video) fixture — real container with audio stream only
    let audio_only_path = root.join("Scenes/Scene1/Cut1/audio_only.mp4");
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","sine=frequency=440:duration=2",
        "-c:a","aac","-f","mp4",
    ]).arg(&audio_only_path));

    // Master with audio
    let master_path = root.join("Final/Scene1_master.mp4");
    fs::create_dir_all(master_path.parent().unwrap()).unwrap();
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","color=c=black:s=160x90:r=24:d=2",
        "-f","lavfi","-i","sine=frequency=440:duration=2",
        "-shortest","-c:v","libx264","-pix_fmt","yuv420p","-c:a","aac",
    ]).arg(&master_path));

    // Master silent (no audio)
    let silent_master = root.join("Final/silent_master.mp4");
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","color=c=black:s=160x90:r=24:d=2",
        "-c:v","libx264","-pix_fmt","yuv420p",
    ]).arg(&silent_master));

    // Required audio assets (positive non-MP3 fixtures: WAV, FLAC, OGG)
    let audio_wav_path = root.join("audio/music.wav");
    fs::create_dir_all(audio_wav_path.parent().unwrap()).unwrap();
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","sine=frequency=440:duration=2",
        "-c:a","pcm_s16le","-ar","48000","-f","wav",
    ]).arg(&audio_wav_path));

    let audio_flac_path = root.join("audio/music.flac");
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","sine=frequency=440:duration=2",
        "-c:a","flac","-ar","48000","-f","flac",
    ]).arg(&audio_flac_path));

    let audio_ogg_path = root.join("audio/music.ogg");
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","sine=frequency=440:duration=2",
        "-c:a","libvorbis","-ar","48000","-f","ogg",
    ]).arg(&audio_ogg_path));

    let audio_asset_path = root.join("audio/music.mp3");
    ok(Command::new("ffmpeg").args([
        "-v","error","-y","-f","lavfi","-i","sine=frequency=440:duration=2",
        "-c:a","libmp3lame","-q:a","4",
    ]).arg(&audio_asset_path));

    let corrupt_audio_path = root.join("audio/music_corrupt.mp3");
    fs::write(&corrupt_audio_path, b"NOTAUDIO").unwrap();

    // Missing audio asset file (bytes missing)
    assert!(!root.join("audio/music_missing.mp3").exists());
    // intentionally no file created

    // Build state with valid selected take, locked version, master, snapshot
    let mut state = ProjectState::default();
    state.settings.aspect_ratio = Some("9:16".into());
    state.profile = Some(ProductionProfile {
        resolution: Some("160x90".into()),
        aspect_ratios: vec!["9:16".into()],
        deliverables: vec![DeliverableRecord {
            id: "del_1".into(), name: "Scene master".into(), audience: None,
            platforms: Vec::new(), runtime: None, aspect_ratio: Some("9:16".into()),
            resolution: Some("160x90".into()), frame_rate: None, language: None,
        }],
        audio_plan: Some("music".into()),
        ..ProductionProfile::default()
    });
    // Approved MP3 asset
    state.assets.push(AssetRecord { id: "a1".into(), path: "audio/music.mp3".into(), role: AssetRole::Music, status: AssetStatus::Approved, scene_id: Some(1), cut_id: None, character_id: None, source_job_id: None, parent_asset_id: None, notes: None });
    // Additional positive fixtures (not selected by default, just for decode tests)
    state.assets.push(AssetRecord { id: "a_wav".into(), path: "audio/music.wav".into(), role: AssetRole::Music, status: AssetStatus::Approved, scene_id: Some(1), cut_id: None, character_id: None, source_job_id: None, parent_asset_id: None, notes: None });
    state.assets.push(AssetRecord { id: "a_flac".into(), path: "audio/music.flac".into(), role: AssetRole::Music, status: AssetStatus::Approved, scene_id: Some(1), cut_id: None, character_id: None, source_job_id: None, parent_asset_id: None, notes: None });
    state.assets.push(AssetRecord { id: "a_ogg".into(), path: "audio/music.ogg".into(), role: AssetRole::Music, status: AssetStatus::Approved, scene_id: Some(1), cut_id: None, character_id: None, source_job_id: None, parent_asset_id: None, notes: None });

    let scene = state.scene_mut(1, "Scene");
    let mut cut = CutRecord::new(workbench_host::scene_plan::handoff::CutId::new(1,1));
    cut.stable_id = "shot_a".into();
    cut.takes.push(TakeRecord { id: "t1".into(), path: "Scenes/Scene1/Cut1/take.mp4".into(), status: TakeStatus::Selected, notes: None });
    cut.selected_take = Some("t1".into());
    scene.cuts = vec![cut];

    // Post / version with assembly pointing to master
    let post = scene.post.get_or_insert_with(workbench_host::workflow::post::ScenePost::default);
    use workbench_host::workflow::post::{CutVersion, CutVersionStage, VersionShot};
    let version = CutVersion {
        id: "cutver_1".into(), label: "Selects 1".into(), stage: CutVersionStage::PictureLock,
        assembly_path: Some("Final/Scene1_master.mp4".into()),
        shots: vec![VersionShot { shot_id: "shot_a".into(), cut_label: 1, take_id: "t1".into(), path: "Scenes/Scene1/Cut1/take.mp4".into() }],
        assembly_snapshot: Some(vec![AssemblyShotSnapshot { shot_id: "shot_a".into(), cut_label: 1, take_id: "t1".into(), path: "Scenes/Scene1/Cut1/take.mp4".into(), start_seconds: 0.0, duration_seconds: 2.0, transition: "cut".into() }]),
        editorial_path: workbench_host::workflow::post::EditorialPath::HostFfmpeg,
    };
    post.versions.push(version);
    post.picture_locked_id = Some("cutver_1".into());

    // Positive valid control (before mutations)
    let mut s = state.clone();
    let check = post::check_deliverable(&mut s, 1, "del_1", &root).unwrap();
    assert!(check.passed, "valid should pass: {:?}", check.messages);
    assert!(check.media_findings.iter().filter(|f| f.severity == QCSeverity::Error && (f.id.contains("source") || f.id.contains("audio") || f.id == "master_audio_missing")).next().is_none(), "valid should have no dependency errors: {:?}", check.media_findings);

    // Helper to restore original take bytes
    let original_bytes = fs::read(&take_path).unwrap();

    // === Mutation tests (mutate BYTES at exact selected source path) ===

    // 1. Missing selected source (file removed at exact path)
    {
        fs::remove_file(&take_path).unwrap();
        let mut bad = state.clone();
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "missing selected source should fail");
        let has_source_exists = b.media_findings.iter().any(|f| f.id == "source_exists" && f.severity == QCSeverity::Error);
        assert!(has_source_exists, "missing source should have source_exists error: {:?}", b.media_findings);
        // Restore exact bytes and identity
        fs::write(&take_path, &original_bytes).unwrap();
    }

    // 2. Corrupt selected source (bytes truncated at exact path)
    {
        let truncated = &original_bytes[..original_bytes.len()/4];
        fs::write(&take_path, truncated).unwrap();
        let mut bad = state.clone();
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "corrupt selected source should fail");
        let has_decode_err = b.media_findings.iter().any(|f| f.id == "full_decode" || f.id == "audio_decode" || f.id == "audio_stream");
        assert!(has_decode_err || b.media_findings.iter().any(|f| f.severity == QCSeverity::Error && f.id.contains("decode")), "corrupt should produce decode/probe error: {:?}", b.media_findings);
        fs::write(&take_path, &original_bytes).unwrap();
    }

    // 3. Audio-only selected source (bytes overwritten with audio-only content at exact path)
    {
        let audio_bytes = fs::read(&audio_only_path).unwrap();
        fs::write(&take_path, &audio_bytes).unwrap();
        let mut bad = state.clone();
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "audio-only selected source should fail");
        let has_no_video_or_decode = b.media_findings.iter().any(|f| f.id == "no_video" || f.id == "full_decode");
        assert!(has_no_video_or_decode, "audio-only source should trigger no_video or full_decode error: {:?}", b.media_findings);
        fs::write(&take_path, &original_bytes).unwrap();
    }

    // 4. Short video + long audio (bytes overwritten with short_video at exact path); request window 2s
    {
        let short_bytes = fs::read(&short_video_path).unwrap();
        fs::write(&take_path, &short_bytes).unwrap();
        // The version snapshot requests 2.0s duration; video is 0.4s
        let mut bad = state.clone();
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "short video long audio should fail for 2s window");
        let has_window_err = b.media_findings.iter().any(|f| f.id == "source_window" && f.severity == QCSeverity::Error);
        assert!(has_window_err, "short video should trigger source_window error: {:?}", b.media_findings);
        fs::write(&take_path, &original_bytes).unwrap();
    }

    // 5. Outside / symlink source refusal unchanged bytes
    {
        let outside = dir.path().join("outside.mp4");
        fs::write(&outside, b"outside untouched").unwrap();
        let link_target = outside.as_os_str().to_str().unwrap();
        fs::remove_file(&take_path).unwrap();
        std::os::unix::fs::symlink(link_target, &take_path).unwrap();
        let mut bad = state.clone();
        let result = post::check_deliverable(&mut bad, 1, "del_1", &root);
        assert!(result.is_err() || !result.as_ref().unwrap().passed, "symlink source should refuse/open error");
        if let Ok(b) = result {
            assert!(!b.passed, "symlink source should fail deliverable");
            assert!(b.media_findings.iter().any(|f| f.id == "selected_source_escape" && f.severity == QCSeverity::Error), "symlink must fail confinement, not merely a later media check");
        }
        // Restore exact bytes and identity
        fs::remove_file(&take_path).unwrap();
        fs::write(&take_path, &original_bytes).unwrap();
        assert_eq!(fs::read(&outside).unwrap(), b"outside untouched", "outside bytes must remain unchanged");
    }

    // Positive control restored (identity unchanged)
    let mut s = state.clone();
    let check = post::check_deliverable(&mut s, 1, "del_1", &root).unwrap();
    assert!(check.passed, "restored positive should pass: {:?}", check.messages);
    assert_eq!(fs::read(&take_path).unwrap(), original_bytes, "take bytes must be restored exactly");

    // === New dependency checks ===

    // Direct non-MP3 audio decode positives (WAV, FLAC, OGG)
    for (label, p) in [("WAV", &audio_wav_path), ("FLAC", &audio_flac_path), ("OGG", &audio_ogg_path)] {
        let findings = check_audio_decode(p);
        assert!(!findings.iter().any(|f| f.severity == QCSeverity::Error), "{} audio decode should have no errors: {:?}", label, findings);
        let has_stream = findings.iter().any(|f| f.id == "audio_stream");
        assert!(has_stream || findings.iter().any(|f| f.severity == QCSeverity::Info), "{} should report audio stream info: {:?}", label, findings);
    }

    // Unapproved required audio
    {
        let mut bad = state.clone();
        bad.assets[0].status = AssetStatus::Rejected;
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "unapproved required audio should fail");
        let has_unapproved = b.media_findings.iter().any(|f| f.id == "audio_unapproved" && f.severity == QCSeverity::Error);
        assert!(has_unapproved, "should report audio_unapproved: {:?}", b.media_findings);
    }

    // Missing required role (clear assets with audio required)
    {
        let mut bad = state.clone();
        bad.assets.clear();
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "missing required audio role should fail");
        let has_missing_role = b.media_findings.iter().any(|f| f.id == "audio_role_missing" && f.severity == QCSeverity::Error);
        assert!(has_missing_role, "should report audio_role_missing: {:?}", b.media_findings);
    }

    // Missing required asset bytes (path points to missing file)
    {
        let mut bad = state.clone();
        bad.assets[0].path = "audio/music_missing.mp3".into();
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "missing required asset bytes should fail");
        let has_file_err = b.media_findings.iter().any(|f| f.id == "file_exists" && f.severity == QCSeverity::Error);
        assert!(has_file_err, "should report file_exists error for missing asset: {:?}", b.media_findings);
    }

    // Optional silence (silent master when no audio required) preserved
    {
        let mut silent = state.clone();
        silent.profile.as_mut().unwrap().audio_plan = Some("".into());
        silent.assets.clear();
        silent.scene_mut(1,"Scene").post.as_mut().unwrap().versions[0].assembly_path = Some("Final/silent_master.mp4".into());
        let s = post::check_deliverable(&mut silent, 1, "del_1", &root).unwrap();
        assert!(s.passed, "silent master allowed when no audio required: {:?}", s.messages);
    }

    // Missing unselected alternate allowed preserved
    {
        let mut alt = state.clone();
        alt.scene_mut(1,"Scene").cuts[0].takes.push(TakeRecord { id: "t2".into(), path: "Scenes/Scene1/Cut1/nonexistent.mp4".into(), status: TakeStatus::Alternate, notes: None });
        let a = post::check_deliverable(&mut alt, 1, "del_1", &root).unwrap();
        assert!(a.passed, "unselected missing alternate should not block: {:?}", a.messages);
    }

    // Corrupt audio asset preserved
    {
        let mut bad = state.clone();
        bad.assets[0].path = "audio/music_corrupt.mp3".into();
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "corrupt required audio should fail");
        let has_err = b.media_findings.iter().any(|f| f.id == "audio_decode" || f.id == "audio_stream" || f.id == "file_exists");
        assert!(has_err, "corrupt audio should have decode/file error: {:?}", b.media_findings);
    }

    // Master audio stream missing preserved
    {
        let mut bad = state.clone();
        bad.scene_mut(1,"Scene").post.as_mut().unwrap().versions[0].assembly_path = Some("Final/silent_master.mp4".into());
        let b = post::check_deliverable(&mut bad, 1, "del_1", &root).unwrap();
        assert!(!b.passed, "missing master audio stream should fail");
        let has_master_audio = b.media_findings.iter().any(|f| f.id == "master_audio_missing" && f.severity == QCSeverity::Error);
        assert!(has_master_audio, "should report master_audio_missing: {:?}", b.media_findings);
    }

    // .m3u8 playlist referencing outside-root local .ts and loopback URL
    {
        let playlist_path = root.join("Final/q02_master.m3u8");
        fs::create_dir_all(playlist_path.parent().unwrap()).unwrap();
        // Outside-root segment file
        let outside_segment = dir.path().join("outside_segment.ts");
        fs::write(&outside_segment, b"FAKESEGMENT").unwrap();
        fs::write(&playlist_path, format!("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:1.0,\n{}\n#EXTINF:1.0,\nhttp://127.0.0.1/loop_segment.ts\n#EXT-X-ENDLIST\n", outside_segment.display())).unwrap();
        let probe_error = probe_media(&playlist_path).unwrap_err();
        assert!(probe_error.contains("not on whitelist"), "playlist must fail at format gate, not a later decode: {probe_error}");
        let levels_error = probe_audio_levels(&playlist_path).unwrap_err();
        assert!(levels_error.contains("not on whitelist"), "levels must fail at format gate, not a later decode: {levels_error}");
    }

    // Final identity preservation check
    assert_eq!(fs::read(&take_path).unwrap(), original_bytes, "final selected source bytes must match original exactly");
    let final_state = state.clone();
    assert_eq!(final_state.scene(1).unwrap().post.as_ref().unwrap().versions.len(), 1);
    assert_eq!(final_state.scene(1).unwrap().cuts[0].selected_take.as_deref(), Some("t1"));
    assert_eq!(final_state.scene(1).unwrap().post.as_ref().unwrap().versions[0].shots[0].take_id, "t1");
    assert_eq!(final_state.scene(1).unwrap().post.as_ref().unwrap().versions[0].shots[0].path, "Scenes/Scene1/Cut1/take.mp4");
}
