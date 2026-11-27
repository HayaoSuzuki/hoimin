namespace HoiminOracle.SessionWalRetry

inductive Fault where
  | busy | busyRecovery | locked | lockedSharedCache | io | invalidQuery
  deriving DecidableEq, BEq, Repr

def Fault.isContention : Fault → Bool
  | .busy | .busyRecovery | .locked | .lockedSharedCache => true
  | .io | .invalidQuery => false

inductive Reply where
  | mode (name : String)
  | fault (reason : Fault)
  deriving DecidableEq

inductive Outcome where
  | enabled
  | refused (mode : String)
  | failed (reason : Fault)
  deriving DecidableEq, Repr

inductive Decision where
  | finish (outcome : Outcome)
  | wait (milliseconds : Nat)
  deriving DecidableEq, Repr

def delayLimit : Nat := 5

def classify (deadline observed sleepAt : Nat) : Reply → Decision
  | .mode name => if name.toLower = "wal" ∨ name.toLower = "memory" then
      .finish .enabled else .finish (.refused name)
  | .fault error => if error.isContention = true ∧ observed < deadline then
      .wait (min delayLimit (deadline - sleepAt)) else .finish (.failed error)

theorem no_retry_on_or_after_deadline (deadline observed sleepAt : Nat) (error : Fault)
    (expired : deadline ≤ observed) :
    classify deadline observed sleepAt (.fault error) = .finish (.failed error) := by
  simp [classify, Nat.not_lt.mpr expired]

theorem permanent_failure_is_terminal (deadline observed sleepAt : Nat) (error : Fault)
    (permanent : error.isContention = false) :
    classify deadline observed sleepAt (.fault error) = .finish (.failed error) := by
  simp [classify, permanent]

theorem waiting_requires_unexpired_contention (deadline observed sleepAt delay : Nat) (error : Fault)
    (retry : classify deadline observed sleepAt (.fault error) = .wait delay) :
    error.isContention = true ∧ observed < deadline ∧
      delay ≤ delayLimit ∧ delay ≤ deadline - sleepAt := by
  by_cases eligible : error.isContention = true ∧ observed < deadline
  · have waits : min delayLimit (deadline - sleepAt) = delay := by
      simpa [classify, eligible] using retry
    subst delay
    exact ⟨eligible.1, eligible.2, Nat.min_le_left _ _, Nat.min_le_right _ _⟩
  · simp [classify, eligible] at retry

theorem zero_wait_if_clock_crosses_deadline (deadline observed sleepAt : Nat) (error : Fault)
    (contention : error.isContention = true) (before : observed < deadline)
    (reached : deadline ≤ sleepAt) :
    classify deadline observed sleepAt (.fault error) = .wait 0 := by
  simp [classify, contention, before, Nat.sub_eq_zero_of_le reached]

structure Attempt where
  reply : Reply
  queryTime : Nat := 0
  clockAdvance : Nat := 0
  oversleep : Nat := 0

structure Observation where
  outcome : Option Outcome
  queryStarts : List Nat
  sleeps : List Nat
  deriving DecidableEq

def runWith (decideReply : Nat → Nat → Nat → Reply → Decision) (deadline now : Nat) :
    List Attempt → Observation
  | [] => ⟨none, [], []⟩
  | attempt :: rest =>
    let observed := now + attempt.queryTime
    let sleepAt := observed + attempt.clockAdvance
    match decideReply deadline observed sleepAt attempt.reply with
    | .finish outcome => ⟨some outcome, [now], []⟩
    | .wait delay =>
      let after := runWith decideReply deadline (sleepAt + delay + attempt.oversleep) rest
      ⟨after.outcome, now :: after.queryStarts, delay :: after.sleeps⟩

def run := runWith classify

theorem first_probe_runs_even_with_zero_timeout (start : Nat) (attempt : Attempt)
    (rest : List Attempt) :
    (run 0 start (attempt :: rest)).queryStarts.head? = some start := by
  simp only [run, runWith]
  split <;> rfl

theorem finished_sqlite_reply_has_no_further_attempts (deadline start : Nat) (attempt : Attempt)
    (rest : List Attempt) (outcome : Outcome)
    (finished : classify deadline (start + attempt.queryTime)
      (start + attempt.queryTime + attempt.clockAdvance) attempt.reply = .finish outcome) :
    run deadline start (attempt :: rest) = ⟨some outcome, [start], []⟩ := by
  simp [run, runWith, finished]

end HoiminOracle.SessionWalRetry
