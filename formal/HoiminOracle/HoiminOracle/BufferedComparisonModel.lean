namespace HoiminOracle.BufferedComparison

inductive Side | left | right
  deriving DecidableEq

inductive Fault | interrupted | ioError
  deriving DecidableEq

/-- Each buffer made available by a successful pre-EOF read is nonempty. -/
inductive Stream
  | terminal (fault : Option Fault)
  | chunk (first : UInt8) (rest : List UInt8) (next : Stream)

def Stream.bytes : Stream → List UInt8
  | .terminal _ => []
  | .chunk first rest next => (first :: rest) ++ next.bytes

def Stream.fault : Stream → Option Fault
  | .terminal fault => fault
  | .chunk _ _ next => next.fault

def dropBuffer (buffer : List UInt8) (next : Stream) (count : Nat) : Stream :=
  match buffer.drop count with
  | [] => next
  | first :: rest => .chunk first rest next

private theorem dropBuffer_bytes (buffer : List UInt8) (next : Stream) (count : Nat) :
    (dropBuffer buffer next count).bytes = buffer.drop count ++ next.bytes := by
  unfold dropBuffer
  cases buffer.drop count <;> rfl

private theorem dropBuffer_fault (buffer : List UInt8) (next : Stream) (count : Nat) :
    (dropBuffer buffer next count).fault = next.fault := by
  unfold dropBuffer
  cases buffer.drop count <;> rfl

inductive Event
  | fill (side : Side)
  | consume (side : Side) (count : Nat)
  deriving DecidableEq

inductive Outcome
  | equal
  | different
  | failed (side : Side) (fault : Fault)
  | exhausted
  deriving DecidableEq

inductive Decision
  | stop (outcome : Outcome)
  | advance (left right : Stream)

structure Step where
  events : List Event
  decision : Decision

def pairFills : List Event := [.fill .left, .fill .right]

def compareBuffers (left right : List UInt8) (leftTail rightTail : Stream) : Step :=
  let count := min left.length right.length
  if left.take count = right.take count then
    ⟨pairFills ++ [.consume .left count, .consume .right count],
      .advance (dropBuffer left leftTail count) (dropBuffer right rightTail count)⟩
  else ⟨pairFills, .stop .different⟩

def step : Stream → Stream → Step
  | .terminal (some error), _ => ⟨[.fill .left], .stop (.failed .left error)⟩
  | _, .terminal (some error) => ⟨pairFills, .stop (.failed .right error)⟩
  | .terminal none, .terminal none => ⟨pairFills, .stop .equal⟩
  | .terminal none, .chunk .. | .chunk .., .terminal none => ⟨pairFills, .stop .different⟩
  | .chunk a as' leftTail, .chunk b bs' rightTail => compareBuffers (a :: as') (b :: bs') leftTail rightTail

structure Scan where
  events : List Event
  outcome : Outcome
  deriving DecidableEq

def runWith (iterate : Stream → Stream → Step) : Nat → Stream → Stream → Scan
  | 0, _, _ => ⟨[], .exhausted⟩
  | fuel + 1, left, right =>
    let current := iterate left right
    match current.decision with
    | .stop outcome => ⟨current.events, outcome⟩
    | .advance a b =>
      let rest := runWith iterate fuel a b
      ⟨current.events ++ rest.events, rest.outcome⟩

def run (left right : Stream) : Scan :=
  runWith step (left.bytes.length + right.bytes.length + 1) left right

private theorem equal_iff_equal_suffix (count : Nat) (left right : List UInt8)
    (prefixMatch : left.take count = right.take count) :
    left = right ↔ left.drop count = right.drop count := by
  constructor
  · rintro rfl
    rfl
  · intro suffixMatch
    calc
      left = left.take count ++ left.drop count := (List.take_append_drop count left).symm
      _ = right.take count ++ right.drop count := by rw [prefixMatch, suffixMatch]
      _ = right := List.take_append_drop count right

