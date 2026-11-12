//! Local-only regression for derived padding destination at the existing execution seam.
use std::{fs,process::Command};
use workbench_host::{fs_ops::pad_audio_handles,session,workflow::qc::{check_audio_decode,QCSeverity}};
#[test]
fn padding_cannot_overwrite_or_escape_the_shot_bucket(){
 let dir=tempfile::tempdir().unwrap();let root=dir.path();session::open(root.to_str().unwrap()).unwrap();
 let bucket=root.join("Scenes/Scene1/Cut1");fs::create_dir_all(&bucket).unwrap();
 let input=bucket.join("performance.wav");let output=bucket.join("performance_pad_h0.3_t1.0.wav");
 assert!(Command::new("ffmpeg").args(["-v","error","-f","lavfi","-i","sine=frequency=440:duration=1","-c:a","pcm_s16le"]).arg(&input).status().unwrap().success());
 let original=fs::read(&input).unwrap();let outside=tempfile::tempdir().unwrap();let target=outside.path().join("protected.wav");fs::write(&target,&original).unwrap();
 #[cfg(unix)]{
  std::os::unix::fs::symlink(&target,&output).unwrap();
  assert!(pad_audio_handles(input.to_str().unwrap(),output.to_str().unwrap(),300,1.0).is_err(),"reject padded output symlink, no outside overwrite");
  assert_eq!(fs::read(&target).unwrap(),original);fs::remove_file(&output).unwrap();
  std::os::unix::fs::symlink(outside.path().join("missing.wav"),&output).unwrap();
  assert!(pad_audio_handles(input.to_str().unwrap(),output.to_str().unwrap(),300,1.0).is_err());assert!(!outside.path().join("missing.wav").exists());fs::remove_file(&output).unwrap();
 }
 fs::write(&output,&original).unwrap();assert!(pad_audio_handles(input.to_str().unwrap(),output.to_str().unwrap(),300,1.0).is_err());assert_eq!(fs::read(&output).unwrap(),original);fs::remove_file(&output).unwrap();
 let other=root.join("Scenes/Scene2/Cut1");fs::create_dir_all(&other).unwrap();
 assert!(pad_audio_handles(input.to_str().unwrap(),other.join("wrong.wav").to_str().unwrap(),300,1.0).is_err());assert!(!other.join("wrong.wav").exists());
 pad_audio_handles(input.to_str().unwrap(),output.to_str().unwrap(),300,1.0).unwrap();assert!(check_audio_decode(&output).iter().all(|f|f.severity!=QCSeverity::Error));assert_eq!(fs::read(&input).unwrap(),original);
 println!("SB12 padding: external/dangling symlink, occupied and wrong-shot destination refused; original/outside unchanged; fresh exclusive padded WAV strictly decodes. No provider dispatch.");session::close();
}
