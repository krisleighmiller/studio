use serde_json::Value;

#[test]
fn provider_registry_explicitly_reports_operation_and_input_metadata() {
    let registry: Value = serde_json::from_str(include_str!("../src/workflow/providers.json"))
        .expect("provider registry parses");
    let entries = registry["prices"].as_array().expect("prices array");
    assert!(!entries.is_empty());
    for entry in entries {
        let capability = entry["capability"].as_str().expect("capability");
        let support = entry["operation_support"]
            .as_str()
            .expect("operation support status");
        assert!(matches!(
            support,
            "supported" | "unsupported" | "unverified"
        ));
        let input_limits = entry["input_limits"]
            .as_object()
            .expect("input limits status");
        assert!(matches!(
            input_limits["status"].as_str(),
            Some("known" | "unknown" | "not_applicable")
        ));
        if capability.starts_with("video.") {
            let buckets = entry["duration_buckets"]
                .as_object()
                .expect("video duration buckets status");
            assert!(matches!(
                buckets["status"].as_str(),
                Some("known" | "unknown")
            ));
        }
        if entry["pricing_status"] != "known" {
            assert!(
                entry["pricing_status"] == "unknown" || entry["pricing_status"] == "placeholder"
            );
        }
    }
}
