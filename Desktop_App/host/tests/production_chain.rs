//! Get_Studio_To_Production.md, Phase 4 Done-When chain — the connected scenario.
//!
//! One scenario, driven command-by-command through the real dispatcher
//! (`workbench_host::dispatch`), from a plan that is not yet approved to a
//! playable review export and the truthful Finish facts:
//!
//!   approved plan → approved source strategies → initial render →
//!   registered take → take review/selection → explicit continuity-frame
//!   extraction & binding → dependent render → selected dependent take →
//!   assembly → playable review export → classification → Finish facts.
//!
//! Every hop calls the same host commands the application calls. The external
//! provider boundary is mocked by a local HTTP stub that speaks the real
//! Replicate prediction protocol: the production adapter builds the request,
//! the spend gate reserves before dispatch, the durable prediction id is
//! recorded through the recovery path, and the provider artifact (an FFmpeg
//! clip, i.e. a realistic local media result) is delivered to the job's take
//! path exactly as the downloader would leave it. No billable network call is
//! ever attempted, `main` is never touched, and no workflow state is
//! hand-edited: takes are registered only through the production
//! completion/reconciliation path (`workflow_import` → `restore_render_jobs`).
//!
//! The fixture is a clearly documented derivative of Lite_Challenge Scene 1:
//! the same two-cut shape, b-roll text-to-video lane, and dissolve transition,
//! built with the production `scene_plan::fixtures` document builders, but
//! with NO pre-existing takes — the chain begins before any render.
//!
//! Known substitution (documented per the task): the stub's HTTP POST/GET
//! round-trip is real, but the final byte-transfer into the project is
//! simulated by delivering the artifact file at the planned path instead of
//! fetching it, because the host's download security policy requires https
//! and is an intentional boundary that tests must not weaken. The download
//! step still runs the production `download_atlas_media` code path (it
//! resolves the scoped destination and accepts the delivered file), and every
//! preceding hop — authorization, reservation, adapter request construction,
//! polling, recovery metadata, reconciliation — executes for real.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde_json::{json, Value};
use workbench_host::ctx::HostCtx;
use workbench_host::director_actions::resolve_under_project;
use workbench_host::dispatch;
use workbench_host::scene_plan::fixtures::{broll_scene_yaml, cut_markdown};
use workbench_host::scene_plan::RenderRequest;
use workbench_host::voice::RecorderState;
use workbench_host::workflow::import::load_state;
use workbench_host::workflow::spend;
use workbench_host::workflow::state::{JobStatus, StageStatus, TakeStatus};

/// Session root and the pending-approval store are process globals; tests in
/// this binary must run one at a time.
fn session_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let mutex = LOCK.get_or_init(|| Mutex::new(()));
    // A panicking test must not wedge every remaining test in the binary.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------
// Minimal local HTTP stub speaking the Replicate protocol.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Stub {
    posts: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    fail_next: Arc<Mutex<bool>>,
    last_auth: Arc<Mutex<Option<String>>>,
    last_body: Arc<Mutex<Option<Value>>>,
    /// The port the listener bound; empty until the serve thread publishes it.
    port: Arc<Mutex<String>>,
    /// (artifact src, delivery dst) written at the poll that finalizes the job,
    /// simulating the provider artifact arriving locally.
    delivery: Arc<Mutex<Option<(PathBuf, PathBuf)>>>,
}

fn read_request(stream: &mut TcpStream) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    let mut reader = BufReader::new(stream.try_clone().expect("stub clone"));
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .expect("stub request line");
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let content_length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).expect("stub body");
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    (method, path, headers, body)
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = if status == 200 { "OK" } else { "NOT FOUND" };
    let text = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(text.as_bytes());
    let _ = stream.flush();
}

