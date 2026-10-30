//! Local-only assembly trim regressions; fixtures never touch real projects.
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use workbench_host::workflow::{import::persist_state, state::ProjectState};
use workbench_host::{ctx::HostCtx, director, director_actions, pending, session};

struct SessionGuard;
impl Drop for SessionGuard {
    fn drop(&mut self) {
        pending::cancel_all();
        session::close();
    }
}

fn successful(command: &mut Command) -> Output {
    let output = command.output().expect("local ffmpeg/ffprobe required");
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn probe(path: &Path) -> Value {
    serde_json::from_slice(
        &successful(
            Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-show_streams",
                    "-show_format",
                    "-of",
                    "json",
                ])
                .arg(path),
        )
        .stdout,
    )
    .unwrap()
}

fn duration(path: &Path) -> f64 {
    probe(path)["format"]["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn frame_brightness(path: &Path) -> Vec<f64> {
    let pixels = successful(
        Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-vf",
                "scale=8:8",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "pipe:1",
            ]),
    )
    .stdout;
    assert_eq!(pixels.len() % 64, 0);
    pixels
        .chunks_exact(64)
        .map(|frame| frame.iter().map(|v| f64::from(*v)).sum::<f64>() / 64.0)
        .collect()
}

/// Deterministic loopback planner only; approval/render use the real host path.
fn planned_turn(app: &HostCtx, inputs: Value, user: &str) -> director::DirectorTurnResponse {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    fs::write(app.settings_path(), json!({"apis":[{"id":"loopback-planner","name":"Local trim fixture","kind":"atlas-cloud","baseUrl":format!("http://{}", listener.local_addr().unwrap()),"apiKey":"","secretKey":"","enabled":true}],"capabilityDefaults":{}}).to_string()).unwrap();
    let action = json!({"action":"run_specialist","specialist":"assemble","inputs":inputs});
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "Director did not call local planner"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("loopback accept: {error}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buffer = [0u8; 4096];
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0, "incomplete planner request");
            bytes.extend_from_slice(&buffer[..count]);
            assert!(bytes.len() < 4 * 1024 * 1024);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                assert!(headers.starts_with("POST /v1/chat/completions "));
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .expect("request content length");
                if bytes.len() >= end + 4 + length {
                    let _: Value =
                        serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
                    break;
                }
            }
        }
        let reply = json!({"choices":[{"message":{"content":format!("<director_action>{action}</director_action>")}}]}).to_string();
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
    });
    let request = serde_json::from_value(json!({"model":"deepseek-ai/deepseek-v4-pro","apiId":"loopback-planner","messages":[{"role":"user","content":user}],"context":{}})).unwrap();
    let result = director::director_turn(app.clone(), request);
    server.join().unwrap();
    result.unwrap()
}

