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

#[rstest::rstest]
#[case::mutationstatus_killed_1(0)]
#[case::mutationstatus_survived_2(1)]
#[case::mutationstatus_timeout_3(2)]
#[case::mutationstatus_outofmemory_4(3)]
#[case::mutationstatus_processlimit_5(4)]
#[case::mutationstatus_error_6(5)]
#[case::mutationstatus_notrun_7(6)]
fn canonical_names_encode_and_decode_every_mutation_status(#[case] case_index: usize) {
    let (status, name, _) = CASES[case_index];

    assert_eq!(status.as_str(), name);
    assert_eq!(MutationStatus::from_name(name), Some(status));
}

#[rstest::rstest]
#[case::mutationstatus_killed_1(0)]
#[case::mutationstatus_survived_2(1)]
#[case::mutationstatus_timeout_3(2)]
#[case::mutationstatus_outofmemory_4(3)]
#[case::mutationstatus_processlimit_5(4)]
#[case::mutationstatus_error_6(5)]
#[case::mutationstatus_notrun_7(6)]
fn serde_round_trips_preserve_every_canonical_mutation_status_name(#[case] case_index: usize) {
    let (status, _, json) = CASES[case_index];

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

#[rstest::rstest]
#[case::value("")]
#[case::unknown("unknown")]
#[case::killed("Killed")]
#[case::survived("SURVIVED")]
#[case::timeout("Timeout")]
#[case::outofmemory("OutOfMemory")]
#[case::processlimit("ProcessLimit")]
#[case::error("Error")]
#[case::notrun("NotRun")]
#[case::out_of_memory("out-of-memory")]
#[case::process_limit("process-limit")]
#[case::not_run("not-run")]
#[case::killed_13(" killed")]
#[case::killed_14("killed ")]
#[case::killed_15("killed\n")]
#[case::killed_16("killed\0")]
#[case::value_17("ｋｉｌｌｅｄ")]
fn canonical_name_and_serde_decoders_reject_noncanonical_names(#[case] name: &str) {
    assert_eq!(MutationStatus::from_name(name), None, "{name:?}");
    let json = serde_json::to_string(name).unwrap();
    assert!(
        serde_json::from_str::<MutationStatus>(&json).is_err(),
        "{name:?}"
    );
}
