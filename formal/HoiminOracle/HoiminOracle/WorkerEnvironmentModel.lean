namespace HoiminOracle.WorkerEnvironment

abbrev Path := List String

inductive Platform where
  | unix | windows
  deriving DecidableEq

structure PathInput where
  absolute : Bool := false
  parts : Path

structure Tree where
  directories : List Path
  links : List (Path × Path) := []

structure Variable where
  name : String
  paths : List PathInput

structure Input where
  original : Path := ["project"]
  worker : Path := ["worker"]
  imports : List Path := []
  sources : List PathInput := []
  inherited : List Variable := []

def normalize (path : Path) : Path :=
  path.foldl (fun acc part =>
    if part = "." || part = "" then acc
    else if part = ".." then acc.dropLast
    else acc ++ [part]) []

def canonicalFrom (tree : Tree) (current : Path) : List String → Option Path
  | [] => some current
  | part :: rest =>
    if part = "." || part = "" then canonicalFrom tree current rest
    else if part = ".." then canonicalFrom tree current.dropLast rest
    else
      let child := current ++ [part]
      let resolved := (tree.links.lookup child).getD child
      if resolved ∈ tree.directories then canonicalFrom tree resolved rest else none

def resolve (tree : Tree) (path : Path) : Path := (canonicalFrom tree [] path).getD (normalize path)

def key (platform : Platform) (path : Path) : Path :=
  match platform with
  | .unix => normalize path
  | .windows => (normalize path).map String.toLower

def inside (platform : Platform) (root path : Path) : Option Path :=
  if key platform (path.take root.length) = key platform root && root.length ≤ path.length then
    some (path.drop root.length)
  else none

def rewrite (platform : Platform) (tree : Tree) (input : Input) (path : PathInput) : Path :=
  let resolved := resolve tree (if path.absolute then path.parts else input.original ++ path.parts)
  match inside platform (resolve tree input.original) resolved with
  | some rest => input.worker ++ rest
  | none => resolved

def orderedUnique (platform : Platform) (seen : List Path) : List Path → List Path
  | [] => []
  | path :: rest =>
    if key platform path ∈ seen then orderedUnique platform seen rest
    else path :: orderedUnique platform (key platform path :: seen) rest

theorem unique_is_sublist (platform : Platform) (seen : List Path) (paths : List Path) :
    (orderedUnique platform seen paths).Sublist paths := by
  induction paths generalizing seen with
  | nil => exact .slnil
  | cons head tail ih =>
    by_cases duplicate : key platform head ∈ seen
    · simpa [orderedUnique, duplicate] using List.Sublist.cons head (ih seen)
    · simpa [orderedUnique, duplicate] using List.Sublist.cons_cons head (ih (key platform head :: seen))

theorem unique_avoids_seen (platform : Platform) (seen : List Path) (paths : List Path) :
    ∀ path ∈ orderedUnique platform seen paths, key platform path ∉ seen := by
  induction paths generalizing seen with
  | nil => simp [orderedUnique]
  | cons head tail ih =>
    by_cases duplicate : key platform head ∈ seen
    · simpa [orderedUnique, duplicate] using ih seen
    · intro path member
      simp [orderedUnique, duplicate] at member
      rcases member with rfl | rest
      · exact duplicate
      · have disjoint := ih (key platform head :: seen) path rest
        exact fun old => disjoint (List.mem_cons_of_mem _ old)

theorem unique_keys_are_distinct (platform : Platform) (seen : List Path) (paths : List Path) :
    ((orderedUnique platform seen paths).map (key platform)).Nodup := by
  induction paths generalizing seen with
  | nil => simp [orderedUnique]
  | cons head tail ih =>
    by_cases duplicate : key platform head ∈ seen
    · simpa [orderedUnique, duplicate] using ih seen
    · simp only [orderedUnique, if_neg duplicate, List.map_cons, List.nodup_cons]
      constructor
      · intro member
        obtain ⟨path, inTail, equal⟩ := List.mem_map.mp member
        have disjoint := unique_avoids_seen platform (key platform head :: seen) tail path inTail
        exact disjoint (by simp [equal])
      · exact ih (key platform head :: seen)

def pythonPath (platform : Platform) (name : String) : Bool :=
  match platform with
  | .unix => name == "PYTHONPATH"
  | .windows => name.toLower == "pythonpath"

def orderedPaths (platform : Platform) (tree : Tree) (input : Input) : List Path :=
  let variables := input.inherited.mergeSort (fun left right => left.name ≤ right.name)
  let fromEnvironment := (variables.filter (fun item => pythonPath platform item.name)).flatMap Variable.paths
  input.worker :: input.imports.map (fun path => input.worker ++ (if path = ["."] then [] else path)) ++
    (input.sources ++ fromEnvironment).map (rewrite platform tree input)

def joinable (platform : Platform) (path : Path) : Bool :=
  let forbidden := match platform with | .unix => ':' | .windows => '"'
  !(path.any (fun part => part.toList.contains forbidden))

structure Environment where
  cwd : Path
  searchPath : List Path
  retained : List String
  deriving DecidableEq

def compose (platform : Platform) (paths : List Path) (input : Input) : Option Environment :=
  let searchPath := orderedUnique platform [] paths
  let retained := (input.inherited.filter (fun item => !pythonPath platform item.name)).map Variable.name
  if searchPath.all (joinable platform) then some ⟨input.worker, searchPath, retained.mergeSort (· ≤ ·)⟩ else none

def build (platform : Platform) (tree : Tree) (input : Input) : Option Environment :=
  compose platform (orderedPaths platform tree input) input

theorem outside_paths_keep_their_resolved_location (platform : Platform) (tree : Tree) (input : Input)
    (path : PathInput) (outside : inside platform (resolve tree input.original)
      (resolve tree (if path.absolute then path.parts else input.original ++ path.parts)) = none) :
    rewrite platform tree input path = resolve tree (if path.absolute then path.parts else input.original ++ path.parts) := by
  simp [rewrite, outside]

theorem worker_precedes_all_other_paths (platform : Platform) (tree : Tree) (input : Input)
    (environment : Environment) (built : build platform tree input = some environment) :
    environment.cwd = input.worker ∧ environment.searchPath.head? = some input.worker := by
  simp only [build, compose] at built
  split at built
  · cases built
    simp [orderedPaths, orderedUnique]
  · contradiction

end HoiminOracle.WorkerEnvironment
