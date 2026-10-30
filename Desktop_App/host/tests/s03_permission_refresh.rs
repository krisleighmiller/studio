//! A no-op renderer refresh must not erase an already granted exact operation.
use std::sync::atomic::{AtomicUsize,Ordering};
use workbench_host::workflow::{import,spend,state::{ProjectState,SpendRequestMetadata}};
#[test]
fn exact_permission_survives_noop_import_and_remains_one_use() {
 let t=tempfile::tempdir().unwrap();let root=t.path();import::save_project(root,ProjectState::new()).unwrap();
 import::import_project(root,import::ImportOptions{persist:true}).unwrap();
 let hash=spend::fingerprint(b"s03 exact local request").unwrap();let q=spend::quote_unbounded_scoped(root,&hash).unwrap();let token=spend::confirm_unbounded_scoped(root,"local-fixture","media","atlas_generate_image","s03-image",&hash,&q.ledger_revision).unwrap();
 import::import_project(root,import::ImportOptions{persist:true}).unwrap();
 let metadata=SpendRequestMetadata{request_hash:hash,disclosure:None,duration_seconds:None,width:None,height:None,prediction_id:None};let calls=AtomicUsize::new(0);
 let invoke=|m:SpendRequestMetadata|spend::run_scoped_with_request(root,"local-fixture","media","atlas_generate_image","image.text_to_image","s03-image",Some(&token),None,Some(m),None,||{calls.fetch_add(1,Ordering::SeqCst);Ok("local-only-dispatch".into())});
 let mut different=metadata.clone();different.request_hash=spend::fingerprint(b"different request").unwrap();assert!(invoke(different).is_err());assert_eq!(calls.load(Ordering::SeqCst),0);
 assert_eq!(invoke(metadata.clone()).expect("no-op refresh must retain exact permission"),"local-only-dispatch");assert_eq!(calls.load(Ordering::SeqCst),1);
 import::import_project(root,import::ImportOptions{persist:true}).unwrap();assert!(invoke(metadata).is_err());assert_eq!(calls.load(Ordering::SeqCst),1,"consumed permission must never revive");
}
