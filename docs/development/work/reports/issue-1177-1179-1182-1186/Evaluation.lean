import Std

/- Finite counterexample only: this does not verify Python, Git or hoimin. -/
structure Input where
  a : Bool
  b : Bool
  c : Bool
  d : Bool
deriving DecidableEq, Repr

def allRows : List Input := [
  ⟨false,false,false,false⟩, ⟨false,false,false,true⟩,
  ⟨false,false,true,false⟩, ⟨false,false,true,true⟩,
  ⟨false,true,false,false⟩, ⟨false,true,false,true⟩,
  ⟨false,true,true,false⟩, ⟨false,true,true,true⟩,
  ⟨true,false,false,false⟩, ⟨true,false,false,true⟩,
  ⟨true,false,true,false⟩, ⟨true,false,true,true⟩,
  ⟨true,true,false,false⟩, ⟨true,true,false,true⟩,
  ⟨true,true,true,false⟩, ⟨true,true,true,true⟩]

def gate (x : Input) : Bool := x.a && x.b && x.c && x.d
def dropLast (x : Input) : Bool := x.a && x.b && x.c
def evenRows := allRows.filter fun x => !(x.a ^^ x.b ^^ x.c ^^ x.d)

def covers3 (rows : List Input) : Bool := allRows.all fun r =>
  (rows.any fun s => s.a == r.a && s.b == r.b && s.c == r.c) &&
  (rows.any fun s => s.a == r.a && s.b == r.b && s.d == r.d) &&
  (rows.any fun s => s.a == r.a && s.c == r.c && s.d == r.d) &&
  (rows.any fun s => s.b == r.b && s.c == r.c && s.d == r.d)

theorem parity_covers_three : covers3 evenRows = true := by decide
theorem parity_misses_fault : evenRows.all (fun r => gate r == dropLast r) = true := by decide
theorem full_detects_fault : allRows.any (fun r => gate r != dropLast r) = true := by decide

def zeroTotal (_ : List Nat) : Nat := 0
theorem zero_permutation (xs : List Nat) : zeroTotal xs.reverse = zeroTotal xs := by rfl
theorem zero_concatenation (xs ys : List Nat) :
    zeroTotal (xs ++ ys) = zeroTotal xs + zeroTotal ys := by rfl
theorem zero_wrong_anchor : zeroTotal [1] ≠ 1 := by decide

#print axioms parity_covers_three
#print axioms parity_misses_fault
#print axioms full_detects_fault
#print axioms zero_permutation
#print axioms zero_concatenation
#print axioms zero_wrong_anchor
