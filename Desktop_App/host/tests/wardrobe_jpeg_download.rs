#![cfg(feature = "mock-provider")]
use serde_json::json;
use std::{fs, io::{Read, Write}, net::TcpListener, process::Command};
use workbench_host::{atlas, workflow::{character_wardrobe, import, state::{JobRecord, JobStatus, ProjectState}}};

#[test]
fn downloaded_jpeg_settles_wardrobe_on_callback_and_reimport() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir_all(root.join("Characters/Hero")).unwrap();
    fs::write(root.join("Characters/Hero/character.yaml"), "name: Hero\n").unwrap();
    let fixture = dir.path().join("fixture.jpg");
    assert!(Command::new("ffmpeg").args(["-v", "error", "-f", "lavfi", "-i", "color=blue:s=32x48", "-frames:v", "1", "-threads", "1"]).arg(&fixture).status().unwrap().success());
    let jpeg = fs::read(&fixture).unwrap();
    let mut state = ProjectState::from_json(&json!({"version":2,"characters":[{"id":"hero","name":"Hero","dir":"Characters/Hero"}]}).to_string()).unwrap();
    for i in 1..=2 {
        let mut job = JobRecord::open(format!("job_{i}"), "character_wardrobe");
        job.output_path = Some(format!("Preproduction/CharacterWardrobe/wardrobe_{i:06}.png"));
        job.raw_provider_id = Some("hero".into());
        state.jobs.push(job);
    }
    import::persist_state(&root, &state).unwrap();
    let app = workbench_host::ctx::HostCtx::new(dir.path().join("settings"));
    let recorder = workbench_host::voice::RecorderState::new();
    workbench_host::dispatch(&app, &recorder, "open_project", &json!({"path": root})).unwrap();
    for i in 1..=2 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let body = jpeg.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        });
        let requested = format!("Preproduction/CharacterWardrobe/wardrobe_{i:06}.png");
        let response = atlas::download_atlas_media(&format!("http://{addr}/opaque"), &requested, root.to_str(), None).unwrap();
        server.join().unwrap();
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        let actual = root.join(requested.replace(".png", ".jpg"));
        assert_eq!(response["path"].as_str().unwrap(), actual.to_str().unwrap());
        assert_eq!(fs::read(&actual).unwrap(), jpeg);
        if i == 1 { let _ = character_wardrobe::on_saved(root.to_str(), &requested); }
    }
    let reopened = import::load_state(&root).unwrap();
    for i in 1..=2 {
        let job = reopened.jobs.iter().find(|j| j.id == format!("job_{i}")).unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let asset = reopened.asset(&job.output_asset_ids[0]).unwrap();
        assert_eq!(asset.path, format!("Preproduction/CharacterWardrobe/wardrobe_{i:06}.jpg"));
        assert_eq!(asset.character_id.as_deref(), Some("hero"));
    }
    assert_eq!(import::load_state(&root).unwrap().assets.len(), 2);
    // A second download resolving to an existing .jpg must not overwrite it.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let body = jpeg.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let _ = stream.read(&mut request).unwrap();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        stream.write_all(&body).unwrap();
    });
    let error = atlas::download_atlas_media(&format!("http://{addr}/opaque"), "Preproduction/CharacterWardrobe/wardrobe_000001.png", root.to_str(), None).unwrap_err();
    server.join().unwrap();
    assert!(error.contains("without overwriting"));
    assert_eq!(fs::read(root.join("Preproduction/CharacterWardrobe/wardrobe_000001.jpg")).unwrap(), jpeg);
}
