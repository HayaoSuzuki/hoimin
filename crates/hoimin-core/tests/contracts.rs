use std::cell::Cell;

use hoimin_core::{ContractInvariant, contract_ensure, contract_require};

#[derive(Debug)]
struct Invalid;

#[cfg(feature = "contracts")]
#[test]
fn enabled_ensure_evaluates_condition_without_evaluating_success_context() {
    let evaluations = Cell::new(0);
    contract_ensure!(
        "test.enabled",
        {
            evaluations.set(evaluations.get() + 1);
            true
        },
        {
            evaluations.set(evaluations.get() + 1);
            7
        }
    );
    assert_eq!(evaluations.get(), 1);
}

#[cfg(not(feature = "contracts"))]
#[test]
fn disabled_ensure_does_not_evaluate_condition_or_context() {
    let evaluations = Cell::new(0);
    contract_ensure!(
        "test.disabled",
        {
            evaluations.set(evaluations.get() + 1);
            false
        },
        {
            evaluations.set(evaluations.get() + 1);
            7
        }
    );
    assert_eq!(evaluations.get(), 0);
}

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
