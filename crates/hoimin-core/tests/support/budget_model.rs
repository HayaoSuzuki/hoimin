//! Independent journal model shared by property tests and the fuzz target.
//! Entries retain released reservations, unlike the production active-ID map.
use hoimin_core::{
    BudgetError, BudgetKind, BudgetLedger, LimitReached, Reservation, ReservationId, ReserveError,
    RunBudgets,
};

#[derive(Clone, Debug)]
pub enum Command {
    Reserve(u8, u64),
    Release(u64),
}

pub const KINDS: [BudgetKind; 3] = [BudgetKind::Memory, BudgetKind::Copy, BudgetKind::Processes];

/// Replay a single history, checking every observable state after every command.
/// u128 reference sums prevent the oracle from sharing u64 overflow mistakes.
pub fn check_history(limits: [u64; 3], commands: &[Command]) -> [u64; 3] {
    let mut actual = BudgetLedger::new(RunBudgets {
        memory: limits[0],
        copy: limits[1],
        processes: limits[2],
    });
    let mut journal: Vec<(Reservation, bool)> = Vec::new();
    for command in commands {
        match *command {
            Command::Reserve(index, amount) => {
                let index = usize::from(index) % KINDS.len();
                let reserved: u128 = journal
                    .iter()
                    .filter(|(row, live)| *live && row.kind == KINDS[index])
                    .map(|(row, _)| u128::from(row.amount))
                    .sum();
                let available = limits[index] - u64::try_from(reserved).unwrap();
                let expected = if amount <= available {
                    let id = ReservationId(u64::try_from(journal.len()).unwrap());
                    journal.push((
                        Reservation {
                            kind: KINDS[index],
                            amount,
                        },
                        true,
                    ));
                    Ok(id)
                } else {
                    Err(ReserveError::LimitReached(LimitReached {
                        kind: KINDS[index],
                        requested: amount,
                        available,
                    }))
                };
                assert_eq!(
                    actual.reserve(KINDS[index], amount),
                    expected,
                    "{command:?}"
                );
            }
            Command::Release(id) => {
                let expected = match usize::try_from(id).ok().and_then(|i| journal.get_mut(i)) {
                    Some((_, live)) if *live => {
                        *live = false;
                        Ok(())
                    }
                    Some(_) => Err(BudgetError::AlreadyReleased(ReservationId(id))),
                    None => Err(BudgetError::UnknownReservation(ReservationId(id))),
                };
                assert_eq!(actual.release(ReservationId(id)), expected, "{command:?}");
            }
        }
        for (i, (row, live)) in journal.iter().enumerate() {
            assert_eq!(
                actual.reservation(ReservationId(u64::try_from(i).unwrap())),
                live.then_some(row)
            );
        }
        for k in KINDS {
            let expected: u128 = journal
                .iter()
                .filter(|(r, live)| *live && r.kind == k)
                .map(|(r, _)| u128::from(r.amount))
                .sum();
            assert_eq!(u128::from(actual.reserved(k)), expected);
        }
        assert_eq!(
            actual.limits(),
            RunBudgets {
                memory: limits[0],
                copy: limits[1],
                processes: limits[2]
            }
        );
    }
    KINDS.map(|k| actual.reserved(k))
}

/// Bound each libFuzzer input to 256 operations; retain full-width amounts/IDs.
pub fn decode(data: &[u8]) -> Vec<Command> {
    data.as_chunks::<10>()
        .0
        .iter()
        .take(256)
        .map(|chunk| {
            let value = u64::from_le_bytes(chunk[2..10].try_into().unwrap());
            match chunk[0] % 2 {
                0 => Command::Reserve(chunk[1], value),
                _ => Command::Release(value),
            }
        })
        .collect()
}
