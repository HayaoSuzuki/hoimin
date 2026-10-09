import Std

namespace HoiminOracle.Sampling

def nextWord (state : UInt64) : UInt64 × UInt64 :=
  let state := state + 0x9e3779b97f4a7c15
  let a := (state ^^^ (state >>> 30)) * 0xbf58476d1ce4e5b9
  let b := (a ^^^ (a >>> 27)) * 0x94d049bb133111eb
  (state, b ^^^ (b >>> 31))

-- Model-only fuel; corpus generation fails if it is exhausted.
def draw : Nat → UInt64 → UInt64 → Option (UInt64 × Nat)
  | 0, _, _ => none
  | fuel + 1, state, bound =>
    let (state, word) := nextWord state
    if word < (0 - bound) % bound then draw fuel state bound
    else some (state, (word % bound).toNat)

def swapAt (xs : Array Nat) (i j : Nat) : Array Nat :=
  if hi : i < xs.size then
    if hj : j < xs.size then xs.swap i j else xs
  else xs

set_option maxHeartbeats 20000 in
theorem swapAt_perm (xs : Array Nat) (i j : Nat) : (swapAt xs i j).Perm xs := by
  unfold swapAt
  split
  · split
    · exact Array.swap_perm _ _
    · exact .refl _
  · exact .refl _

def swaps : List (Nat × Nat) → Array Nat → Array Nat
  | [], xs => xs
  | (i,j) :: rest, xs => swaps rest (swapAt xs i j)

set_option maxHeartbeats 20000 in
theorem swaps_perm (steps : List (Nat × Nat)) (xs : Array Nat) :
    (swaps steps xs).Perm xs := by
  induction steps generalizing xs with
  | nil => exact .refl _
  | cons step rest ih => exact (ih _).trans (swapAt_perm xs step.1 step.2)

set_option maxHeartbeats 20000 in
theorem swaps_length (steps : List (Nat × Nat)) (xs : Array Nat) :
    (swaps steps xs).size = xs.size := (swaps_perm steps xs).size_eq

set_option maxHeartbeats 20000 in
theorem swaps_nodup (steps : List (Nat × Nat)) (xs : Array Nat)
    (h : xs.toList.Nodup) : (swaps steps xs).toList.Nodup := by
  exact ((Array.perm_iff_toList_perm.mp (swaps_perm steps xs)).nodup_iff).mpr h

def shuffle : Nat → Nat → UInt64 → Array Nat → Option (Array Nat)
  | 0, _, _, xs => some xs
  | remaining + 1, i, state, xs => do
    let (state, offset) ← draw 128 state (UInt64.ofNat (xs.size - i))
    shuffle remaining (i+1) state (swapAt xs i (i+offset))

def sample (population count : Nat) (seed : UInt64) : Option (List Nat) := do
  let xs ← shuffle (min population count) 0 seed (List.range population).toArray
  pure (xs.toList.take (min population count))

def accepts (population count budget : Nat) (truncated : Bool) : Bool :=
  !truncated && population > 0 && count > 0 && min population count ≤ budget

set_option maxHeartbeats 20000 in
theorem accepted_budget (p k b : Nat) (t : Bool) (h : accepts p k b t = true) :
    min p k ≤ b := by simp [accepts] at h; omega

set_option maxHeartbeats 20000 in
theorem accepted_complete_population (p k b : Nat) (t : Bool)
    (h : accepts p k b t = true) : t = false ∧ p > 0 := by
  simp [accepts] at h
  exact ⟨h.1.1.1, h.1.1.2⟩

-- Replacement duplicates, ignored truncation, silent budget clipping, and
-- modulo bias with reduced words are all observable by the audit.
def sensitivity : Bool :=
  !([0,0] : List Nat).isPerm [0,1] &&
  !accepts 2 1 2 true && !accepts 2 2 1 false &&
  ((List.range 16).filter (fun n => n % 3 == 0)).length !=
    ((List.range 16).filter (fun n => n % 3 == 1)).length

set_option maxHeartbeats 20000 in
example : sensitivity = true := by decide

end HoiminOracle.Sampling

namespace HoiminOracle.Sampling
set_option maxHeartbeats 20000 in
theorem shuffle_perm (count i : Nat) (seed : UInt64) (xs ys : Array Nat)
    (h : shuffle count i seed xs = some ys) : ys.Perm xs := by
  induction count generalizing i seed xs with
  | zero => simp [shuffle] at h; subst ys; exact .refl _
  | succ n ih =>
    cases hd : draw 128 seed (UInt64.ofNat (xs.size - i)) with
    | none => simp [shuffle, hd] at h
    | some pair =>
      simp [shuffle, hd] at h
      exact (ih (i+1) pair.1 (swapAt xs i (i+pair.2)) h).trans (swapAt_perm xs i (i+pair.2))
end HoiminOracle.Sampling

namespace HoiminOracle.Sampling
set_option maxHeartbeats 20000 in
theorem shuffled_prefix_nodup (count i k : Nat) (seed : UInt64) (xs ys : Array Nat)
    (h : shuffle count i seed xs = some ys) (unique : xs.toList.Nodup) :
    (ys.toList.take k).Nodup := by
  have p := Array.perm_iff_toList_perm.mp (shuffle_perm count i seed xs ys h)
  exact (p.nodup_iff.mpr unique).take

set_option maxHeartbeats 20000 in
theorem shuffled_prefix_length (count i k : Nat) (seed : UInt64) (xs ys : Array Nat)
    (h : shuffle count i seed xs = some ys) :
    (ys.toList.take k).length = min k xs.size := by
  simp only [List.length_take, Array.length_toList]
  rw [(shuffle_perm count i seed xs ys h).size_eq]

-- Reduced-word exhaustive example demonstrates why rejecting the short residue
-- prefix removes modulo bias. This is model-only, not a PRNG-quality theorem.
set_option maxHeartbeats 20000 in
example : ([0,1,2].map fun r =>
    ((List.range 16).filter fun n => n >= 1 && n % 3 == r).length) = [5,5,5] := by decide
end HoiminOracle.Sampling
