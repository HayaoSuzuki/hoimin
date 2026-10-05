import HoiminOracle.ExceptionHierarchyProofs
import Lean.Data.Json

open HoiminOracle.ExceptionHierarchy Lean

namespace ExceptionHierarchyAudit

def rootId : ClassId := ⟨1, 0⟩
def childId : ClassId := ⟨1, 1⟩
def siblingId : ClassId := ⟨1, 2⟩
def graph (linked : Bool) (custom : Nat := 0) : Graph := fun id =>
  if id == rootId then some ⟨exceptionId, custom != 2⟩
  else if id == childId then some ⟨if linked then rootId else valueErrorId, custom != 1⟩
  else if id == siblingId then some ⟨rootId, true⟩ else none

def globals : Nat → Option ClassId
  | 1 => some rootId | 2 => some childId | 3 => some siblingId | _ => none

def locals (scope : Nat) (key : Nat) : Option (Option ClassId) :=
  if (scope == 1 || scope == 3) && key == 2 || scope == 2 && key == 1 then some none else none

def observation (pairs : List (String × String)) (error truncated : Option Bool := none)
    (load : Option String := none) (fingerprint : Option Bool := none) : Json :=
  Json.mkObj [("pairs", toJson pairs), ("error", toJson error), ("truncated", toJson truncated),
    ("load", toJson load), ("fingerprint_matches", toJson fingerprint)]

def row (id kind mode : String) (files : List (String × String)) (expected : Json)
    (roots : List String := []) (actions : List String := []) (changed : String := "")
    (maxCandidates : Nat := 100) : Json :=
  Json.mkObj [("schema", toJson (1 : Nat)), ("id", toJson id), ("kind", toJson kind),
    ("mode", toJson mode), ("files", toJson files), ("roots", toJson roots),
    ("actions", toJson actions), ("changed_source", toJson changed),
    ("max_candidates", toJson maxCandidates), ("expected", expected)]

def strictRow (id : String) (files : List (String × String)) (pairs : List (String × String))
    (roots : List String := []) : Json :=
  row id "candidate" "strict" files (observation pairs (some false) (some false)) roots

def definition (name parent : String) (custom : Bool := false) : String :=
  "class " ++ name ++ "(" ++ parent ++ "):" ++
    (if custom then "\n    def __init__(self, *args): pass\n" else " pass\n")

def site (source : String) (kind : Nat) : String :=
  if kind == 0 then "raise " ++ source ++ "()\n"
  else "try:\n    pass\nexcept" ++ (if kind == 2 then "* " else " ") ++ source ++ ":\n    pass\n"

def candidateRows : List Json := [false, true].flatMap fun linked =>
  (List.range 3).flatMap fun custom => (List.range 3).flatMap fun kind =>
    (List.range 4).map fun scope =>
      let sourceName := if scope == 3 then "_Service__Child" else "Child"
      let definitions :=  definition "Root" "Exception" (custom == 2) ++
        definition sourceName (if linked then "Root" else "ValueError") (custom == 1) ++
        definition "Sibling" "Root"
      let body := site sourceName kind
      let function := if scope == 3 then
          "class Service:\n    def target(self, __Child):\n" ++
          String.join ((body.trimAsciiEnd.toString.splitOn "\n").map fun line => "        " ++ line ++ "\n")
        else "def target(" ++ (if scope == 1 then "Child" else if scope == 2 then "Root" else "") ++
          "):\n" ++ String.join ((body.trimAsciiEnd.toString.splitOn "\n").map fun line => "    " ++ line ++ "\n")
      let pairs := [(1, "Root"), (3, "Sibling")].filterMap fun (key, name) =>
        if namedEligible (graph linked custom) globals (locals scope) 2 key 0 1 (kind == 0) true
        then some (sourceName, name) else none
      strictRow s!"candidate-{linked}-{custom}-{kind}-{scope}" [("service.py", definitions ++ function)] pairs

