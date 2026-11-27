import Init.Data.Order.Ord

namespace HoiminOracle.RunFingerprint

open Std

abbrev Bytes := List Nat

def littleEndian : Nat → Nat → Bytes
  | 0, _ => []
  | width + 1, value => value % 256 :: littleEndian width (value / 256)

private def readLittleEndian : Bytes → Nat
  | [] => 0
  | byte :: rest => byte + 256 * readLittleEndian rest

@[simp] theorem littleEndian_length (width value : Nat) : (littleEndian width value).length = width := by
  induction width generalizing value with
  | zero => rfl
  | succ width ih => simp [littleEndian, ih]

theorem littleEndian_roundtrip (width value : Nat) (fits : value < 256 ^ width) :
    readLittleEndian (littleEndian width value) = value := by
  induction width generalizing value with
  | zero => simp [littleEndian, readLittleEndian] at ⊢; simp at fits; omega
  | succ width ih =>
    have bound : value / 256 < 256 ^ width := by
      apply Nat.div_lt_of_lt_mul
      simpa [Nat.pow_succ, Nat.mul_comm] using fits
    rw [littleEndian, readLittleEndian, ih _ bound]
    exact Nat.mod_add_div value 256

def sized (data : Bytes) : Bytes := littleEndian 8 data.length ++ data

theorem sized_split (left right first rest : Bytes)
    (leftFits : left.length < 256 ^ 8) (rightFits : right.length < 256 ^ 8)
    (equal : sized left ++ first = sized right ++ rest) :
    left = right ∧ first = rest := by
  simp only [sized, List.append_assoc] at equal
  have sizeEncodings := List.append_inj_left equal (by simp)
  have sizes := congrArg readLittleEndian sizeEncodings
  rw [littleEndian_roundtrip _ _ leftFits, littleEndian_roundtrip _ _ rightFits] at sizes
  exact List.append_inj (List.append_inj_right equal (by simp)) sizes

def frameStream : List (Nat × Bytes) → Bytes
  | [] => []
  | field :: rest => field.1 :: (sized field.2 ++ frameStream rest)

theorem frames_distinguish_sections (left right : List (Nat × Bytes))
    (leftFits : ∀ field ∈ left, field.2.length < 256 ^ 8)
    (rightFits : ∀ field ∈ right, field.2.length < 256 ^ 8)
    (same : frameStream left = frameStream right) : left = right := by
  induction left generalizing right with
  | nil => cases right <;> simp [frameStream] at same ⊢
  | cons head tail ih =>
    cases right with
    | nil => simp [frameStream] at same
    | cons first rest =>
      simp only [frameStream, List.cons.injEq] at same
      have split := sized_split _ _ _ _ (leftFits head (by simp)) (rightFits first (by simp)) same.2
      have equalHead : head = first := Prod.ext same.1 split.1
      have equalTail := ih rest (fun field mem => leftFits field (by simp [mem]))
        (fun field mem => rightFits field (by simp [mem])) split.2
      rw [equalHead, equalTail]

def utf8 (text : String) : Bytes := text.toUTF8.toList.map UInt8.toNat

def counted (encode : α → Bytes) (items : List α) : Bytes :=
  littleEndian 8 items.length ++ items.flatMap encode

private def sorted [Ord α] (items : List α) : List α :=
  items.mergeSort fun left right => (compare left right).isLE

private def orderedSet [BEq α] [Ord α] (items : List α) : List α :=
  (sorted items).eraseDups

private theorem orderedSet_perm [BEq α] [Ord α] [OrientedOrd α] [TransOrd α] [LawfulEqOrd α]
    {left right : List α} (perm : List.Perm left right) : orderedSet left = orderedSet right := by
  have total (a b : α) : (compare a b).isLE || (compare b a).isLE := by
    rw [OrientedOrd.eq_swap (a := b) (b := a)]
    cases compare a b <;> decide
  have sortedPairwise (items : List α) : (sorted items).Pairwise (fun a b => (compare a b).isLE) :=
    List.pairwise_mergeSort (le := fun a b => (compare a b).isLE)
      (fun a b c => TransOrd.isLE_trans (a := a) (b := b) (c := c)) total items
  have permSorted : List.Perm (sorted left) (sorted right) :=
    (List.mergeSort_perm _ _).trans (perm.trans (List.mergeSort_perm _ _).symm)
  have same := permSorted.eq_of_pairwise
    (fun _ _ _ _ ab ba => LawfulEqOrd.eq_of_compare (OrientedCmp.isLE_antisymm ab ba))
    (sortedPairwise left) (sortedPairwise right)
  simp [orderedSet, same]

structure Source where
  path : String
  hash : Bytes

structure InputFile where
  path : String
  hash : String
  deriving BEq

structure Line where
  start : Nat
  endLine : Nat
  deriving BEq

structure Target where
  path : String
  lines : List Line := []
  symbols : List String := []

inductive Arg
  | unix (bytes : Bytes)
  | windows (units : List Nat)

structure Duration where
  secs : Nat
  nanos : Nat := 0

