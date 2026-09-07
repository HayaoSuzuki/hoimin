import Std

namespace HoiminOracle.ResourceCleanup

inductive Event
  | beginCleanup | removeDirectory | commit | retryableFailure
  | probeRegistry | probeCounters | close | admit
  deriving Repr, DecidableEq, BEq

inductive Variant
  | correct | holdsRegistry | exposesCounters | duplicateCleanup | admitsAfterClose
  deriving DecidableEq, BEq

structure State where
  registered : Bool := true
  directoryExists : Bool := true
  active : Bool := true
  cleaning : Bool := false
  closed : Bool := false
  registryHeld : Bool := false
  violation : Bool := false
  deriving Repr, DecidableEq, BEq

def reserve (s : State) : State :=
  { s with cleaning := true, active := false, registryHeld := false }

theorem reservation_hides_counters_and_releases_registry (s : State) :
    (reserve s).active = false ∧ (reserve s).registryHeld = false ∧
    (reserve s).cleaning = true := by
  simp [reserve]

def step (variant : Variant) (s : State) : Event → State
  | .beginCleanup =>
      if s.cleaning then
        if variant == .duplicateCleanup then { s with violation := true } else s
      else if s.registered then
        { reserve s with
          active := variant == .exposesCounters
          registryHeld := variant == .holdsRegistry }
      else s
  | .removeDirectory =>
      if s.cleaning then { s with directoryExists := false } else s
  | .commit =>
      if s.cleaning && !s.directoryExists then
        { s with registered := false, cleaning := false, registryHeld := false }
      else s
  | .retryableFailure =>
      if s.cleaning && s.directoryExists then
        { s with cleaning := false, active := true, registryHeld := false }
      else s
  | .probeRegistry => { s with violation := s.violation || s.registryHeld }
  | .probeCounters =>
      { s with violation := s.violation ||
          (s.registered && s.active && !s.directoryExists) }
  | .close => { s with closed := true }
  | .admit =>
      if s.closed then
        if variant == .admitsAfterClose then { s with violation := true } else s
      else if !s.registered then { closed := false, violation := s.violation }
      else s

def run (variant : Variant) (events : List Event) : State :=
  events.foldl (step variant) {}

end HoiminOracle.ResourceCleanup
