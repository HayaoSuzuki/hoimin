namespace HoiminOracle.ExceptionAcquisition

structure Limits where
  perFile : Nat
  total : Nat

structure Source where
  path : String
  rawBytes : Nat
  fingerprintMatches : Bool
  decodedBytes : Option Nat
  depthFailure : Bool := false

inductive Failure
  | rawLimit (path : String)
  | fingerprint (path : String)
  | encoding (path : String)
  | decodedFileLimit (path : String)
  | decodedTotalLimit
  | analysisDepth
  | cancelled

def charge (perFile remaining : Nat) (source : Source) : Except Failure Nat :=
  if min perFile remaining < source.rawBytes then .error (.rawLimit source.path)
  else if !source.fingerprintMatches then .error (.fingerprint source.path)
  else match source.decodedBytes with
    | none => .error (.encoding source.path)
    | some size =>
      if perFile < size then .error (.decodedFileLimit source.path)
      else if remaining < size then .error .decodedTotalLimit
      else .ok (remaining - size)

def acquire (perFile remaining : Nat) (pendingDepth : Bool) : List Source → Except Failure Nat
  | [] => if pendingDepth then .error .analysisDepth else .ok remaining
  | source :: rest => (charge perFile remaining source).bind fun next =>
    acquire perFile next (pendingDepth || source.depthFailure) rest

inductive Outcome | built | cached | failed (reason : Failure)

structure Observation where
  outcome : Outcome
  cached : Bool

def loadWith (acquireAll : Limits → List Source → Except Failure Nat)
    (limits : Limits) (cached cancelled : Bool) (sources : List Source) : Observation :=
  if cancelled then ⟨.failed .cancelled, cached⟩
  else if cached then ⟨.cached, true⟩
  else match acquireAll limits sources with
    | .ok _ => ⟨.built, true⟩
    | .error failure => ⟨.failed failure, false⟩

def load := loadWith fun limits => acquire limits.perFile limits.total false

theorem charge_conserves_decoded_bytes (perFile remaining next size : Nat) (source : Source)
    (decoded : source.decodedBytes = some size)
    (success : charge perFile remaining source = .ok next) : next + size = remaining := by
  simp only [charge, decoded] at success
  split at success <;> try contradiction
  split at success <;> try contradiction
  split at success <;> try contradiction
  split at success <;> try contradiction
  simp only [Except.ok.injEq] at success
  omega

theorem charge_never_increases_remaining (perFile remaining next : Nat) (source : Source)
    (success : charge perFile remaining source = .ok next) : next ≤ remaining := by
  cases decoded : source.decodedBytes with
  | none =>
    simp only [charge, decoded] at success
    split at success
    · contradiction
    · split at success <;> contradiction
  | some size =>
    have balance := charge_conserves_decoded_bytes perFile remaining next size source decoded success
    omega

theorem acquire_never_increases_remaining (perFile remaining next : Nat) (pendingDepth : Bool)
    (sources : List Source) (success : acquire perFile remaining pendingDepth sources = .ok next) :
    next ≤ remaining := by
  induction sources generalizing remaining pendingDepth with
  | nil => by_cases pendingDepth <;> simp_all [acquire]
  | cons source rest ih =>
    cases charged : charge perFile remaining source with
    | error reason => simp [acquire, charged, Except.bind] at success
    | ok left =>
      have later : acquire perFile left (pendingDepth || source.depthFailure) rest = .ok next := by
        simpa [acquire, charged, Except.bind] using success
      exact Nat.le_trans (ih left _ later) (charge_never_increases_remaining perFile remaining left source charged)

theorem acquisition_error_precedes_pending_analysis (perFile remaining : Nat) (pendingDepth : Bool)
    (source : Source) (later : List Source) (reason : Failure)
    (failed : charge perFile remaining source = .error reason) :
    acquire perFile remaining pendingDepth (source :: later) = .error reason := by
  simp [acquire, failed, Except.bind]

theorem cancelled_load_preserves_cache (limits : Limits) (cached : Bool) (sources : List Source) :
    load limits cached true sources = ⟨.failed .cancelled, cached⟩ := rfl

theorem uncached_acquisition_failure_does_not_publish (limits : Limits) (sources : List Source) (reason : Failure)
    (failed : acquire limits.perFile limits.total false sources = .error reason) :
    load limits false false sources = ⟨.failed reason, false⟩ := by
  simp [load, loadWith, failed]

end HoiminOracle.ExceptionAcquisition