#[test]
fn director_custom_trim_revision_preserves_other_windows_and_fades_through_export() {
    // Only this test uses session/pending globals; the other test is parser-only.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let production = "### Scene 2\n```yaml\ntransition_in: fade\ntransition_out: cut\nspeech:\n  mode: none\n```\n### Scene 7\n```yaml\ntransition_in: cut\ntransition_out: fade\nspeech:\n  mode: none\n```\n";
    let production_path = root.join("prod.md");
    let source = root.join("marked.mp4");
    fs::write(&production_path, production).unwrap();
    // Black during source [0,1); white during [1,4). This makes a head-only
    // source-relative revision distinguishable in the actually decoded output.
    successful(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=white:s=90x160:r=24:d=4,drawbox=color=black:t=fill:enable='lt(t,1)'",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000:duration=4",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&source),
    );
    let production_before = fs::read(&production_path).unwrap();
    let source_before = fs::read(&source).unwrap();
    persist_state(&root, &ProjectState::default()).unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let _guard = SessionGuard;
    let app = HostCtx::new(dir.path().join("isolated-settings"));
    let mut unchanged_scene = None;
    let mut baseline_frames = None;
    let mut tightened_frames = None;

    for (name, start, first_duration, use_map) in [
        ("baseline-inline", 0.5, 2.5, false),
        ("tightened-inline", 1.0, 2.0, false),
        ("tightened-map", 1.0, 2.0, true),
    ] {
        // Keep the original source-relative endpoint (3s), not the full 4s
        // source endpoint; keep Scene 7's asymmetric window untouched.
        assert_eq!(start + first_duration, 3.0);
        let relative_output = format!("Final/{name}.mp4");
        let relative_work = format!("Final/work-{name}");
        let output = root.join(&relative_output);
        let work = root.join(&relative_work);
        let mut inputs = json!({"production_path":"prod.md", "trim":"full", "fade_in":false,
            "caption_mode":"none", "output_path":relative_output, "work_dir":relative_work,
            "scenes":[{"id":2,"video":"marked.mp4","trim":"custom","start":start,"duration":first_duration},
                      {"id":7,"video":"marked.mp4","trim":"custom","start":1.25,"duration":0.75}]});
        if use_map {
            let map = format!("assemble:\n  scenes:\n    2:\n      trim: custom\n      start: {start}\n      duration: {first_duration}\n      video: marked.mp4\n    7:\n      trim: custom\n      duration: 0.75\n      start: 1.25\n      video: marked.mp4\n  trim: full\n");
            fs::write(root.join("map.yaml"), map).unwrap();
            inputs.as_object_mut().unwrap().remove("scenes");
            inputs["assemble_map"] = json!("map.yaml");
        }
        let card = planned_turn(&app, inputs, "Tighten only Scene 2's head, retaining its current endpoint, Scene 7's trim and closing fade. Do not edit source media or production notes.");
        assert_eq!(
            card.tool_events,
            vec!["approval_required:assemble"],
            "{}",
            card.reply
        );
        assert!(
            card.reply.contains("Assemble plan (host-validated)"),
            "{}",
            card.reply
        );
        assert!(
            card.reply
                .contains(&format!("trim start={start:.2} dur={first_duration:.2}")),
            "{}",
            card.reply
        );
        assert!(
            card.reply.contains("trim start=1.25 dur=0.75"),
            "{}",
            card.reply
        );
        assert!(
            card.reply.contains("edge fades: in=false out=true"),
            "{}",
            card.reply
        );
        assert!(
            !output.exists() && !work.exists(),
            "no render writes before approval"
        );
        assert_eq!(fs::read(&production_path).unwrap(), production_before);
        assert_eq!(fs::read(&source).unwrap(), source_before);
        let token = card
            .reply
            .split(pending::APPROVE_TOKEN_PREFIX)
            .nth(1)
            .and_then(|rest| rest.split("]]").next())
            .expect("one-time approval token")
            .to_string();
        let completed = director::director_approve(app.clone(), token.clone()).unwrap();
        assert_eq!(
            completed.tool_events,
            vec!["approve:assemble"],
            "{}",
            completed.reply
        );
        let info = probe(&output);
        let streams = info["streams"].as_array().unwrap();
        let video = streams
            .iter()
            .find(|stream| stream["codec_type"] == "video")
            .unwrap();
        assert_eq!(video["width"], 1080);
        assert_eq!(video["height"], 1920);
        assert!(streams.iter().any(|stream| stream["codec_type"] == "audio"));
        let runtime = duration(&output);
        assert!(
            (runtime - (first_duration + 0.75)).abs() < 0.15,
            "{name}: runtime {runtime}"
        );
        assert!((duration(&work.join("scene_02_trim.mp4")) - first_duration).abs() < 0.05);
        assert!((duration(&work.join("scene_07_trim.mp4")) - 0.75).abs() < 0.05);
        let frames = frame_brightness(&output);
        assert!(!frames.is_empty());
        assert!(
            *frames.last().unwrap() < 75.0,
            "{name}: closing fade was lost"
        );
        let scene_seven = frame_brightness(&work.join("scene_07_trim.mp4"));
        assert!(scene_seven.iter().all(|level| *level > 230.0));
        if let Some(ref previous) = unchanged_scene {
            assert_eq!(&scene_seven, previous, "unrelated scene changed");
        } else {
            unchanged_scene = Some(scene_seven);
        }
        eprintln!("{name}: start={start}, duration={first_duration}, master={runtime:.3}s; first/last gray={:.1}/{:.1}", frames[0], frames.last().unwrap());
        if name == "baseline-inline" {
            assert!(frames[0] < 15.0, "baseline must retain black head");
            baseline_frames = Some(frames);
        } else {
            assert!(
                frames[0] > 230.0,
                "head trim or opening-fade override missing"
            );
            if let Some(ref previous) = tightened_frames {
                assert_eq!(
                    &frames, previous,
                    "YAML and inline trims rendered differently"
                );
            } else {
                tightened_frames = Some(frames);
            }
        }
        let output_before_replay = fs::read(&output).unwrap();
        assert!(
            director::director_approve(app.clone(), token).is_err(),
            "approval replay accepted"
        );
        assert_eq!(fs::read(&output).unwrap(), output_before_replay);
        assert_eq!(fs::read(&production_path).unwrap(), production_before);
        assert_eq!(fs::read(&source).unwrap(), source_before);
    }
    assert!(baseline_frames.unwrap().len() > tightened_frames.unwrap().len());

    // Actual public Director validation rejects malformed/out-of-bounds windows
    // before either approval or ffmpeg output creation, using the real media.
    for scene in [
        json!({"start":-0.25,"duration":1.0}),
        json!({"start":0.25,"duration":0.0}),
        json!({"start":0.25,"duration":-0.5}),
        json!({"start":4.0,"duration":0.25}),
        json!({"start":3.0,"duration":1.01}),
        json!({"start":"NaN","duration":1.0}),
        json!({"start":0.0,"duration":"Infinity"}),
        json!({"start":null,"duration":1.0}),
    ] {
        let mut scene = scene;
        scene["id"] = json!(2);
        scene["video"] = json!("marked.mp4");
        scene["trim"] = json!("custom");
        let action = json!({"action":"run_specialist","specialist":"assemble","inputs":{
            "production_path":"prod.md","output_path":"Final/invalid.mp4","work_dir":"Final/invalid-work","scenes":[scene]}});
        let parsed = director_actions::parse_actions(&format!(
            "<director_action>{action}</director_action>"
        ))
        .unwrap();
        assert!(
            director_actions::validate_action(parsed[0].clone(), Some(root.to_str().unwrap()))
                .is_err(),
            "accepted {action}"
        );
        assert!(!root.join("Final/invalid.mp4").exists());
        assert!(!root.join("Final/invalid-work").exists());
    }
    assert_eq!(fs::read(&production_path).unwrap(), production_before);
    assert_eq!(fs::read(&source).unwrap(), source_before);
}