private theorem healthy_with_fuel (fuel : Nat) (left right : Stream)
    (enough : left.bytes.length + right.bytes.length < fuel)
    (leftHealthy : left.fault = none) (rightHealthy : right.fault = none) :
    (runWith step fuel left right).outcome =
      if left.bytes = right.bytes then .equal else .different := by
  induction fuel generalizing left right with
  | zero => omega
  | succ fuel ih =>
    cases left with
    | terminal leftFault =>
      change leftFault = none at leftHealthy
      subst leftFault
      cases right with
      | terminal rightFault =>
        change rightFault = none at rightHealthy
        subst rightFault
        simp [runWith, step, Stream.bytes]
      | chunk =>
        simp [runWith, step, Stream.bytes]
    | chunk a as' leftTail =>
      cases right with
      | terminal rightFault =>
        change rightFault = none at rightHealthy
        subst rightFault
        simp [runWith, step, Stream.bytes]
      | chunk b bs' rightTail =>
        let left := a :: as'
        let right := b :: bs'
        let count := min left.length right.length
        let l := left ++ leftTail.bytes
        let r := right ++ rightTail.bytes
        have positive : 0 < count := by simp [count, left, right]
        have leftFits : count ≤ left.length := Nat.min_le_left _ _
        have rightFits : count ≤ right.length := Nat.min_le_right _ _
        have takeLeft : l.take count = left.take count := List.take_append_of_le_length leftFits
        have takeRight : r.take count = right.take count := List.take_append_of_le_length rightFits
        change l.length + r.length < fuel + 1 at enough
        change (runWith step (fuel + 1) (.chunk a as' leftTail) (.chunk b bs' rightTail)).outcome =
          if l = r then .equal else .different
        by_cases same : left.take count = right.take count
        · have dropLeft : (dropBuffer left leftTail count).bytes = l.drop count := by
            rw [dropBuffer_bytes, show l.drop count = left.drop count ++ leftTail.bytes
              from List.drop_append_of_le_length leftFits]
          have dropRight : (dropBuffer right rightTail count).bytes = r.drop count := by
            rw [dropBuffer_bytes, show r.drop count = right.drop count ++ rightTail.bytes
              from List.drop_append_of_le_length rightFits]
          have smaller : (dropBuffer left leftTail count).bytes.length +
              (dropBuffer right rightTail count).bytes.length < fuel := by
            rw [dropLeft, dropRight, List.length_drop, List.length_drop]
            have leftAvailable : count ≤ l.length := by
              simp only [l, List.length_append]
              omega
            omega
          have matchWhole := equal_iff_equal_suffix count l r (by rw [takeLeft, takeRight, same])
          have tailResult := ih _ _ smaller
            (by rwa [dropBuffer_fault]) (by rwa [dropBuffer_fault])
          have nextStep : step (.chunk a as' leftTail) (.chunk b bs' rightTail) =
              ⟨pairFills ++ [.consume .left count, .consume .right count],
                .advance (dropBuffer left leftTail count) (dropBuffer right rightTail count)⟩ := by
            change compareBuffers left right leftTail rightTail = _
            simp only [compareBuffers, count, if_pos (show
              left.take (min left.length right.length) = right.take (min left.length right.length)
              from same)]
          simp only [runWith, nextStep]
          rw [tailResult, dropLeft, dropRight]
          simp only [← matchWhole]
        · have different : l ≠ r := by
            intro eq
            have := congrArg (List.take count) eq
            rw [takeLeft, takeRight] at this
            exact same this
          have nextStep : step (.chunk a as' leftTail) (.chunk b bs' rightTail) =
              ⟨pairFills, .stop .different⟩ := by
            change compareBuffers left right leftTail rightTail = _
            simp only [compareBuffers, if_neg (show
              ¬left.take (min left.length right.length) = right.take (min left.length right.length)
              from same)]
          simp [runWith, nextStep, different]

theorem comparison_matches_all_bytes (left right : Stream)
    (leftHealthy : left.fault = none) (rightHealthy : right.fault = none) :
    (run left right).outcome = if left.bytes = right.bytes then .equal else .different :=
  healthy_with_fuel _ left right (Nat.lt_succ_self _) leftHealthy rightHealthy

theorem left_error_does_not_read_right (fault : Fault) (right : Stream) :
    run (.terminal (some fault)) right = ⟨[.fill .left], .failed .left fault⟩ := by
  rfl

theorem right_error_precedes_eof_decision (fault : Fault) :
    run (.terminal none) (.terminal (some fault)) = ⟨pairFills, .failed .right fault⟩ := by
  rfl

end HoiminOracle.BufferedComparison
