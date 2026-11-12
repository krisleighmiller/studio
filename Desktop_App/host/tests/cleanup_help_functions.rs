use workbench_host::script;

#[test]
fn hp01_bundled_guide_example_is_parsed_by_the_real_script_parser() {
    let guide = include_str!("../../src/docs/workbench-guide.md");
    let example = guide.split_once("```text\n").expect("documented script example").1
        .split_once("\n```").expect("closed script example").0;
    let scenes = script::extract_scenes(example);
    assert_eq!(scenes.len(), 2);
    assert_eq!(scenes.iter().map(|scene| scene.id).collect::<Vec<_>>(), vec![1, 2]);
    assert!(scenes[0].title.contains("Hello"));
    assert_eq!(scenes[0].location.as_deref(), Some("A quiet doorway"));
    assert_eq!(scenes[0].dialogue.len(), 2);
    assert_eq!(scenes[0].dialogue[0].speaker, "Alice");
    assert_eq!(scenes[0].dialogue[0].text, "Hello, Bob!");
    assert_eq!(scenes[1].dialogue[0].speaker, "Bob");
    assert_eq!(scenes[1].dialogue[0].text, "See you soon!");
    // Contrast: an ordinary title/slugline cannot recover scenes on its own.
    assert!(script::extract_scenes("# Two friends\nINT. DOORWAY - DAY\nAlice: Hello!").is_empty());
    // This invokes only the pure parser: no host/provider dispatch is available.
}