def loadRows : List Json := [false, true].map fun early =>
  let imp := "import pkg.errors as loaded\n"
  let source := "import pkg as p\nfrom pkg import Root\n" ++ (if early then imp else "") ++
    "def target():\n    raise Root()\n" ++ (if early then "" else imp)
  strictRow s!"load-{early}" [
    ("pkg/__init__.py", definition "Root" "Exception"),
    ("pkg/errors.py", "from pkg import Root\n" ++ definition "Child" "Root"),
    ("service.py", source)]
    (if eligible (graph true) rootId childId (if early then 0 else 2) 1 true true
      then [("Root", "loaded.Child")] else [])

def relativeGraph (same : Bool) : Graph := fun id =>
    if id == childId then some ⟨⟨if same then 1 else 2, 0⟩, true⟩
    else if id == rootId then some ⟨exceptionId, true⟩
    else if id == ⟨2, 0⟩ then some ⟨valueErrorId, true⟩ else none

def relativeRows : List Json := [false, true].map fun same =>
  strictRow s!"relative-{same}" ((if same then [("lib/pkg/__init__.py", "")] else
    [("pkg/base.py", definition "Root" "ValueError")]) ++ [
    ("lib/pkg/base.py", definition "Root" "Exception"),
    ("lib/pkg/errors.py", "from .base import Root\n" ++ definition "Child" "Root"),
    ("service.py", "from pkg.errors import Child\nfrom " ++
      (if same then "pkg" else "lib.pkg") ++ ".base import Root\ndef target():\n    raise Child()\n")])
    (if eligible (relativeGraph same) childId rootId 0 1 true true then [("Child", "Root")] else []) ["lib"]

def reservedRows : List Json := ["sys", "os", "__main__"].map fun name =>
  strictRow ("reserved-" ++ name) [
    (name ++ ".py", definition "Root" "Exception" ++ definition "Child" "Root"),
    ("service.py", "import " ++ name ++ " as e\ndef target():\n    raise e.Child()\n")]
    (if eligible (graph true) childId rootId 0 1 true false then [("e.Child", "e.Root")] else [])

def resourceRows : List Json := [254, 255, 256].map fun aliases =>
  let names := (List.range 255).map fun n => s!"Child{n}"
  let definitions := definition "Root" "Exception" ++ String.join (names.map (definition · "Root"))
  let source := String.join ((List.range aliases).map fun n => s!"import errors as e{n}\n") ++
    "def target():\n    raise e0.Child0()\n"
  let retained := 256 * (aliases + 1)
  let accepted := (reserve 65536 (retained - 1)).isSome
  let destinations := (names.filter (· != "Child0") ++ ["Root"]).mergeSort (· ≤ ·)
  let pairs := if accepted then (destinations.take 1).map fun n => ("e0.Child0", "e0." ++ n) else []
  row s!"resource-{retained}" "resource" "strict" [("errors.py", definitions), ("service.py", source)]
    (observation pairs (some (!accepted)) (if accepted then some true else none)) [] [] "" 1

def events : List Event := [.change, .delete, .restore, .build]
def traces : Nat → List (List Event)
  | 0 => [[]]
  | n + 1 => events.flatMap fun event => (traces n).map (event :: ·)
def domain (depth : Nat) : List (List Event) := (List.range (depth + 1)).flatMap traces
def eventName : Event → String
  | .change => "change" | .delete => "delete" | .restore => "restore" | .build => "build"
def loadName : Load → String
  | .never => "never" | .ok => "ok" | .error => "error"
def errorsSource (linked : Bool) : String :=
  definition "Root" "Exception" ++ definition "Child" (if linked then "Root" else "ValueError")

def snapshotRows : List Json := [false, true].flatMap fun base => (domain 3).mapIdx fun n es =>
  let state := run base initial es
  row s!"snapshot-{base}-{n}" "snapshot" "internal-fixture" [
    ("errors.py", errorsSource base), ("lib/errors.py", errorsSource true),
    ("service.py", "from errors import Root, Child\ndef target():\n    raise Child()\n")]
    (observation (if candidate state then [("Child", "Root")] else []) none none
      (some (loadName state.lastLoad)) (some (fingerprintMatches state))) ["lib"]
    (es.map eventName) (errorsSource (!base))