#[test]
fn custom_windows_must_fit_separate_audio_inline_and_yaml() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(
        root.join("prod.md"),
        "### Scene 1\n```yaml\nspeech:\n  mode: none\n```\n",
    )
    .unwrap();
    successful(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=white:s=90x160:r=24:d=4",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(root.join("video.mp4")),
    );
    successful(
        Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
            ])
            .arg(root.join("short.wav")),
    );
    // Wrong behavior: video-only validation accepts both the partially covered
    // and entirely out-of-audio windows because the picture is four seconds.
    for use_map in [false, true] {
        for (start, duration, valid) in [
            (0.25, 0.5, true),
            (0.25, 0.75, true),
            (0.9, 0.2, false),
            (2.0, 1.0, false),
        ] {
            let mut inputs = json!({"production_path":"prod.md", "output_path":"Final/check.mp4", "work_dir":"Final/check-work", "scenes":[{"id":1,"video":"video.mp4","audio":"short.wav","trim":"custom","start":start,"duration":duration}]}).as_object().unwrap().clone();
            if use_map {
                fs::write(root.join("map.yaml"), format!("assemble:\n  scenes:\n    1:\n      video: video.mp4\n      audio: short.wav\n      trim: custom\n      start: {start}\n      duration: {duration}\n")).unwrap();
                inputs.remove("scenes");
                inputs.insert("assemble_map".into(), json!("map.yaml"));
            }
            let result = workbench_host::assemble::validate_plan(&inputs, root.to_str());
            assert_eq!(
                result.is_ok(),
                valid,
                "map={use_map}, start={start}, duration={duration}: {result:?}"
            );
            let notes = workbench_host::assemble::approval_notes(&inputs, root.to_str());
            if !valid {
                assert!(result.unwrap_err().contains("audio"));
                assert!(!notes.contains("host-validated"), "{notes}");
            }
            assert!(
                !root.join("Final").exists(),
                "validation must not write outputs"
            );
        }
    }
    // A voiceover replaces embedded audio too; an unused alternate WAV must
    // not constrain a scene that retains its embedded audio.
    successful(
        Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(root.join("video.mp4"))
            .args([
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=mono",
                "-t",
                "4",
                "-c:v",
                "copy",
                "-c:a",
                "aac",
            ])
            .arg(root.join("embedded.mp4")),
    );
    let inputs = json!({"production_path":"prod.md", "output_path":"Final/embedded.mp4", "work_dir":"Final/embedded-work", "scenes":[{"id":1,"video":"embedded.mp4","audio":"short.wav","trim":"custom","start":2,"duration":1}]}).as_object().unwrap().clone();
    for (mode, valid) in [("voiceover", false), ("dialogue", true)] {
        fs::write(
            root.join("prod.md"),
            format!("### Scene 1\n```yaml\nspeech:\n  mode: {mode}\n```\n"),
        )
        .unwrap();
        assert_eq!(
            workbench_host::assemble::validate_plan(&inputs, root.to_str()).is_ok(),
            valid,
            "speech mode {mode}"
        );
    }
    assert!(!root.join("Final").exists());
}

#[test]
fn yaml_scene_custom_windows_survive_field_order_and_global_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let text = "assemble:\n  scenes:\n    - id: 2\n      trim: custom\n      duration: 1.75\n      start: 0.25\n      video: two.mp4\n    - id: 7\n      start: 1.5\n      duration: 0.625\n      trim: custom\n      video: seven.mp4\n  trim: handles\n";
    fs::write(dir.path().join("map.yaml"), text).unwrap();
    let mut inputs = json!({"assemble_map":"map.yaml"})
        .as_object()
        .unwrap()
        .clone();
    workbench_host::assemble::merge_assemble_map_into_inputs(
        &mut inputs,
        Some(dir.path().to_str().unwrap()),
    )
    .unwrap();
    assert_eq!(
        inputs["scenes"],
        json!([
            {"id":2,"video":"two.mp4","trim":"custom","start":0.25,"duration":1.75},
            {"id":7,"video":"seven.mp4","trim":"custom","start":1.5,"duration":0.625}
        ])
    );
    assert_eq!(inputs.get("trim"), Some(&json!("handles")));
}