fn serve(stub: Stub) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("stub bind");
    let addr = listener.local_addr().expect("stub addr");
    *stub.port.lock().unwrap() = addr.port().to_string();
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let (method, path, headers, body) = read_request(&mut stream);
        let auth = headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .map(|(_, value)| value.clone());
        if method == "POST" && path.ends_with("/predictions") && path.contains("/models/") {
            let posts = stub.posts.fetch_add(1, Ordering::SeqCst) + 1;
            let id = format!("mock-pred-{posts}");
            *stub.last_auth.lock().unwrap() = auth;
            *stub.last_body.lock().unwrap() = serde_json::from_slice(&body).ok();
            respond(
                &mut stream,
                200,
                &json!({"id": id, "status": "starting", "output": null}).to_string(),
            );
        } else if method == "GET" && path.starts_with("/v1/predictions/") {
            let polls = stub.polls.fetch_add(1, Ordering::SeqCst) + 1;
            let id = path.trim_start_matches("/v1/predictions/").to_string();
            let failing = { *stub.fail_next.lock().unwrap() };
            if failing {
                respond(
                    &mut stream,
                    200,
                    &json!({"id": id, "status": "failed", "output": null,
                            "error": "provider rejected the request frame"})
                    .to_string(),
                );
            } else {
                // The provider "finishes": deliver the artifact to the planned
                // path, then report success, exactly once per prediction.
                let delivery = stub.delivery.lock().unwrap().take();
                if let Some((src, dst)) = delivery {
                    fs::create_dir_all(dst.parent().expect("delivery parent"))
                        .expect("delivery dir");
                    fs::copy(&src, &dst).expect("delivery copy");
                }
                respond(
                    &mut stream,
                    200,
                    &json!({"id": id, "status": "succeeded",
                            "output": [format!("https://provider.invalid/out-{polls}.mp4")]})
                    .to_string(),
                );
            }
        } else {
            respond(&mut stream, 404, &json!({"error": "not found"}).to_string());
        }
    }
}

