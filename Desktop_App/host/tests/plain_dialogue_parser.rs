use workbench_host::script::extract_scenes;

#[test]
fn parses_plain_sections_without_crossing_boundaries() {
    let input = "Scene 1 — Ward\nLocation: Hospital\nAction: Doors open.\nDialogue:\nMara: First line\ncontinues here.\nBob: Second line.\nSound: Alarm sounds.\nAction: Nobody speaks.\nDialogue:\nMara: Last line.\nScene 2 — Hall\nDialogue:\nBob: Next scene.\n";
    let scenes = extract_scenes(input);
    assert_eq!(scenes.len(), 2);
    assert_eq!(scenes[0].dialogue.iter().map(|d| (d.speaker.as_str(),d.text.as_str())).collect::<Vec<_>>(), vec![("Mara","First line continues here."),("Bob","Second line."),("Mara","Last line.")]);
    assert_eq!(scenes[0].location.as_deref(), Some("Hospital"));
    assert_eq!(scenes[0].sound, vec!["Alarm sounds."]);
    assert_eq!(scenes[1].dialogue[0].text, "Next scene.");
}
