/// A state-bearing value whose internal consistency can be checked in CI builds.
pub trait ContractInvariant {
    fn invariant(&self) -> bool;
}

#[cfg(feature = "contracts")]
#[macro_export]
macro_rules! contract_require {
    ($id:expr, $condition:expr, $context:expr $(,)?) => {{
        if !$condition {
            panic!(
                "contract violation [require:{}]: condition `{}`; context: {:?}",
                $id,
                stringify!($condition),
                $context
            );
        }
    }};
}

#[cfg(not(feature = "contracts"))]
#[macro_export]
macro_rules! contract_require {
    ($id:expr, $condition:expr, $context:expr $(,)?) => {
        ()
    };
}

#[cfg(feature = "contracts")]
#[macro_export]
macro_rules! contract_ensure {
    ($id:expr, $condition:expr, $context:expr $(,)?) => {{
        if !$condition {
            panic!(
                "contract violation [ensure:{}]: condition `{}`; context: {:?}",
                $id,
                stringify!($condition),
                $context
            );
        }
    }};
}

#[cfg(not(feature = "contracts"))]
#[macro_export]
macro_rules! contract_ensure {
    ($id:expr, $condition:expr, $context:expr $(,)?) => {
        ()
    };
}
