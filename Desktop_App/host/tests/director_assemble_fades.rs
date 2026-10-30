//! Connected Director -> loopback planner HTTP -> approval -> real ffmpeg render.
//! The planner returns deterministic actions, not live-model or Electron/UI proof.
//! All project/settings/media writes are isolated in a disposable scratch project.
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};
use workbench_host::workflow::{import::persist_state, state::ProjectState};
use workbench_host::{ctx::HostCtx, director, pending, session};

struct SessionGuard;
impl Drop for SessionGuard {
    fn drop(&mut self) {
        pending::cancel_all();
        session::close();
    }
}

fn successful(mut command: Command) -> Output {
    let output = command.output().expect("ffmpeg/ffprobe must be installed");
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Decode every frame, reducing only after decode, so first/last really are the
/// first/last encoded pictures (not an imprecise seek or container-duration guess).
fn brightness(path: &Path) -> (f64, f64, f64) {
    let mut command = Command::new("ffmpeg");
    command.args(["-v", "error", "-i"]).arg(path).args([
        "-map",
        "0:v:0",
        "-vf",
        "scale=8:8",
        "-pix_fmt",
        "gray",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    let pixels = successful(command).stdout;
    assert_eq!(pixels.len() % 64, 0, "incomplete decoded frame");
    let frames: Vec<f64> = pixels
        .chunks_exact(64)
        .map(|frame| frame.iter().map(|v| f64::from(*v)).sum::<f64>() / 64.0)
        .collect();
    assert!(
        frames.len() >= 20,
        "fixture must contain enough real frames"
    );
    (frames[0], frames[frames.len() / 2], *frames.last().unwrap())
}

fn assert_master(path: &Path) {
    let mut command = Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path);
    let probe: Value = serde_json::from_slice(&successful(command).stdout).unwrap();
    let streams = probe["streams"].as_array().unwrap();
    let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    assert_eq!(video["width"], 1080);
    assert_eq!(video["height"], 1920);
    assert!(streams.iter().any(|s| s["codec_type"] == "audio"));
    let duration: f64 = probe["format"]["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (0.9..1.2).contains(&duration),
        "unexpected runtime {duration}"
    );
}

/// One actual HTTP request, with a bounded listener and socket timeout. Dropping
/// the listener before approval also ensures rendering does not need a provider.
fn planned_turn(
    app: &HostCtx,
    inputs: Value,
    user: &str,
) -> (director::DirectorTurnResponse, Value) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    fs::write(
        app.settings_path(),
        json!({
            "apis": [{"id":"loopback-planner", "name":"Deterministic local planner fixture",
                "kind":"atlas-cloud", "baseUrl":base, "apiKey":"", "secretKey":"", "enabled":true}],
            "capabilityDefaults":{}
        })
        .to_string(),
    )
    .unwrap();
    let action = json!({"action":"run_specialist", "specialist":"assemble", "inputs":inputs});
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < deadline,
                        "Director never dispatched planner HTTP"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("loopback accept: {e}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let request = loop {
            let mut buffer = [0u8; 4096];
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0, "incomplete HTTP request");
            bytes.extend_from_slice(&buffer[..n]);
            assert!(bytes.len() < 4 * 1024 * 1024, "unbounded planner request");
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
                    .expect("HTTP content length");
                if bytes.len() >= end + 4 + length {
                    break serde_json::from_slice::<Value>(&bytes[end + 4..end + 4 + length])
                        .unwrap();
                }
            }
        };
        let reply = json!({"choices":[{"message":{"content":format!("<director_action>\n{action}\n</director_action>")}}]}).to_string();
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
        request
    });
    let request = serde_json::from_value(json!({
        "model":"deepseek-ai/deepseek-v4-pro", "apiId":"loopback-planner",
        "messages":[{"role":"user", "content":user}], "context":{}
    }))
    .unwrap();
    let response = director::director_turn(app.clone(), request);
    let received = server.join().unwrap();
    (response.unwrap(), received)
}

