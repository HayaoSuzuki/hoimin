use hoimin_core::MutationStatus;

const CASES: [(MutationStatus, &str, &str); 7] = [
    (MutationStatus::Killed, "killed", r#""killed""#),
    (MutationStatus::Survived, "survived", r#""survived""#),
    (MutationStatus::Timeout, "timeout", r#""timeout""#),
    (
        MutationStatus::OutOfMemory,
        "out_of_memory",
        r#""out_of_memory""#,
    ),
    (
        MutationStatus::ProcessLimit,
        "process_limit",
        r#""process_limit""#,
    ),
    (MutationStatus::Error, "error", r#""error""#),
    (MutationStatus::NotRun, "not_run", r#""not_run""#),
];

#[test]
fn canonical_names_encode_and_decode_every_mutation_status() {
    for (status, name, _) in CASES {
        assert_eq!(status.as_str(), name);
        assert_eq!(MutationStatus::from_name(name), Some(status));
    }
}

#[test]
fn serde_round_trips_preserve_every_canonical_mutation_status_name() {
    for (status, _, json) in CASES {
        let encoded = serde_json::to_string(&status).unwrap();
        assert_eq!(encoded, json);
        assert_eq!(
            serde_json::from_str::<MutationStatus>(json).unwrap(),
            status
        );
        assert_eq!(
            serde_json::from_str::<MutationStatus>(&encoded).unwrap(),
            status
        );
    }
}

#[test]
fn canonical_name_and_serde_decoders_reject_noncanonical_names() {
    for name in [
        "",
        "unknown",
        "Killed",
        "SURVIVED",
        "Timeout",
        "OutOfMemory",
        "ProcessLimit",
        "Error",
        "NotRun",
        "out-of-memory",
        "process-limit",
        "not-run",
        " killed",
        "killed ",
        "killed\n",
        "killed\0",
        "ｋｉｌｌｅｄ",
    ] {
        assert_eq!(MutationStatus::from_name(name), None, "{name:?}");
        let json = serde_json::to_string(name).unwrap();
        assert!(
            serde_json::from_str::<MutationStatus>(&json).is_err(),
            "{name:?}"
        );
    }
}
