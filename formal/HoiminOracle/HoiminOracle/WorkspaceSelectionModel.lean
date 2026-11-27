namespace HoiminOracle.WorkspaceSelection

abbrev Path := List String

inductive Kind where
  | directory
  | file (content : String)
  | link (target : String)
  deriving DecidableEq

structure Entry where
  path : Path
  kind : Kind

def Kind.isDirectory : Kind → Bool
  | .directory => true
  | _ => false

inductive Pattern where
  | all
  | exact (path : Path)
  | below (path : Path)
  | suffix (suffix : String)
  | directory (path : Path)
  | invalid
  deriving DecidableEq

inductive Literal where
  | file (path : Path)
  | tree (path : Path)

def Literal.path : Literal → Path
  | .file path | .tree path => path

structure Rules where
  caseInsensitive : Bool := false
  ignores : List Pattern := []
  includes : List Pattern := []
  excludes : List Pattern := []
  literals : List Literal := []

def normalizeName (foldCase : Bool) (name : String) : String :=
  if foldCase then name.toLower else name

def samePath (foldCase : Bool) (left right : Path) : Bool :=
  left.map (normalizeName foldCase) == right.map (normalizeName foldCase)

def isPrefix (foldCase : Bool) (ancestor path : Path) : Bool :=
  ancestor.length ≤ path.length && samePath foldCase ancestor (path.take ancestor.length)

def patternMatches (foldCase : Bool) (path : Path) (isDirectory : Bool) : Pattern → Bool
  | .all => true
  | .exact other => samePath foldCase other path
  | .below ancestor => ancestor.length < path.length && isPrefix foldCase ancestor path
  | .suffix suffix => path.getLast?.any fun basename =>
      (normalizeName foldCase basename).endsWith (normalizeName foldCase suffix)
  | .directory other => isDirectory && samePath foldCase other path
  | .invalid => false

def ancestors (path : Path) : List Path := ((List.range path.length).drop 1).map (path.take ·)

def prunes (foldCase : Bool) (rules : List Pattern) (entry : Entry) : Bool :=
  (ancestors entry.path).any (fun path => rules.any (patternMatches foldCase path true)) ||
    rules.any (patternMatches foldCase entry.path entry.kind.isDirectory)

def excludedNames : List String :=
  [".git", ".venv", "venv", "env", "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache",
   ".pyre", ".pytype", ".tox", ".nox"]

def defaultExcluded (foldCase : Bool) (entry : Entry) : Bool :=
  entry.path.any fun name => excludedNames.contains (normalizeName foldCase name)

def literalExcluded (foldCase : Bool) (entry : Entry) : Literal → Bool
  | .file path => !entry.kind.isDirectory && samePath foldCase path entry.path
  | .tree path => isPrefix foldCase path entry.path && (path.length < entry.path.length || entry.kind.isDirectory)

def selected (rules : Rules) (entry : Entry) : Bool :=
  !defaultExcluded rules.caseInsensitive entry &&
  !rules.literals.any (literalExcluded rules.caseInsensitive entry) &&
  !prunes rules.caseInsensitive rules.excludes entry &&
  (!prunes false rules.ignores entry ||
    rules.includes.any (patternMatches rules.caseInsensitive entry.path entry.kind.isDirectory))

theorem defaults_apply_to_both_walks (rules : Rules) (entry : Entry)
    (excluded : defaultExcluded rules.caseInsensitive entry = true) : selected rules entry = false := by
  simp [selected, excluded]

theorem literals_apply_to_both_walks (rules : Rules) (entry : Entry)
    (excluded : rules.literals.any (literalExcluded rules.caseInsensitive entry) = true) :
    selected rules entry = false := by simp [selected, excluded]

theorem excludes_apply_to_both_walks (rules : Rules) (entry : Entry)
    (excluded : prunes rules.caseInsensitive rules.excludes entry = true) : selected rules entry = false := by
  simp [selected, excluded]

