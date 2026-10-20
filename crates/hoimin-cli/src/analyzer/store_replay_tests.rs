use super::*;
use hoimin_core::ByteSpan;

fn candidate(sequence: u64) -> MutationCandidate {
    MutationCandidate {
        id: format!("candidate-{sequence}"),
        sequence,
        path: "src/calc.py".into(),
        span: ByteSpan {
            start: sequence,
            length: 1,
        },
        original: "+".repeat(4096),
        replacement: "-".repeat(4096),
        operator: "binary_add_sub".into(),
        line: 1,
        column: 0,
        symbol: None,
        file_hash: "0".repeat(64),
    }
}

fn spool(candidates: &[MutationCandidate]) -> (tempfile::TempDir, CandidateSpoolRef) {
    let directory = tempfile::tempdir().unwrap();
    let mut store = CandidateStore::new_in(candidates.len() as u64, directory.path()).unwrap();
    for candidate in candidates {
        store.push(candidate).unwrap();
    }
    (directory, store.finish().unwrap())
}

fn scan(replay: &mut CandidateReplay, reference: &CandidateSpoolRef) {
    while replay
        .read(reference, &CandidateReadTarget::Next)
        .unwrap()
        .is_some()
    {}
}

#[test]
fn reverse_selected_reads_visit_one_scan_plus_one_record_per_fetch() {
    // Arrange.
    let candidates: Vec<_> = (1..=32).map(candidate).collect();
    let (_directory, reference) = spool(&candidates);
    let selected = [
        "candidate-32",
        "candidate-24",
        "candidate-16",
        "candidate-8",
    ];
    let mut replay = CandidateReplay::new(selected.map(str::to_owned));
    RECORD_READS.set(0);

    // Act.
    scan(&mut replay, &reference);
    let actual: Vec<_> = selected
        .map(|id| {
            replay
                .read(&reference, &CandidateReadTarget::Id(id.into()))
                .unwrap()
                .unwrap()
                .sequence
        })
        .into();

    // Assert: the bounded reader counts physical record reads, including indexed reads.
    assert_eq!(actual, [32, 24, 16, 8]);
    assert_eq!(RECORD_READS.get(), 36);
}

#[test]
fn replay_indexes_only_requested_ids_including_unfound_slots() {
    // Arrange.
    let candidates: Vec<_> = (1..=32).map(candidate).collect();
    let (_directory, reference) = spool(&candidates);
    let mut replay = CandidateReplay::new(
        ["candidate-3", "candidate-31", "absent", "candidate-3"].map(str::to_owned),
    );

    // Act.
    scan(&mut replay, &reference);

    // Assert: discovery cannot add unselected keys or retain candidate bodies.
    assert_eq!(replay.selected.len(), 3);
    assert_eq!(
        replay
            .selected
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["absent", "candidate-3", "candidate-31"]
    );
    assert_eq!(replay.selected["absent"], None);
    assert_eq!(
        replay.selected["candidate-31"].unwrap().expected_sequence,
        31
    );
}

#[test]
fn selected_duplicate_id_uses_last_discovered_body() {
    // Arrange.
    let first = candidate(1);
    let mut last = candidate(3);
    last.id.clone_from(&first.id);
    last.replacement = "last".into();
    let (_directory, reference) = spool(&[first, candidate(2), last]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);

    // Act.
    scan(&mut replay, &reference);
    let selected = replay
        .read(&reference, &CandidateReadTarget::Id("candidate-1".into()))
        .unwrap()
        .unwrap();

    // Assert.
    assert_eq!(selected.sequence, 3);
    assert_eq!(selected.replacement, "last");
}

#[test]
fn indexed_read_leaves_the_sequential_position_unchanged() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1), candidate(2), candidate(3)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    replay.read(&reference, &CandidateReadTarget::Next).unwrap();

    // Act.
    let indexed = replay
        .read(&reference, &CandidateReadTarget::Id("candidate-1".into()))
        .unwrap()
        .unwrap();
    let next = replay
        .read(&reference, &CandidateReadTarget::Next)
        .unwrap()
        .unwrap();

    // Assert.
    assert_eq!(indexed.sequence, 1);
    assert_eq!(next.sequence, 2);
}

#[test]
fn retiring_a_sequential_read_rewinds_only_its_record() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1), candidate(2), candidate(3)]);
    let mut replay = CandidateReplay::new([]);
    replay.read(&reference, &CandidateReadTarget::Next).unwrap();
    replay.read(&reference, &CandidateReadTarget::Next).unwrap();

    // Act.
    replay.retire_read();
    let repeated = replay
        .read(&reference, &CandidateReadTarget::Next)
        .unwrap()
        .unwrap();

    // Assert.
    assert_eq!(repeated.sequence, 2);
}

#[test]
fn retiring_a_failed_read_does_not_rewind_an_accepted_record() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1), candidate(2)]);
    let mut replay = CandidateReplay::new([]);
    replay.read(&reference, &CandidateReadTarget::Next).unwrap();
    let original = std::fs::read(&reference.token).unwrap();
    let mut corrupt = original.clone();
    let tail = corrupt.iter().position(|byte| *byte == b'\n').unwrap() + 1;
    corrupt[tail] = b'!';
    std::fs::write(&reference.token, corrupt).unwrap();
    assert!(matches!(
        replay.read(&reference, &CandidateReadTarget::Next),
        Err(StoreError::CorruptRecord(_))
    ));

    // Act.
    replay.retire_read();
    std::fs::write(&reference.token, original).unwrap();
    let next = replay
        .read(&reference, &CandidateReadTarget::Next)
        .unwrap()
        .unwrap();

    // Assert.
    assert_eq!(next.sequence, 2);
}