def rows := candidateRows ++ loadRows ++ relativeRows ++ reservedRows ++ resourceRows ++ snapshotRows
def corpus := String.join (rows.map fun r => r.compress ++ "\n")

-- Deliberately broken variants stay in the executable, never in imported proof modules.
def brokenStep (variant : Nat) (base : Bool) (state : State) (event : Event) : State :=
  if event == .build && !state.cache.isSome && variant == 0 && state.current == .changed then
    { state with cache := some ⟨0, 1, !base⟩, lastLoad := .ok }
  else if event == .build && !state.cache.isSome && variant == 1 && state.current == .missing then
    { state with cache := some ⟨1, 1, true⟩, lastLoad := .ok }
  else if event == .build && variant == 2 then step base { state with cache := none } event
  else step base state event

def brokenRun (variant : Nat) (base : Bool) (es : List Event) : State :=
  es.foldl (brokenStep variant base) initial

def usable (state : State) : Bool := fingerprintMatches state && candidate state

def checkSensitivity : IO Unit := do
  for depth in List.range 4 do
    let start ← IO.monoMsNow
    let all := domain depth
    let changed := all.filter fun es => usable (brokenRun 0 false es) != usable (run false initial es)
    let missing := all.filter fun es => usable (brokenRun 1 false es) != usable (run false initial es)
    IO.println s!"depth={depth} alphabet=4 traces={all.length} events={all.foldl (fun n es => n + es.length) 0} changed={changed.length} missing={missing.length} changed_first={repr (changed.head?)} missing_first={repr (missing.head?)} elapsed_ms={(← IO.monoMsNow) - start}"
  for variant in [0, 1] do
    unless (domain 3).any (fun es => usable (brokenRun variant false es) != usable (run false initial es)) do
      throw (IO.userError s!"undetected snapshot variant {variant}")
  unless (domain 3).any (fun es => candidate (brokenRun 2 true es) != candidate (run true initial es)) do
    throw (IO.userError "undetected cache reload")
  let healthy := namedEligible (graph true) globals (locals 0) 2 1 0 1 true true
  let checks := [
    ("opaque-local", healthy != namedEligible (graph true) globals (locals 1) 2 1 0 1 true true),
    ("private-local", healthy != namedEligible (graph true) globals (locals 3) 2 1 0 1 true true),
    ("late-load", healthy != eligible (graph true) childId rootId 2 1 true true),
    ("module-identity", eligible (relativeGraph false) childId rootId 0 1 true true !=
      eligible (relativeGraph true) childId rootId 0 1 true true),
    ("reserved-origin", healthy != eligible (graph true) childId rootId 0 1 true false),
    ("constructor", healthy != eligible (graph true 1) childId rootId 0 1 true true),
    ("inclusive-limit", (reserve 2 2).isSome != decide (2 ≤ 2)),
    ("reject-all", healthy)]
  for (name, detected) in checks do
    unless detected do throw (IO.userError ("undetected " ++ name))
  IO.println s!"sensitivity: 11 broken variants detected; strict={candidateRows.length + loadRows.length + relativeRows.length + reservedRows.length + resourceRows.length} internal-fixture={snapshotRows.length}"

def main (args : List String) : IO Unit := do
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path corpus
  | ["--check", path] => unless (← IO.FS.readFile path) == corpus do throw (IO.userError "stale corpus")
  | ["--sensitivity"] => checkSensitivity
  | ["--assert-old-safe"] =>
      match (domain 3).find? (fun es => usable (brokenRun 0 false es)) with
      | some es => throw (IO.userError s!"refuted old current-only check: {repr es}")
      | none => pure ()
  | _ => throw (IO.userError "usage: --output PATH | --check PATH | --sensitivity | --assert-old-safe")

end ExceptionHierarchyAudit

def main := ExceptionHierarchyAudit.main