theorem includes_override_ignore (rules : Rules) (entry : Entry)
    (allowedName : defaultExcluded rules.caseInsensitive entry = false)
    (allowedLiteral : rules.literals.any (literalExcluded rules.caseInsensitive entry) = false)
    (allowedGlob : prunes rules.caseInsensitive rules.excludes entry = false)
    (included : rules.includes.any (patternMatches rules.caseInsensitive entry.path entry.kind.isDirectory) = true) :
    selected rules entry = true := by simp [selected, allowedName, allowedLiteral, allowedGlob, included]

theorem file_exclusion_preserves_directory (foldCase : Bool) (path other : Path) :
    literalExcluded foldCase ⟨path, .directory⟩ (.file other) = false := rfl

theorem tree_exclusion_preserves_equal_named_file (foldCase : Bool) (path : Path) (content : String) :
    literalExcluded foldCase ⟨path, .file content⟩ (.tree path) = false := by
  simp [literalExcluded, Kind.isDirectory]

structure Manifest where
  files : List (Path × String)
  directories : List Path
  symlinks : List Path
  deriving DecidableEq

def summarize (tree : List Entry) (choose : Entry → Bool) : Manifest :=
  let entries := tree.filter choose
  let ordinary := entries.filter fun entry => match entry.kind with | .link _ => false | _ => true
  { files := entries.filterMap (fun entry => match entry.kind with
      | .file content => some (entry.path, content)
      | _ => none)
    directories := (tree.filter fun entry => entry.kind.isDirectory &&
      ordinary.any (fun child => isPrefix false entry.path child.path)).map Entry.path
    symlinks := entries.filterMap (fun entry => match entry.kind with
      | .link _ => some entry.path
      | _ => none) }

theorem copied_contents_have_a_selected_regular_file (tree : List Entry) (choose : Entry → Bool)
    (path : Path) (content : String) (copied : (path, content) ∈ (summarize tree choose).files) :
    ∃ entry ∈ tree, choose entry = true ∧ entry.path = path ∧ entry.kind = .file content := by
  simp only [summarize, List.mem_filterMap, List.mem_filter] at copied
  obtain ⟨entry, ⟨existsInTree, chosen⟩, found⟩ := copied
  cases kind : entry.kind with
  | directory | link _ => simp [kind] at found
  | file _ =>
    simp [kind] at found
    exact ⟨entry, existsInTree, chosen, found.1, by simp [kind, found.2]⟩

theorem selected_file_preserves_its_ancestor (tree : List Entry) (choose : Entry → Bool)
    (directory file : Entry) (dirExists : directory ∈ tree) (fileExists : file ∈ tree)
    (dirKind : directory.kind = .directory) (content : String) (fileKind : file.kind = .file content)
    (chosen : choose file = true) (ancestor : isPrefix false directory.path file.path = true) :
    directory.path ∈ (summarize tree choose).directories := by
  simp only [summarize, List.mem_map, List.mem_filter, Bool.and_eq_true, List.any_eq_true]
  exact ⟨directory, ⟨dirExists, by simp [dirKind, Kind.isDirectory],
    ⟨file, ⟨⟨fileExists, chosen⟩, by simp [fileKind]⟩, ancestor⟩⟩, rfl⟩

inductive Result where
  | invalidLiteral
  | invalidGlob
  | manifest (value : Manifest)
  deriving DecidableEq

def validLiteral (path : Path) : Bool :=
  !path.isEmpty && path.all (fun part => part ≠ "" && part ≠ "." && part ≠ "..")

def runWith (select : Rules → Entry → Bool) (rules : Rules) (tree : List Entry) : Result :=
  if rules.literals.any (fun exclusion => !validLiteral exclusion.path) then .invalidLiteral
  else if (rules.excludes ++ rules.includes).contains .invalid then .invalidGlob
  else .manifest (summarize tree (select rules))

def run := runWith selected

end HoiminOracle.WorkspaceSelection