#[test]
fn retiring_an_indexed_read_does_not_rewind_an_accepted_scan_record() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1), candidate(2)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    replay.read(&reference, &CandidateReadTarget::Next).unwrap();
    replay
        .read(&reference, &CandidateReadTarget::Id("candidate-1".into()))
        .unwrap();

    // Act.
    replay.retire_read();
    let next = replay
        .read(&reference, &CandidateReadTarget::Next)
        .unwrap()
        .unwrap();

    // Assert.
    assert_eq!(next.sequence, 2);
}

#[test]
fn retiring_eof_does_not_repeat_the_last_accepted_record() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1)]);
    let mut replay = CandidateReplay::new([]);
    scan(&mut replay, &reference);

    // Act.
    replay.retire_read();
    let next = replay.read(&reference, &CandidateReadTarget::Next).unwrap();

    // Assert.
    assert_eq!(next, None);
}

#[test]
fn sequential_scan_validates_tail_after_finding_every_requested_id() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1), candidate(2)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    replay.read(&reference, &CandidateReadTarget::Next).unwrap();
    let mut bytes = std::fs::read(&reference.token).unwrap();
    let tail = bytes.iter().position(|byte| *byte == b'\n').unwrap() + 1;
    bytes[tail] = b'!';
    std::fs::write(&reference.token, bytes).unwrap();

    // Act.
    let result = replay.read(&reference, &CandidateReadTarget::Next);

    // Assert.
    assert!(matches!(result, Err(StoreError::CorruptRecord(_))));
}

#[test]
fn indexed_read_rejects_a_selected_id_that_was_not_discovered() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1)]);
    let mut replay = CandidateReplay::new(["absent".into()]);
    scan(&mut replay, &reference);

    // Act.
    let result = replay.read(&reference, &CandidateReadTarget::Id("absent".into()));

    // Assert.
    assert!(matches!(result, Err(StoreError::MissingCandidate { id }) if id == "absent"));
}

#[test]
fn indexed_read_rejects_an_unselected_discovered_id() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1), candidate(2)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    scan(&mut replay, &reference);

    // Act.
    let result = replay.read(&reference, &CandidateReadTarget::Id("candidate-2".into()));

    // Assert.
    assert!(matches!(result, Err(StoreError::MissingCandidate { id }) if id == "candidate-2"));
}

#[test]
fn indexed_read_rejects_a_replaced_candidate_identity() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    scan(&mut replay, &reference);
    let mut replacement = candidate(1);
    replacement.id = "different-1".into();
    let mut encoded = serde_json::to_vec(&replacement).unwrap();
    encoded.push(b'\n');
    std::fs::write(&reference.token, encoded).unwrap();

    // Act.
    let result = replay.read(&reference, &CandidateReadTarget::Id("candidate-1".into()));

    // Assert.
    assert!(matches!(
        result,
        Err(StoreError::CandidateIdMismatch { expected, actual })
            if expected == "candidate-1" && actual == "different-1"
    ));
}

#[test]
fn indexed_read_rejects_a_corrupt_record() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    scan(&mut replay, &reference);
    std::fs::write(&reference.token, b"not json\n").unwrap();

    // Act.
    let result = replay.read(&reference, &CandidateReadTarget::Id("candidate-1".into()));

    // Assert.
    assert!(matches!(result, Err(StoreError::CorruptRecord(_))));
}

#[test]
fn indexed_read_rejects_a_truncated_record_as_an_error() {
    // Arrange.
    let (_directory, reference) = spool(&[candidate(1)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    scan(&mut replay, &reference);
    std::fs::write(&reference.token, []).unwrap();

    // Act.
    let result = replay.read(&reference, &CandidateReadTarget::Id("candidate-1".into()));

    // Assert.
    assert!(matches!(
        result,
        Err(StoreError::UnexpectedEof {
            expected_records: 1,
            actual_records: 0,
        })
    ));
}

#[test]
fn replay_rejects_switching_spool_files() {
    // Arrange.
    let (_first_directory, first) = spool(&[candidate(1)]);
    let (_second_directory, second) = spool(&[candidate(1)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    replay.read(&first, &CandidateReadTarget::Next).unwrap();

    // Act.
    let result = replay.read(&second, &CandidateReadTarget::Id("candidate-1".into()));

    // Assert.
    assert!(matches!(result, Err(StoreError::SpoolMismatch)));
}

#[test]
fn replay_rejects_changing_a_bound_spool_record_count() {
    // Arrange.
    let (_directory, mut reference) = spool(&[candidate(1)]);
    let mut replay = CandidateReplay::new(["candidate-1".into()]);
    replay.read(&reference, &CandidateReadTarget::Next).unwrap();
    reference.records = 2;

    // Act.
    let result = replay.read(&reference, &CandidateReadTarget::Next);

    // Assert.
    assert!(matches!(result, Err(StoreError::SpoolMismatch)));
}