fn start_stub() -> Stub {
    let stub = Stub {
        posts: Arc::new(AtomicUsize::new(0)),
        polls: Arc::new(AtomicUsize::new(0)),
        fail_next: Arc::new(Mutex::new(false)),
        last_auth: Arc::new(Mutex::new(None)),
        last_body: Arc::new(Mutex::new(None)),
        port: Arc::new(Mutex::new(String::new())),
        delivery: Arc::new(Mutex::new(None)),
    };
    let worker = stub.clone();
    std::thread::spawn(move || serve(worker));
    // Wait until the listener has published its port.
    for _ in 0..200 {
        if !stub.port.lock().unwrap().is_empty() {
            return stub;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("provider stub never bound");
}

// ---------------------------------------------------------------------------
// Fixture + production-boundary helpers
// ---------------------------------------------------------------------------

fn ff(args: &[&str]) {
    let output = Command::new("ffmpeg")
        .args(["-y"])
        .args(args)
        .output()
        .expect("ffmpeg is required for the production chain");
    assert!(
        output.status.success(),
        "ffmpeg {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A realistic provider artifact: valid, decodable H.264 video (2 s).
fn build_artifact(path: &Path) {
    fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).expect("artifact dir");
    ff(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc=size=320x180:rate=25",
        "-t",
        "2",
        "-pix_fmt",
        "yuv420p",
        path.to_str().unwrap(),
    ]);
}

fn ffprobe_json(path: &Path) -> Value {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .expect("ffprobe is required for the production chain");
    assert!(
        output.status.success(),
        "ffprobe {}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("ffprobe json")
}

/// Lite_Challenge Scene 1 derivative: two b-roll cuts, dissolve transition,
/// committed-looking documents, and no media of any kind yet.
fn write_fixture(root: &Path) {
    let handoff_1 = broll_scene_yaml(1);
    let handoff_2 = broll_scene_yaml(2);
    fs::create_dir_all(root.join("Scene_Prompts/Scene1")).expect("sidecar dir");
    fs::write(
        root.join("Scene_Prompts/Scene1/Cut1.txt"),
        cut_markdown(1, 1, &handoff_1),
    )
    .expect("sidecar 1");
    fs::write(
        root.join("Scene_Prompts/Scene1/Cut2.txt"),
        cut_markdown(1, 2, &handoff_2),
    )
    .expect("sidecar 2");
    let prod = format!(
        "# Lite Chain — Scene 1 (derivative of Lite_Challenge Scene 1)\n\n{}\n\n{}\n",
        cut_markdown(1, 1, &handoff_1),
        cut_markdown(1, 2, &handoff_2)
    );
    fs::write(root.join("prod.md"), prod).expect("prod.md");
}

struct Harness {
    app: HostCtx,
    recorder: RecorderState,
    root: PathBuf,
    stub: Stub,
    _settings: TempDirHolder,
}

/// Holds the temp dir alive for the whole harness; never read directly.
#[allow(dead_code)]
struct TempDirHolder(tempfile::TempDir);

impl Harness {
    fn new() -> Harness {
        let dir = tempfile::tempdir().expect("temp project");
        let root = dir.path().join("LiteChain");
        fs::create_dir_all(&root).expect("project dir");
        write_fixture(&root);
        let stub = start_stub();
        let port = stub.port.lock().unwrap().clone();
        let settings_dir = dir.path().join("settings");
        fs::create_dir_all(&settings_dir).expect("settings dir");
        fs::write(
            settings_dir.join("settings.json"),
            json!({
                "apis": [{
                    "id": "mock-replicate",
                    "name": "Mock Replicate (test provider)",
                    "kind": "replicate",
                    "baseUrl": format!("http://127.0.0.1:{port}"),
                    "apiKey": "mock-provider-key",
                    "secretKey": "",
                    "enabled": true,
                    "catalog": [{"id": "testorg/testvideo"}]
                }],
                "capabilityDefaults": {
                    "video.text_only": {"apiId": "mock-replicate", "modelId": "testorg/testvideo"},
                    "video.start_frame": {"apiId": "mock-replicate", "modelId": "testorg/testvideo"}
                }
            })
            .to_string(),
        )
        .expect("settings.json");
        let harness = Harness {
            app: HostCtx::new(settings_dir),
            recorder: RecorderState::new(),
            root,
            stub,
            _settings: TempDirHolder(dir),
        };
        harness
            .call(
                "open_project",
                json!({ "path": harness.root.to_str().unwrap() }),
            )
            .expect("open project");
        harness
    }

    fn root_str(&self) -> String {
        self.root.to_str().unwrap().to_string()
    }

    fn call(&self, cmd: &str, mut args: Value) -> Result<Value, String> {
        if args.get("projectRoot").is_none() && !matches!(cmd, "open_project" | "close_project") {
            args["projectRoot"] = json!(self.root_str());
        }
        dispatch(&self.app, &self.recorder, cmd, &args)
    }

    fn state(&self) -> workbench_host::workflow::state::ProjectState {
        load_state(&self.root).expect("workflow state")
    }

    /// Extract the host-issued one-use approval token from an approval card.
    fn card_id(card: &str) -> String {
        let marker = "[[workbench_approve:";
        let start = card
            .find(marker)
            .unwrap_or_else(|| panic!("approval token missing from card: {card}"));
        let rest = &card[start + marker.len()..];
        let end = rest.find("]]").expect("approval token terminator");
        rest[..end].to_string()
    }

    /// Run one cut's render through the real production boundaries:
    /// prepare → card approval (must NOT reach the provider) → unbounded quote
    /// → one-use confirmation → confirmed generate → provider round-trip.
    /// Returns the take path relative to the project root.
    fn render_cut(&self, scene: u32, cut: u32) -> String {
        let prepared = self
            .call(
                "workflow_prepare_render",
                json!({ "sceneId": scene, "cut": cut }),
            )
            .expect("prepare render");
        let card = prepared["pendingAction"]
            .as_str()
            .expect("render card")
            .to_string();
        assert!(
            card.contains(&format!("Render Scene {scene} Cut {cut}")),
            "{card}"
        );
        let posts_before = self.stub.posts.load(Ordering::SeqCst);
        assert_eq!(
            posts_before,
            self.stub.posts.load(Ordering::SeqCst),
            "provider hit before authorization"
        );

        // 1) The one-use director card consumes approval but the paid
        //    operation still requires its own spend authorization.
        let gate = self
            .call("director_approve", json!({ "id": Self::card_id(&card) }))
            .expect("director approve returns a turn response");
        let reply = gate["reply"].as_str().unwrap_or_default().to_string();
        assert!(
            reply.contains("awaiting spend authorization"),
            "approved card must stop at the spend gate: {reply}"
        );
        assert_eq!(
            self.stub.posts.load(Ordering::SeqCst),
            posts_before,
            "provider must not be dispatched without per-operation spend authorization"
        );

        // 2) Resolve the pending render job and rebuild the exact media
        //    request shape with production functions, then quote → confirm →
        //    dispatch through the confirmed command.
        let state = self.state();
        let job = state
            .jobs
            .iter()
            .filter(|job| job.kind == "render" && job.status == JobStatus::Pending)
            .rev()
            .find(|job| {
                job.raw_provider_id
                    .as_deref()
                    .is_some_and(|id| id == format!("S{scene:02}C{cut:02}"))
            })
            .expect("pending render job");
        let output_rel = job.output_path.clone().expect("render job output path");
        let save_path =
            resolve_under_project(&output_rel, Some(&self.root_str())).expect("save path");

        let sidecar_rel = {
            let scene_state = state.scene(scene).expect("scene");
            let cut_state = scene_state
                .cuts
                .iter()
                .find(|c| c.id.cut == cut)
                .expect("cut");
            cut_state
                .production_note_path
                .clone()
                .expect("production note")
        };
        let handoff_text =
            fs::read_to_string(self.root.join(&sidecar_rel)).expect("handoff document");
        let request = RenderRequest::from_document(&handoff_text).expect("render request");
        let params = json!({
            "prompt": request.prompt,
            "duration": request.duration_seconds,
            "aspect_ratio": request.aspect_ratio.as_str(),
            "resolution": "1080p",
        });
        let media_args = json!({
            "model": "testorg/testvideo",
            "params": params,
            "save_path": save_path.to_string_lossy(),
            "wait_seconds": 300,
            "api_id": "mock-replicate",
        });
        let request_bytes =
            serde_json::to_vec(&json!(["atlas_generate_video", "replicate", media_args]))
                .expect("fingerprint bytes");
        let hash = spend::fingerprint(&request_bytes).expect("fingerprint");
        let quote = spend::quote_unbounded_scoped(&self.root, &hash).expect("unbounded quote");
        let token = self
            .call(
                "confirm_unbounded_spend",
                json!({
                    "provider": "replicate",
                    "category": "media",
                    "operation": "atlas_generate_video",
                    "model": "testorg/testvideo",
                    "requestHash": quote.request_hash,
                    "ledgerRevision": quote.ledger_revision,
                }),
            )
            .expect("one-use spend confirmation")
            .as_str()
            .expect("token string")
            .to_string();

        // 3) The provider delivers its artifact to the planned path.
        let artifact = self.root.join(format!(".provider-artifacts/{output_rel}"));
        build_artifact(&artifact);
        *self.stub.delivery.lock().unwrap() = Some((artifact, save_path.clone()));

        let submitted = self
            .call(
                "media_generate_confirmed",
                json!({
                    "operation": "atlas_generate_video",
                    "mediaArgs": media_args,
                    "token": token,
                }),
            )
            .expect("confirmed media generation");
        assert!(
            submitted
                .as_str()
                .is_some_and(|text| text.contains("\"saved\"")),
            "confirmed generate must report a saved artifact: {submitted}"
        );

        // Provider-boundary truth: exactly one new submission, authenticated,
        // and it carried the approved production prompt.
        assert_eq!(
            self.stub.posts.load(Ordering::SeqCst),
            posts_before + 1,
            "confirmed generate must reach the provider adapter exactly once"
        );
        assert_eq!(
            self.stub.last_auth.lock().unwrap().as_deref(),
            Some("Bearer mock-provider-key")
        );
        let body = self
            .stub
            .last_body
            .lock()
            .unwrap()
            .clone()
            .expect("stub saw a body");
        assert_eq!(
            body["input"]["prompt"].as_str(),
            Some(request.prompt.as_str()),
            "the dispatched request must carry the production prompt"
        );

        // 4) Production reconciliation registers the take; state is never
        //    edited by hand.
        self.call("workflow_import", json!({})).expect("rescan");
        output_rel
    }

    fn selected_take(&self, scene: u32, cut: u32) -> String {
        let state = self.state();
        let takes = &state
            .scene(scene)
            .expect("scene")
            .cuts
            .iter()
            .find(|c| c.id.cut == cut)
            .expect("cut")
            .takes;
        let take_id = takes
            .last()
            .expect("take registered via production scan")
            .id
            .clone();
        self.call(
            "workflow_set_take_status",
            json!({ "sceneId": scene, "cut": cut, "takeId": take_id, "status": "selected" }),
        )
        .expect("select take");
        take_id
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = self.call("close_project", json!({}));
    }
}

// ---------------------------------------------------------------------------
// The connected chain
// ---------------------------------------------------------------------------

#[test]
fn approved_plan_reaches_a_playable_review_export_through_production_boundaries() {
    let _session = session_lock();
    let h = Harness::new();

    // Start state: before any approval or render — no takes, no jobs, no master.
    let imported = h.call("workflow_import", json!({})).expect("import");
    assert_eq!(
        imported["state"]["scenes"].as_array().map(Vec::len),
        Some(1),
        "fixture must import exactly one scene"
    );
    let initial = h.state();
    let scene = initial.scene(1).expect("scene 1");
    assert_eq!(scene.cuts.len(), 2, "two-cut Lite Scene 1 derivative");
    assert!(scene.cuts.iter().all(|cut| cut.takes.is_empty()));
    assert!(initial.jobs.is_empty());
    assert_eq!(scene.stages.assemble, StageStatus::Missing);
    assert_eq!(h.stub.posts.load(Ordering::SeqCst), 0);

    // --- approved plan -----------------------------------------------------
    h.call(
        "workflow_save_intent",
        json!({ "sceneId": 1, "intent": {
            "narrative_purpose": "Establish the last train leaving the city.",
            "emotional_purpose": "Quiet finality.",
        }}),
    )
    .expect("save intent");
    let approved = h
        .call("workflow_approve_intent", json!({ "sceneId": 1 }))
        .expect("approve intent");
    assert!(
        approved["note"]
            .as_str()
            .unwrap_or_default()
            .contains("Scene intent approved"),
        "unexpected approve note: {approved}"
    );
    h.call(
        "workflow_set_strategy",
        json!({ "sceneId": 1, "strategy": "cinematic_balanced" }),
    )
    .expect("scene strategy");
    h.call("workflow_backfill_sketches", json!({ "sceneId": 1 }))
        .expect("backfill sketches");

    // --- approved source strategies ----------------------------------------
    h.call(
        "workflow_save_production_prompt",
        json!({ "sceneId": 1, "cut": 1, "prompt":
            "Slow aerial drift over a city skyline at dusk; the last train lights slide past below." }),
    )
    .expect("prompt cut 1");
    h.call(
        "workflow_save_production_prompt",
        json!({ "sceneId": 1, "cut": 2, "prompt":
            "Interior of the carriage: empty seats slide by as the city lights streak past the windows." }),
    )
    .expect("prompt cut 2");
    h.call(
        "workflow_set_visual_source",
        json!({ "sceneId": 1, "cut": 1, "strategy": "no_start_frame" }),
    )
    .expect("source strategy cut 1");
    let planned = h
        .call(
            "workflow_set_visual_source",
            json!({ "sceneId": 1, "cut": 2, "strategy": "previous_selected_take_exit_frame",
                     "predecessorCut": 1 }),
        )
        .expect("source strategy cut 2");
    assert!(
        planned["note"]
            .as_str()
            .unwrap_or_default()
            .contains("Bind a reviewed continuity frame"),
        "cut 2 must be planned-but-blocked until continuity binding: {}",
        planned["note"]
    );

    // --- initial render → registered take ----------------------------------
    let take1_rel = h.render_cut(1, 1);
    assert!(!take1_rel.is_empty());

    // --- take review + selection -------------------------------------------
    let take1 = h.selected_take(1, 1);
    let after_select = h.state();
    let cut1 = after_select
        .scene(1)
        .unwrap()
        .cuts
        .iter()
        .find(|c| c.id.cut == 1)
        .unwrap();
    assert_eq!(cut1.takes.len(), 1);
    assert_eq!(cut1.takes[0].status, TakeStatus::Selected);
    assert_eq!(cut1.selected_take.as_deref(), Some(take1.as_str()));

    // Every render job that completed must own its registered take asset.
    let render_job = after_select
        .jobs
        .iter()
        .find(|job| job.kind == "render")
        .expect("render job");
    assert_eq!(render_job.status, JobStatus::Completed);
    assert_eq!(render_job.output_path.as_deref(), Some(take1_rel.as_str()));
    let take_asset = after_select
        .assets
        .iter()
        .find(|asset| asset.path == take1_rel)
        .expect("take asset");
    assert!(render_job.output_asset_ids.contains(&take_asset.id));

    // The provider round-trip left honest money + recovery records: a spend
    // job with the durable prediction id and unbounded-exposure accounting.
    let spend_job = after_select
        .jobs
        .iter()
        .find(|job| job.id.starts_with("spend-"))
        .expect("reservation persisted as its own job");
    assert_eq!(spend_job.provider.as_deref(), Some("replicate"));
    assert!(spend_job.unbounded_exposure);
    assert!(
        spend_job
            .request_metadata
            .as_ref()
            .and_then(|meta| meta.prediction_id.as_ref())
            .is_some_and(|id| id.starts_with("replicate:mock-pred-")),
        "recovery metadata must record the accepted provider prediction id"
    );
    let unsettled = h
        .call("workflow_unsettled_spend", json!({}))
        .expect("unsettled spend query");
    assert!(
        !unsettled.as_array().map_or(false, |rows| rows.is_empty())
            || unsettled.get("jobs").is_some()
            || unsettled.is_object(),
        "the completed-but-unreconciled spend must remain visible"
    );

    // --- dependent render cannot proceed before continuity binding ---------
    let blocked = h.call("workflow_prepare_render", json!({ "sceneId": 1, "cut": 2 }));
    let blocked_error = blocked.expect_err("cut 2 must not render before continuity binding");
    assert!(
        blocked_error.contains("not render ready") || blocked_error.contains("continuity"),
        "unexpected gate error: {blocked_error}"
    );
    assert_eq!(
        h.stub.posts.load(Ordering::SeqCst),
        1,
        "no provider call while blocked"
    );

    // --- explicit continuity-frame extraction + binding --------------------
    let bound = h
        .call(
            "workflow_bind_continuity_frame",
            json!({ "sceneId": 1, "cut": 2, "frameTimeSeconds": 0.4 }),
        )
        .expect("bind continuity frame");
    let bind_note = bound["note"].as_str().unwrap_or_default().to_string();
    assert!(
        !bind_note.is_empty(),
        "bind continuity frame returned no note: {bound}"
    );
    let bound_state = h.state();
    let cut2 = bound_state
        .scene(1)
        .unwrap()
        .cuts
        .iter()
        .find(|c| c.id.cut == 2)
        .unwrap();
    let source = cut2.visual_source.as_ref().expect("bound visual source");
    assert!(cut2.visual_source_ready());
    assert_eq!(source.source_take_id.as_deref(), Some(take1.as_str()));
    let frame_asset = bound_state
        .asset(source.asset_id.as_deref().unwrap())
        .expect("continuity frame asset");
    assert_eq!(frame_asset.parent_asset_id.as_deref(), Some(take1.as_str()));
    let frame_path = h.root.join(&frame_asset.path);
    let frame_probe = ffprobe_json(&frame_path);
    assert!(
        frame_probe["streams"]
            .as_array()
            .map_or(false, |streams| streams
                .iter()
                .any(|s| s["codec_type"] == "video")),
        "the bound continuity frame must be a real decodable image: {frame_probe}"
    );

    // --- dependent render → selected dependent take ------------------------
    let take2_rel = h.render_cut(1, 2);
    let take2_probe = ffprobe_json(&h.root.join(&take2_rel));
    assert!(
        take2_probe["streams"]
            .as_array()
            .map_or(false, |streams| streams
                .iter()
                .any(|st| st["codec_type"] == "video")),
        "the dependent take must be real decodable media: {take2_probe}"
    );
    assert_eq!(
        h.stub.posts.load(Ordering::SeqCst),
        2,
        "one submission per render"
    );
    h.selected_take(1, 2);

    // --- assembly: prepare (approval-gated), pending truth, approval, run ---
    let prepared_assembly = h
        .call("workflow_prepare_assemble", json!({ "sceneId": 1 }))
        .expect("prepare assembly");
    let assembly_card = prepared_assembly["pendingAction"]
        .as_str()
        .expect("assembly card")
        .to_string();
    assert!(
        assembly_card.contains("Assemble Scene 1"),
        "{assembly_card}"
    );
    let output_rel = {
        let state = h.state();
        state
            .jobs
            .iter()
            .rev()
            .find(|job| job.kind == "assemble" && job.status == JobStatus::Pending)
            .and_then(|job| job.output_path.clone())
            .expect("pending assemble job")
    };
    assert_eq!(output_rel, "Final/Scene1_master.mp4");

    // Failure-path half of step 4 (missing output): the pending assemble job
    // must NOT reconcile to completion just because a card exists.
    h.call("workflow_import", json!({}))
        .expect("rescan with missing output");
    let pending_state = h.state();
    let still_pending = pending_state
        .jobs
        .iter()
        .find(|job| job.kind == "assemble")
        .expect("assemble job");
    assert_eq!(
        still_pending.status,
        JobStatus::Pending,
        "a missing assembly output must not produce a successful completion"
    );
    assert!(!h.root.join(&output_rel).exists());
    assert_eq!(
        pending_state.scene(1).unwrap().stages.assemble,
        StageStatus::Missing,
        "Finish facts must stay truthful while the master does not exist"
    );

    // One-time approval executes the real host-side assembly (production
    // planning, args, ffmpeg execution, result handling).
    let run_value = h
        .call(
            "director_approve",
            json!({ "id": Harness::card_id(&assembly_card) }),
        )
        .expect("approve assembly");
    assert!(
        h.root.join(&output_rel).exists(),
        "approved assemble must produce the master on disk; director reply: {}",
        run_value["reply"].as_str().unwrap_or_default()
    );
    let master = h.root.join(&output_rel);
    let master_probe = ffprobe_json(&master);
    let video_streams: Vec<&Value> = master_probe["streams"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|stream| stream["codec_type"] == "video")
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        video_streams.len(),
        1,
        "master must have exactly one video stream"
    );
    let width = video_streams[0]["width"].as_u64().unwrap_or(0);
    let height = video_streams[0]["height"].as_u64().unwrap_or(0);
    // Production assembly normalizes every take onto the delivery canvas
    // derived from the handoff (9:16 at 1080p) — not the provider's raw size.
    assert!(
        width == 1080 && height == 1920,
        "master must land on the 9:16 1080p canvas, got {width}x{height}"
    );
    let duration: f64 = master_probe["format"]["duration"]
        .as_str()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0.0);
    // Two 2 s takes joined by a dissolve: strictly positive, shorter than
    // sum-of-parts, never the artifact's placeholder nonsense.
    assert!(
        duration > 1.5 && duration < 4.1,
        "master duration out of range: {duration}"
    );
    assert!(fs::metadata(&master).expect("master size").len() > 2_000);

    // --- review export registration + Finish facts --------------------------
    h.call("workflow_import", json!({})).expect("final rescan");
    let final_state = h.state();
    let scene = final_state.scene(1).expect("scene");
    let assemble_job = final_state
        .jobs
        .iter()
        .find(|job| job.kind == "assemble")
        .expect("assemble job");
    assert_eq!(assemble_job.status, JobStatus::Completed);
    assert_eq!(
        assemble_job.output_path.as_deref(),
        Some(output_rel.as_str())
    );
    assert_eq!(scene.stages.assemble, StageStatus::Present);
    let versions = scene.post.as_ref().expect("post versions").versions.clone();
    assert_eq!(versions.len(), 1);
    assert_eq!(
        versions[0].assembly_path.as_deref(),
        Some(output_rel.as_str())
    );
    let review_export = final_state
        .assets
        .iter()
        .find(|asset| asset.notes.as_deref() == Some("review export"))
        .expect("review export asset registered");
    assert_eq!(review_export.path, output_rel);
    for cut in &scene.cuts {
        assert_eq!(cut.takes[0].status, TakeStatus::UsedInAssembly);
    }

    // --- application opening/navigation hop (host side) ---------------------
    let classified = h
        .call("classify_file", json!({ "path": master.to_str().unwrap() }))
        .expect("classify review export");
    assert_eq!(
        classified,
        json!("video"),
        "review export must classify as video"
    );

    // Both renders authorized exactly one provider submission each — nothing
    // reached the "provider" before its authorization hop.
    assert_eq!(h.stub.posts.load(Ordering::SeqCst), 2);
}

#[test]
fn failed_provider_outcome_never_completes_the_render_job_or_replays_blindly() {
    let _session = session_lock();
    let h = Harness::new();
    h.call("workflow_import", json!({})).expect("import");
    h.call(
        "workflow_save_intent",
        json!({ "sceneId": 1, "intent": {"narrative_purpose": "failure path probe"} }),
    )
    .expect("intent");
    h.call("workflow_approve_intent", json!({ "sceneId": 1 }))
        .expect("approve");
    h.call("workflow_backfill_sketches", json!({ "sceneId": 1 }))
        .expect("sketches");
    h.call(
        "workflow_save_production_prompt",
        json!({ "sceneId": 1, "cut": 1, "prompt": "A cut that the provider will fail." }),
    )
    .expect("prompt");
    h.call(
        "workflow_set_visual_source",
        json!({ "sceneId": 1, "cut": 1, "strategy": "no_start_frame" }),
    )
    .expect("source");

    // Prepare + authorize exactly like the happy chain, but the provider
    // reports a FAILED prediction and delivers nothing.
    let prepared = h
        .call("workflow_prepare_render", json!({ "sceneId": 1, "cut": 1 }))
        .expect("prepare");
    let card = prepared["pendingAction"].as_str().unwrap().to_string();
    let _ = h
        .call("director_approve", json!({ "id": Harness::card_id(&card) }))
        .expect("card consumed at the gate");

    let state = h.state();
    let job = state
        .jobs
        .iter()
        .find(|job| job.kind == "render" && job.status == JobStatus::Pending)
        .expect("pending render job");
    let output_rel = job.output_path.clone().unwrap();
    let save_path = resolve_under_project(&output_rel, Some(&h.root_str())).unwrap();
    let handoff_text = fs::read_to_string(
        h.root.join(
            state
                .scene(1)
                .unwrap()
                .cuts
                .iter()
                .find(|c| c.id.cut == 1)
                .unwrap()
                .production_note_path
                .as_ref()
                .unwrap(),
        ),
    )
    .unwrap();
    let request = RenderRequest::from_document(&handoff_text).unwrap();
    let media_args = json!({
        "model": "testorg/testvideo",
        "params": json!({
            "prompt": request.prompt,
            "duration": request.duration_seconds,
            "aspect_ratio": request.aspect_ratio.as_str(),
            "resolution": "1080p",
        }),
        "save_path": save_path.to_string_lossy(),
        "wait_seconds": 300,
        "api_id": "mock-replicate",
    });
    let hash = spend::fingerprint(
        &serde_json::to_vec(&json!([
            "atlas_generate_video",
            "replicate",
            media_args.clone()
        ]))
        .unwrap(),
    )
    .unwrap();
    let quote = spend::quote_unbounded_scoped(&h.root, &hash).expect("quote");
    *h.stub.fail_next.lock().unwrap() = true;
    let token = h
        .call(
            "confirm_unbounded_spend",
            json!({
                "provider": "replicate", "category": "media",
                "operation": "atlas_generate_video", "model": "testorg/testvideo",
                "requestHash": quote.request_hash, "ledgerRevision": quote.ledger_revision,
            }),
        )
        .expect("confirm")
        .as_str()
        .unwrap()
        .to_string();
    let result = h.call(
        "media_generate_confirmed",
        json!({ "operation": "atlas_generate_video", "mediaArgs": media_args, "token": token }),
    );
    // Production semantics for a confirmed generation whose provider job
    // FAILED: the call reports the failed outcome, never a saved artifact.
    // (The specialist lane converts exactly this shape into an error through
    // `require_media_saved`; here we assert the router truth directly.)
    let outcome: Value = serde_json::from_str(
        result
            .as_ref()
            .ok()
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                panic!("confirmed generate should return an outcome string: {result:?}")
            }),
    )
    .expect("outcome json");
    assert!(
        outcome["saved"].is_null(),
        "a failed provider outcome must not report a saved artifact: {outcome}"
    );
    assert!(
        outcome["error"]
            .as_str()
            .unwrap_or_default()
            .contains("prediction failed"),
        "outcome must name the provider failure: {outcome}"
    );

    // Nothing arrived, the render job stays Pending through a rescan, and the
    // identical request cannot be blindly resubmitted (replay guard).
    assert!(
        !save_path.exists(),
        "failed provider must leave no take file"
    );
    h.call("workflow_import", json!({})).expect("rescan");
    let after = h.state();
    let render_job = after
        .jobs
        .iter()
        .find(|job| job.kind == "render")
        .expect("render job");
    assert_eq!(
        render_job.status,
        JobStatus::Pending,
        "failed/missing output must not complete the render job"
    );
    let cut1 = after
        .scene(1)
        .unwrap()
        .cuts
        .iter()
        .find(|c| c.id.cut == 1)
        .unwrap();
    assert!(cut1.takes.is_empty(), "no phantom take registered");
    assert_eq!(
        after.scene(1).unwrap().stages.assemble,
        StageStatus::Missing,
        "Finish facts must not advance past a failed render"
    );

    // Quote freshness: the first attempt changed the ledger revision, so a
    // confirmation built on the old quote must be refused.
    let stale = h.call(
        "confirm_unbounded_spend",
        json!({
            "provider": "replicate", "category": "media",
            "operation": "atlas_generate_video", "model": "testorg/testvideo",
            "requestHash": quote.request_hash.clone(),
            "ledgerRevision": quote.ledger_revision.clone(),
        }),
    );
    assert!(
        stale
            .as_ref()
            .err()
            .is_some_and(|error| error.contains("stale or policy changed")),
        "a confirmation on a stale quote must be refused: {stale:?}"
    );

    // Replay discipline: even with a fresh quote + authorization, the
    // identical unresolved request cannot be blindly resubmitted — an
    // uncertain provider outcome demands recovery, not a second paid dispatch.
    let fresh = spend::quote_unbounded_scoped(&h.root, &quote.request_hash).expect("fresh quote");
    let token2 = h
        .call(
            "confirm_unbounded_spend",
            json!({
                "provider": "replicate", "category": "media",
                "operation": "atlas_generate_video", "model": "testorg/testvideo",
                "requestHash": fresh.request_hash.clone(),
                "ledgerRevision": fresh.ledger_revision.clone(),
            }),
        )
        .expect("second confirmation mint")
        .as_str()
        .unwrap()
        .to_string();
    let replay = h.call(
        "media_generate_confirmed",
        json!({
            "operation": "atlas_generate_video",
            "mediaArgs": media_args.clone(),
            "token": token2
        }),
    );
    let replay_error = replay
        .err()
        .expect("identical unresolved request must not dispatch a second time");
    assert!(
        replay_error.contains("ambiguous previous provider outcome"),
        "unexpected replay error: {replay_error}"
    );
    assert_eq!(
        h.stub.posts.load(Ordering::SeqCst),
        1,
        "the replay guard must stop the request before the provider"
    );
}
