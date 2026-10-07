import Std

namespace HoiminOracle.LineDiverse

/- Boundary: candidates already have unique original ranks and scores from discovery.
   Paths, start lines, scores and ranks are public saved-candidate observations.
   The executable fixture supplies the same two files and explicit operators to Rust.
   Parsing, score calculation, end lines, execution and persistence are excluded.
   All generated cases are strict; arbitrary-list theorems concern only this model. -/
structure Candidate where
  id : String
  path : String
  line : Nat
  rank : Nat
  score : Nat
  deriving Repr, DecidableEq, BEq

inductive Policy | strict | diverse | lineDiverse
  deriving Repr, DecidableEq, BEq

def sameGroup (policy : Policy) (a b : Candidate) : Bool :=
  a.score == b.score && a.path == b.path &&
    (policy != .lineDiverse || a.line == b.line)

-- An independent declarative round-robin model: each bucket's nth member is
-- ordered in round n; the bucket's first original rank breaks round ties.
def roundIndex (policy : Policy) (xs : List Candidate) (c : Candidate) : Nat :=
  (xs.filter fun x => sameGroup policy x c && x.rank < c.rank).length

def groupFirstRank (policy : Policy) (xs : List Candidate) (c : Candidate) : Nat :=
  (xs.filter fun x => sameGroup policy x c).foldl (fun n x => min n x.rank) c.rank

def before (policy : Policy) (xs : List Candidate) (a b : Candidate) : Bool :=
  a.score > b.score || (a.score == b.score &&
    (roundIndex policy xs a < roundIndex policy xs b ||
      (roundIndex policy xs a == roundIndex policy xs b &&
        groupFirstRank policy xs a <= groupFirstRank policy xs b)))

-- Structural insertion sort keeps the small fixed witnesses kernel-reducible.
def insert {α : Type} (le : α → α → Bool) (a : α) : List α → List α
  | [] => [a]
  | b :: rest => if le a b then a :: b :: rest else b :: insert le a rest

def sort {α : Type} (le : α → α → Bool) : List α → List α
  | [] => []
  | a :: rest => insert le a (sort le rest)

set_option maxHeartbeats 10000 in
theorem insert_perm {α : Type} (le : α → α → Bool) (a : α) (xs : List α) :
    (insert le a xs).Perm (a :: xs) := by
  induction xs with
  | nil => exact List.Perm.refl _
  | cons b rest ih =>
    simp only [insert]
    split
    · exact List.Perm.refl _
    · exact (ih.cons b).trans (List.Perm.swap a b rest)

set_option maxHeartbeats 10000 in
theorem sort_perm {α : Type} (le : α → α → Bool) (xs : List α) :
    (sort le xs).Perm xs := by
  induction xs with
  | nil => exact List.Perm.refl _
  | cons a rest ih => exact (insert_perm le a _).trans (ih.cons a)

def order (policy : Policy) (xs : List Candidate) : List Candidate :=
  if policy == .strict then xs else sort (before policy xs) xs

def ids (xs : List Candidate) : List String := xs.map (·.id)
def page {α : Type} (xs : List α) (offset count : Nat) : List α :=
  (xs.drop offset).take count

-- Multiplicity is preserved even without unique input IDs/ranks. Thus no
-- candidate is lost or invented, and unique input IDs remain unique.
set_option maxHeartbeats 10000 in
theorem order_perm (policy : Policy) (xs : List Candidate) :
    (order policy xs).Perm xs := by
  unfold order
  split
  · exact List.Perm.refl xs
  · exact sort_perm _ _

set_option maxHeartbeats 10000 in
theorem ids_perm (policy : Policy) (xs : List Candidate) :
    (ids (order policy xs)).Perm (ids xs) :=
  (order_perm policy xs).map Candidate.id

set_option maxHeartbeats 10000 in
theorem no_duplicate_ids (policy : Policy) (xs : List Candidate)
    (unique : (ids xs).Nodup) : (ids (order policy xs)).Nodup :=
  (ids_perm policy xs).symm.nodup unique

set_option maxHeartbeats 10000 in
theorem slice_prefix {α : Type} (xs : List α) (offset count : Nat) :
    (xs.take (offset + count)).drop offset = page xs offset count := by
  unfold page
  induction offset generalizing xs with
  | zero => simp
  | succ n ih =>
    cases xs with
    | nil => simp
    | cons x xs => simpa [Nat.succ_add] using ih xs

def fixture : List Candidate := [
  ⟨"a1", "a.py", 1, 1, 100⟩, ⟨"a2", "a.py", 1, 2, 100⟩,
  ⟨"a3", "a.py", 1, 3, 100⟩, ⟨"a4", "a.py", 2, 4, 100⟩,
  ⟨"b1", "b.py", 1, 5, 100⟩, ⟨"a5", "a.py", 3, 6, 70⟩,
  ⟨"a6", "a.py", 3, 7, 70⟩, ⟨"b2", "b.py", 2, 8, 70⟩]

set_option maxHeartbeats 10000 in
example : ids (order .lineDiverse fixture) =
    ["a1", "a4", "b1", "a2", "a3", "a5", "b2", "a6"] := by decide

set_option maxHeartbeats 10000 in
example : ids (order .diverse fixture) =
    ["a1", "b1", "a2", "a3", "a4", "a5", "b2", "a6"] := by decide

-- Within-tier first rounds include each distinct (path, line) before repeats;
-- lower scores cannot be promoted past any remaining higher-score candidate.
set_option maxHeartbeats 10000 in
example : ((order .lineDiverse fixture).take 3).map (fun c => (c.path, c.line)) =
    [("a.py", 1), ("a.py", 2), ("b.py", 1)] := by decide
set_option maxHeartbeats 10000 in
example : ((order .lineDiverse fixture).take 5).all (·.score == 100) = true := by decide

def brokenLineOnly : List Candidate :=
  order .lineDiverse (fixture.map fun c => { c with path := "merged.py" })
def brokenIgnoredTiers : List Candidate :=
  order .lineDiverse (fixture.map fun c => { c with score := 100 })

def sensitivityChecks : List (String × Bool) := [
  ("file_only", ids (order .lineDiverse fixture) != ids (order .diverse fixture)),
  ("line_only_across_paths", ids (order .lineDiverse fixture) != ids brokenLineOnly),
  ("ignored_tiers", ids (order .lineDiverse fixture) != ids brokenIgnoredTiers),
  ("offset_before_diversity", page (ids (order .lineDiverse fixture)) 1 1 !=
    (ids (order .lineDiverse (fixture.drop 1))).take 1),
  ("take_before_offset", page (ids (order .lineDiverse fixture)) 1 1 !=
    ((ids (order .lineDiverse fixture)).take 1).drop 1)]

set_option maxHeartbeats 20000 in
example : sensitivityChecks.all (·.2) = true := by decide

set_option maxHeartbeats 10000 in
example : page (ids (order .lineDiverse fixture)) 0 3 ++
    page (ids (order .lineDiverse fixture)) 3 3 ++
    page (ids (order .lineDiverse fixture)) 6 3 =
    ids (order .lineDiverse fixture) := by decide

end HoiminOracle.LineDiverse
