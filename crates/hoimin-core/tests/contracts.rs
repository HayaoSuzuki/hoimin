#[cfg(not(feature = "contracts"))]
use std::cell::Cell;

use hoimin_core::{ContractInvariant, contract_ensure, contract_require};

#[derive(Debug)]
struct Invalid;

impl ContractInvariant for Invalid {
    fn invariant(&self) -> bool {
        false
    }
}

#[cfg(not(feature = "contracts"))]
#[test]
fn disabled_contract_does_not_evaluate_condition_or_context() {
    let evaluated = Cell::new(false);
    contract_require!(
        "test.disabled",
        {
            evaluated.set(true);
            false
        },
        {
            evaluated.set(true);
            1
        }
    );
    assert!(!evaluated.get());
}

#[cfg(not(feature = "contracts"))]
#[test]
fn disabled_ensure_does_not_call_invariant() {
    let value = Invalid;
    std::hint::black_box(&value);
    contract_ensure!("test.disabled.invariant", value.invariant(), &value);
}

#[cfg(feature = "contracts")]
#[test]
#[should_panic(expected = "contract violation [require:test.enabled]")]
fn enabled_contract_panics_with_stable_id() {
    contract_require!("test.enabled", false, 7_u8);
}

#[cfg(feature = "contracts")]
#[test]
#[should_panic(expected = "contract violation [ensure:test.enabled.invariant]")]
fn enabled_ensure_calls_invariant() {
    let value = Invalid;
    contract_ensure!("test.enabled.invariant", value.invariant(), &value);
}
