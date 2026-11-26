//! Interleaved calls through two independent handles to the same session DB.
use super::{Database, Row, accepted, begin, finish, load, lookup, persist, quint, status_number};
use hoimin_cli::session::SessionHandler;
use hoimin_core::SessionLoadOutcome;

const STORED_STATUS: u8 = 3; // Timeout remains available for a future rerun.

#[derive(Clone, Copy, Debug)]
enum Command {
    BeginAgain(u64),
    Load(u64, usize),
    Finish(u64, bool),
    Reopen(u64),
}

struct Harness {
    contender: Option<SessionHandler>,
    database: Database,
}

impl Harness {
    fn new() -> Self {
        let mut database = Database::new();
        database.handler().begin(begin(0, 0, 1)).unwrap();
        database
            .handler()
            .persist(&persist(0, STORED_STATUS))
            .unwrap();
        database.reopen();
        Self {
            contender: Some(database.open_handler()),
            database,
        }
    }

    fn handler(&mut self, actor: u64) -> &mut SessionHandler {
        match actor {
            0 => self.database.handler(),
            1 => self.contender.as_mut().unwrap(),
            _ => panic!("unknown model actor {actor}"),
        }
    }

    fn apply(&mut self, command: Command) -> String {
        let failure = match command {
            Command::BeginAgain(actor) => self.handler(actor).begin(begin(0, 0, 1)).unwrap_err(),
            Command::Load(actor, budget) => match self.handler(actor).load(&load(0, budget)) {
                Ok(loaded) => match loaded.outcome {
                    SessionLoadOutcome::Resumed(reference) => {
                        assert_eq!(reference.run_id, "run-0");
                        return "resumed".into();
                    }
                    SessionLoadOutcome::Fresh(reason) => return reason.code().into(),
                },
                Err(error) => error,
            },
            Command::Finish(actor, complete) => {
                match self.handler(actor).finish(finish(0, complete)) {
                    Ok(finished) => {
                        assert_eq!(finished.run_id, "run-0");
                        assert_eq!(finished.complete, complete);
                        return "finished".into();
                    }
                    Err(error) => error,
                }
            }
            Command::Reopen(actor) => {
                match actor {
                    0 => self.database.reopen(),
                    1 => {
                        drop(self.contender.take());
                        self.contender = Some(self.database.open_handler());
                    }
                    _ => panic!("unknown model actor {actor}"),
                }
                return "reopened".into();
            }
        };
        let code = failure.failure.code().to_owned();
        if code == "session.begin" {
            // Distinguish the duplicate row from an infrastructure error.
            assert!(!accepted::<()>(Err(failure), &["session.begin"]));
        }
        code
    }

    fn check_state(&mut self, state: &serde_json::Value) {
        assert_eq!(
            self.database.rows(),
            [Row {
                fp: 0,
                budget: usize::try_from(quint::number(&state["budget"])).unwrap(),
                complete: state["complete"].as_bool().unwrap(),
            }],
        );
        assert_eq!(self.database.results(), [STORED_STATUS, 0]);
        let owners = quint::set(&state["owners"]);
        for actor in [0, 1] {
            let loaded = self.handler(actor).lookup(&lookup(0));
            if owners.iter().any(|value| quint::number(value) == actor) {
                assert_eq!(
                    status_number(loaded.unwrap().result.unwrap().status),
                    STORED_STATUS
                );
            } else {
                assert_eq!(loaded.unwrap_err().failure.code(), "session.lookup.owner");
            }
        }
    }
}

fn replay(history: &[serde_json::Value]) {
    let mut actual = Harness::new();
    for row in history {
        let dispatch = &row["dispatch"];
        let args = &dispatch["value"];
        let actor = || quint::number(&args["actor"]);
        let command = match dispatch["tag"].as_str().unwrap() {
            "Init" => None,
            "BeginAgain" => Some(Command::BeginAgain(quint::number(args))),
            "Load" => Some(Command::Load(
                actor(),
                usize::try_from(quint::number(&args["limit"])).unwrap(),
            )),
            "Finish" => Some(Command::Finish(
                actor(),
                args["complete"].as_bool().unwrap(),
            )),
            "Reopen" => Some(Command::Reopen(quint::number(args))),
            other => panic!("unknown model command {other}"),
        };
        let state = &row["s"];
        if let Some(command) = command {
            let before = actual.database.snapshot();
            let outcome = actual.apply(command);
            assert_eq!(outcome, state["result"].as_str().unwrap(), "{command:?}");
            if !matches!(outcome.as_str(), "resumed" | "finished") {
                assert_eq!(
                    actual.database.snapshot(),
                    before,
                    "{command:?} changed the DB"
                );
            }
        }
        actual.check_state(state);
    }
}

#[rstest::rstest]
#[case::random_0(include_str!("../../../../formal/quint/traces/session_ownership_0.itf.json"), false)]
#[case::random_1(include_str!("../../../../formal/quint/traces/session_ownership_1.itf.json"), false)]
#[case::random_2(include_str!("../../../../formal/quint/traces/session_ownership_2.itf.json"), false)]
#[case::random_3(include_str!("../../../../formal/quint/traces/session_ownership_3.itf.json"), false)]
#[case::handoff(include_str!("../../../../formal/quint/traces/session_ownership_handoffStep_0.itf.json"), true)]
fn ownership_history_matches_two_sqlite_handlers(#[case] trace: &str, #[case] completes: bool) {
    let history = quint::states(trace);
    assert!(history.len() > 1);
    replay(&history);
    if completes {
        let state = &history.last().unwrap()["s"];
        assert_eq!(state["complete"], true);
        assert_eq!(state["result"], "matching_run_complete");
    }
}