structure Limits where
  jobs : Nat := 1
  maxMutants : Nat := 100
  maxCandidates : Nat := 10000
  baseline : Duration := ⟨60, 0⟩
  mutant : Option Duration := none
  total : Duration := ⟨300, 0⟩
  maxMemory : Nat := 1073741824
  maxOutput : Nat := 1048576
  maxCopy : Nat := 1073741824
  maxWorkspace : Nat := 8589934592
  minFree : Nat := 10737418240
  maxProcesses : Nat := 64

inductive ResourceMode | hard | bestEffort
inductive Profile | full | focused
inductive Environment | untracked | captured (names : List String) (digest : String)

structure Input where
  sources : List Source := []
  targets : List Target := []
  operators : List String := []
  argv : List Arg := []
  limits : Limits := {}
  resourceMode : ResourceMode := .bestEffort
  profile : Profile := .full
  inputFiles : List InputFile := []
  importRoots : List String := []
  includes : List String := []
  excludes : List String := []
  sourceRoots : List String := []
  environment : Environment := .untracked

private def sourceBytes (source : Source) : Bytes :=
  sized (utf8 source.path) ++ sized source.hash

private def targetBytes (target : Target) : Bytes :=
  let lines := target.lines.mergeSort fun a b =>
    a.start < b.start || (a.start == b.start && a.endLine ≤ b.endLine)
  sized (utf8 target.path) ++
    counted (fun line => littleEndian 4 line.start ++ littleEndian 4 line.endLine) lines ++
    counted (sized ∘ utf8) (orderedSet target.symbols)

private def inputFileKey (file : InputFile) : List String × String :=
  (file.path.splitOn "/", file.hash)

private def inputFileBytes (entries : List InputFile) : Bytes :=
  let files := entries.mergeSort fun a b => lexOrd.compare (inputFileKey a) (inputFileKey b) != .gt
  counted (fun file => sized (utf8 file.path) ++ sized (utf8 file.hash)) files.eraseDups

private def argument : Arg → Bytes
  | .unix data => 0 :: sized data
  | .windows units => 1 :: counted (littleEndian 2) units

private def duration (value : Duration) : Bytes :=
  littleEndian 8 value.secs ++ littleEndian 4 value.nanos

private def compatibilityLimits (limits : Limits) : Bytes :=
  let mutant := match limits.mutant with
    | none => [0]
    | some value => 1 :: duration value
  littleEndian 8 limits.maxCandidates ++ duration limits.baseline ++ mutant ++ duration limits.total ++
    ([ limits.maxMemory, limits.maxCopy, limits.maxWorkspace, limits.minFree, limits.maxProcesses ].flatMap
      (littleEndian 8))

private def stringList : List String → Bytes := counted (sized ∘ utf8)

private def environmentBytes : Environment → Bytes
  | .untracked => sized (stringList []) ++ [0]
  | .captured names hash => sized (stringList names) ++ (1 :: sized (utf8 hash))

def sections (input : Input) : List (Nat × Bytes) :=
  [ (1, counted sized (orderedSet (input.sources.map sourceBytes)))
  , (2, counted sized (orderedSet (input.targets.map targetBytes)))
  , (3, stringList (orderedSet input.operators))
  , (4, counted argument input.argv)
  , (5, compatibilityLimits input.limits)
  , (6, [match input.resourceMode with | .hard => 0 | .bestEffort => 1])
  , (7, [match input.profile with | .full => 0 | .focused => 1])
  , (8, inputFileBytes input.inputFiles)
  , (9, stringList input.importRoots)
  , (10, stringList input.includes)
  , (11, stringList input.excludes)
  , (12, stringList input.sourceRoots)
  , (13, environmentBytes input.environment)
  ]

def schemaVersion : Nat := 13

def preimage (input : Input) : Bytes :=
  schemaVersion :: frameStream (sections input)

theorem preimage_distinguishes_sections (left right : Input)
    (leftFits : ∀ field ∈ sections left, field.2.length < 256 ^ 8)
    (rightFits : ∀ field ∈ sections right, field.2.length < 256 ^ 8)
    (same : preimage left = preimage right) : sections left = sections right :=
  frames_distinguish_sections _ _ leftFits rightFits (List.cons.inj same).2

theorem execution_only_limits_do_not_enter_preimage (input : Input) (jobs mutants output : Nat) :
    preimage { input with limits := { input.limits with
      jobs := jobs, maxMutants := mutants, maxOutput := output } } = preimage input := rfl

theorem permuting_sources_targets_and_operators_preserves_preimage (input : Input)
    (sources : List Source) (targets : List Target) (operators : List String)
    (files : List.Perm sources input.sources) (slices : List.Perm targets input.targets)
    (names : List.Perm operators input.operators) :
    preimage { input with sources, targets, operators } = preimage input := by
  simp [preimage, sections, orderedSet_perm (files.map sourceBytes),
    orderedSet_perm (slices.map targetBytes), orderedSet_perm names]

theorem command_kind_separates_arguments (bytes units : Bytes) :
    argument (.unix bytes) ≠ argument (.windows units) := by
  simp [argument]

end HoiminOracle.RunFingerprint
