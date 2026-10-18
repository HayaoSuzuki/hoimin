use hoimin_core::{CandidateIdentityRef, MutationCandidate};
use serde_json::{Value, json};

/// Assign a genuine ID after a fixture's identity fields have been arranged.
pub fn assign_id(candidate: &mut Value) {
    let typed: MutationCandidate = serde_json::from_value(candidate.clone()).unwrap();
    candidate["id"] = json!(
        CandidateIdentityRef::from(&typed)
            .stable_mutant_id()
            .as_str()
    );
}
