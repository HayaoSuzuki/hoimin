import Std

namespace CopyResume

structure Config where
  copied : Bool
  outputCap : Nat
  deriving DecidableEq, BEq, Repr

structure Cached where
  config : Config
  killed : Bool
  deriving DecidableEq, BEq, Repr

structure Result where
  killed : Bool
  reused : Bool
  deriving DecidableEq, BEq, Repr

def compatible (before after : Config) : Bool := before.copied == after.copied
def save (config : Config) : Cached := ⟨config, config.copied⟩
def resume (cached : Cached) (current : Config) : Result :=
  if compatible cached.config current then ⟨cached.killed, true⟩
  else ⟨current.copied, false⟩

-- Broken: all other fingerprint fields agree, and copy policy is omitted.
def brokenResume (cached : Cached) (_current : Config) : Result := ⟨cached.killed, true⟩
-- Broken in the opposite direction: an operational output limit invalidates reuse.
def brokenOutputKey (cached : Cached) (current : Config) : Bool := cached.config == current

theorem changed_copy_never_reuses (cached : Cached) (current : Config)
    (changed : cached.config.copied ≠ current.copied) :
    (resume cached current).reused = false := by
  simp [resume, compatible, changed]

theorem resumed_verdict_matches_fresh (cached : Cached) (current : Config)
    (valid : cached.killed = cached.config.copied) :
    (resume cached current).killed = current.copied := by
  simp only [resume, compatible]
  split
  next h => simpa [valid] using h
  next => rfl

theorem output_cap_does_not_change_compatibility (copied : Bool) (before after : Nat) :
    compatible ⟨copied, before⟩ ⟨copied, after⟩ = true := by simp [compatible]

theorem saved_record_is_valid (config : Config) :
    (save config).killed = (save config).config.copied := by rfl

end CopyResume