#[test]
fn director_assembly_inline_fades_survive_approval_and_change_decoded_edges_without_source_edits() {
    // One test in this binary: session/pending globals cannot race another test.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let production = "### Scene 1\n```yaml\ntransition_in: fade\ntransition_out: fade\nspeech:\n  mode: none\n```\n";
    let production_path = root.join("prod.md");
    let source = root.join("white.mp4");
    fs::write(&production_path, production).unwrap();
    let mut generate = Command::new("ffmpeg");
    generate
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=white:s=90x160:r=24:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&source);
    successful(generate);
    let production_before = fs::read(&production_path).unwrap();
    let source_before = fs::read(&source).unwrap();
    let source_pixels = brightness(&source);
    assert!(source_pixels.0 > 230.0 && source_pixels.2 > 230.0);
    persist_state(&root, &ProjectState::default()).unwrap();
    session::open(root.to_str().unwrap()).unwrap();
    let _guard = SessionGuard;
    let app = HostCtx::new(dir.path().join("isolated-settings"));
    let mut rendered = Vec::new();
    for (name, overrides, expected_in, expected_out) in [
        ("baseline", json!({}), true, true),
        (
            "no-fades",
            json!({"fade_in":false,"fade_out":false}),
            false,
            false,
        ),
        ("no-opening", json!({"fade_in":false}), false, true),
    ] {
        let relative_output = format!("Final/{name}.mp4");
        let relative_work = format!("Final/work-{name}");
        let output = root.join(&relative_output);
        let work = root.join(&relative_work);
        let mut inputs = json!({"production_path":"prod.md", "scenes":[{"id":1,"video":"white.mp4"}],
            "trim":"full", "caption_mode":"none", "output_path":relative_output, "work_dir":relative_work});
        inputs
            .as_object_mut()
            .unwrap()
            .extend(overrides.as_object().unwrap().clone());
        // No assemble-map file is created or supplied in any scenario.
        let (card, received) = planned_turn(
            &app,
            inputs,
            &format!("Render {relative_output}; {name}; do not edit prod.md or the source clip."),
        );
        assert_eq!(
            card.tool_events,
            vec!["approval_required:assemble"],
            "{}",
            card.reply
        );
        assert!(!output.exists(), "{name}: master rendered before approval");
        assert!(
            !work.exists(),
            "{name}: intermediate media written before approval"
        );
        assert!(
            card.reply.contains("Assemble plan (host-validated)"),
            "{}",
            card.reply
        );
        let expected_notes = format!("edge fades: in={expected_in} out={expected_out}");
        assert!(
            card.reply.contains(&expected_notes),
            "approval_notes must explicitly show `{expected_notes}`; got:\n{}",
            card.reply
        );
        // Inspect the SYSTEM message actually transmitted, not the canned reply
        // or user message, to verify that models can discover the new inputs.
        let system = received["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|message| message["role"] == "system")
            .filter_map(|message| message["content"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let assemble_help = system
            .split("### assemble inputs")
            .nth(1)
            .unwrap()
            .split("### text_sync inputs")
            .next()
            .unwrap();
        if name == "no-fades" {
            assert!(assemble_help.contains("fade_in") && assemble_help.contains("fade_out"),
                "transmitted assemble guidance must advertise both inline fade inputs: {assemble_help}");
            let lower = assemble_help.to_lowercase();
            assert!(lower.contains("no map") || lower.contains("without a map") || lower.contains("map is optional") || lower.contains("do not require"),
                "transmitted assemble guidance must explicitly say no map is required: {assemble_help}");
        }
        let token = card
            .reply
            .split(pending::APPROVE_TOKEN_PREFIX)
            .nth(1)
            .and_then(|rest| rest.split("]]").next())
            .expect("emitted one-time approval token")
            .to_string();
        assert!(!token.is_empty());
        assert_eq!(fs::read(&production_path).unwrap(), production_before);
        assert_eq!(fs::read(&source).unwrap(), source_before);
        let completed = director::director_approve(app.clone(), token.clone()).unwrap();
        assert_eq!(
            completed.tool_events,
            vec!["approve:assemble"],
            "{}",
            completed.reply
        );
        assert!(!completed.reply.contains(pending::APPROVE_TOKEN_PREFIX));
        assert_master(&output);
        let pixels = brightness(&output);
        eprintln!("{name}: decoded first/middle/last gray={pixels:?}");
        assert!(
            pixels.1 > 230.0,
            "middle of white clip must remain bright: {pixels:?}"
        );
        if expected_in {
            assert!(
                pixels.0 < 45.0,
                "opening production fade missing: {pixels:?}"
            );
        } else {
            assert!(
                pixels.0 > 230.0,
                "disabled opening fade still darkens picture: {pixels:?}"
            );
        }
        if expected_out {
            assert!(
                pixels.2 < 75.0,
                "closing production fade missing: {pixels:?}"
            );
        } else {
            assert!(
                pixels.2 > 230.0,
                "disabled closing fade still darkens picture: {pixels:?}"
            );
        }
        let output_before_replay = fs::read(&output).unwrap();
        assert!(
            director::director_approve(app.clone(), token).is_err(),
            "approval cannot be replayed"
        );
        assert_eq!(fs::read(&output).unwrap(), output_before_replay);
        assert_eq!(fs::read(&production_path).unwrap(), production_before);
        assert_eq!(fs::read(&source).unwrap(), source_before);
        rendered.push(pixels);
    }
    assert!(
        rendered[1].0 - rendered[0].0 > 180.0,
        "override must change opening pixels"
    );
    assert!(
        rendered[1].2 - rendered[0].2 > 150.0,
        "override must change closing pixels"
    );
    assert!(
        rendered[2].0 > 230.0 && rendered[2].2 < 75.0,
        "one-sided override must preserve closing fade"
    );
}
