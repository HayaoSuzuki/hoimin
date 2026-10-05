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
  Json.mkObj [("schema", toJson (2 : Nat)), ("id", toJson id), ("kind", toJson kind),
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

-- Fuel is the count of user classes, as in the Rust MAX_STEPS loop.
def chainGraph (count : Nat) : Graph := fun id =>
  if id.owner == 1 && id.member < count then
    some ⟨if id.member == 0 then exceptionId else ⟨1, id.member - 1⟩, true⟩
  else none

def depthRows : List Json := [254, 255, 256, 257].flatMap fun count =>
  [false, true].map fun raised =>
    let source := String.join ((List.range count).map fun n =>
      definition s!"C{n}" (if n == 0 then "Exception" else s!"C{n - 1}")) ++
      "def target():\n    " ++
      (if raised then s!"raise C{count - 1}()\n" else
        s!"try:\n        pass\n    except C{count - 1}:\n        pass\n")
    strictRow s!"depth-{count}-{raised}" [("service.py", source)]
      (if eligible (chainGraph count) ⟨1, count - 1⟩ ⟨1, count - 2⟩ 0 1 raised true then
        [(s!"C{count - 1}", s!"C{count - 2}")] else [])

def collisionImports : List (String × String × String) := [
  ("absent", "", ""),
  ("aliased", "import pkg.Child as loaded\n", ""),
  ("from", "from pkg.Child import marker\n", ""),
  ("nested-import", "def load():\n    import pkg.Child as loaded\nload()\n", ""),
  ("nested-from", "def load():\n    from pkg.Child import marker\nload()\n", ""),
  ("relative", "import pkg.loader\n", "from .Child import marker\n"),
  ("nested-relative", "import pkg.loader\npkg.loader.load()\n", "def load():\n    from .Child import marker\n")]

def collisionGlobals (loaded : Bool) : Nat → Option ClassId := fun key =>
  if loaded && key == 2 then none else globals key

def collisionRows : List Json := collisionImports.map fun (name, imports, helper) =>
  strictRow ("namespace-" ++ name) [
    ("pkg/__init__.py", definition "Root" "Exception" ++ definition "Child" "Root"),
    ("pkg/Child.py", "marker = 1\n"), ("pkg/loader.py", helper),
    ("service.py", "import pkg\n" ++ imports ++ "def target():\n    raise pkg.Root()\n")]
    (if namedEligible (graph true) (collisionGlobals (name != "absent")) (locals 0) 1 2 0 1 true true
      then [("pkg.Root", "pkg.Child")] else [])

def attributeWrites : List (String × String × Option Nat) := [
  ("unchanged", "import pkg.errors as e\n", none),
  ("unrelated", "import pkg.other as e\ne.Child = object\n", some 2),
  ("assignment", "import pkg.errors as e\ne.Child = object\n", some 1),
  ("nested", "def patch():\n    import pkg.errors as e\n    e.Child = object\npatch()\n", some 1),
  ("private", "class P:\n    def patch(self):\n        import pkg.errors as __e\n        __e.Child = object\nP().patch()\n", some 1),
  ("setattr", "import pkg.errors as e\nsetattr(e, 'Child', object)\n", some 1),
  ("delete", "import pkg.errors as e\ndel e.Child\n", some 1),
  ("delattr", "import pkg.errors as e\ndelattr(e, 'Child')\n", some 1),
  ("relative", "from pkg import errors as unused\nfrom . import errors as e\ne.Child = object\n", some 1),
  ("constructor", "from .errors import Child as e\ne.__init__ = lambda self, *args: None\n", some 1),
  ("dotted", "import pkg.errors\npkg.errors.Child = object\n", some 1)]

def writeAliases (owner : Option Nat) (name : Nat) : Option Nat :=
  if name == 10 then owner else if name == 20 then some 1 else none

def attributeRows : List Json := attributeWrites.map fun (name, patch, owner) =>
  let trusted := providerTrusted (writeAliases owner) (if owner.isSome then [10] else []) rootId.owner
  strictRow ("attribute-" ++ name) [
    ("pkg/__init__.py", ""),
    ("pkg/errors.py", definition "Root" "Exception" ++ definition "Child" "Root"),
    ("pkg/other.py", definition "Root" "Exception" ++ definition "Child" "Root"),
    ("pkg/patcher.py", patch),
    ("service.py", "import pkg.patcher\nimport pkg.errors as e\nfrom pkg.errors import Root\ndef target():\n    raise Root()\n")]
    (if eligible (graph true) rootId childId 0 1 true trusted then [("Root", "e.Child")] else [])

structure AliasCase where
  name : String
  source : String
  imports : List (ScopedName × String) := [((0, "e"), "errors")]
  edges : AliasEdges := []
  written : List ScopedName := [(0, "other")]

def aliasCases : List AliasCase := [
  { name := "unchanged", source := "import errors as e\nother = e\n",
    edges := [((0, "other"), (0, "e"))], written := [] },
  { name := "direct", source := "import errors as e\ne.Root = object\n", written := [(0, "e")] },
  { name := "assignment", source := "import errors as e\nother = e\nother.Root = object\n",
    edges := [((0, "other"), (0, "e"))] },
  { name := "chain", source := "import errors as e\nfirst = e\nother = first\nother.Root = object\n",
    edges := [((0, "first"), (0, "e")), ((0, "other"), (0, "first"))] },
  { name := "reverse", source := "import errors as e\ndef patch():\n    other = first\n    other.Root = object\nfirst = e\npatch()\n",
    edges := [((1, "other"), (0, "first")), ((0, "first"), (0, "e"))], written := [(1, "other")] },
  { name := "cycle", source := "import errors as e\nfirst = e\nother = first\nfirst = other\nother.Root = object\n",
    edges := [((0, "first"), (0, "e")), ((0, "other"), (0, "first")), ((0, "first"), (0, "other"))] },
  { name := "duplicate", source := "import errors as e\nother = e\nother = e\nother.Root = object\n",
    edges := [((0, "other"), (0, "e")), ((0, "other"), (0, "e"))] },
  { name := "rebind", source := "import errors as e\nimport unrelated as foreign\nother = e\ne = foreign\nother.Root = object\n",
    imports := [((0, "e"), "errors"), ((0, "foreign"), "unrelated")], edges := [((0, "other"), (0, "e")), ((0, "e"), (0, "foreign"))] },
  { name := "unrelated", source := "import unrelated as e\nother = e\nother.Root = object\n",
    imports := [((0, "e"), "unrelated")], edges := [((0, "other"), (0, "e"))] },
  { name := "annotated", source := "import errors as e\nother: object = e\nother.Root = object\n",
    edges := [((0, "other"), (0, "e"))] },
  { name := "chained", source := "import errors as e\nfirst = other = e\nother.Root = object\n",
    edges := [((0, "first"), (0, "e")), ((0, "other"), (0, "e"))] },
  { name := "walrus", source := "import errors as e\nif (other := e):\n    other.Root = object\n",
    edges := [((0, "other"), (0, "e"))] },
  { name := "private", source := "class P:\n    def patch(self):\n        import errors as __e\n        __other = __e\n        __other.Root = object\nP().patch()\n",
    imports := [((2, "_P__e"), "errors")], edges := [((2, "_P__other"), (2, "_P__e"))],
    written := [(2, "_P__other")] },
  { name := "nested", source := "def patch():\n    import errors as e\n    other = e\n    other.Root = object\npatch()\n",
    imports := [((1, "e"), "errors")], edges := [((1, "other"), (1, "e"))], written := [(1, "other")] },
  { name := "attribute", source := "import errors as e\nother = e.Root\nother.__init__ = lambda self, *args: None\n",
    edges := [((0, "other"), (0, "e"))] },
  { name := "delete", source := "import errors as e\nother = e\ndel other.Root\n",
    edges := [((0, "other"), (0, "e"))] },
  { name := "setattr", source := "import errors as e\nother = e\nsetattr(other, 'Root', object)\n",
    edges := [((0, "other"), (0, "e"))] }]

def scopeCases : List AliasCase := [
  { name := "scope-parameter", source := "import errors as e\ndef patch(e):\n    other = e\n    other.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [((1, "other"), (1, "e"))], written := [(1, "other")] },
  { name := "scope-renamed-parameter", source := "import errors as e\ndef patch(value):\n    other = value\n    other.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [((1, "other"), (1, "value"))], written := [(1, "other")] },
  { name := "scope-local-import", source := "import errors as e\ndef patch():\n    import unrelated as e\n    e.Root = object\n",
    imports := [((0, "e"), "errors"), ((1, "e"), "unrelated")], edges := [], written := [(1, "e")] },
  { name := "scope-local-errors-import", source := "import unrelated as e\ndef patch():\n    import errors as e\n    e.Root = object\n",
    imports := [((0, "e"), "unrelated"), ((1, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-free", source := "import errors as e\ndef patch():\n    other = e\n    other.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [((1, "other"), (0, "e"))], written := [(1, "other")] },
  { name := "scope-global", source := "import errors as e\ndef patch():\n    global e\n    other = e\n    other.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [((1, "other"), (0, "e"))], written := [(1, "other")] },
  { name := "scope-closure-parameter", source := "import errors as e\ndef outer(e):\n    def patch():\n        e.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-closure-import", source := "def outer():\n    import errors as e\n    def patch():\n        e.Root = object\n",
    imports := [((1, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-nonlocal", source := "def outer():\n    import errors as e\n    def patch():\n        nonlocal e\n        e.Root = object\n",
    imports := [((1, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-late-nonlocal", source := "def outer():\n    def patch():\n        nonlocal e\n        e.Root = object\n    import errors as e\n",
    imports := [((1, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-late-local-shadow", source := "import errors as e\ndef patch():\n    e.Root = object\n    e = object()\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-method-skips-class", source := "import errors as e\nclass C:\n    e = object()\n    def patch(self):\n        e.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(0, "e")] },
  { name := "scope-method-closure", source := "import errors as e\ndef outer(e):\n    class C:\n        e = object()\n        def patch(self):\n            e.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-class-read-fallback", source := "import errors as e\nclass C:\n    other = e\n    e = object()\n    other.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [((1, "other"), (1, "e")), ((1, "other"), (0, "e"))], written := [(1, "other"), (0, "other")] },
  { name := "scope-private-parameter", source := "import errors as _P__e\nclass P:\n    def patch(self, __e):\n        other = __e\n        other.Root = object\n",
    imports := [((0, "_P__e"), "errors")], edges := [((2, "other"), (2, "_P__e"))], written := [(2, "other")] },
  { name := "scope-default-in-outer", source := "import errors as e\ndef patch(e=(other := e)):\n    other.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [((0, "other"), (0, "e"))], written := [(0, "other")] },
  { name := "scope-lambda-parameter", source := "import errors as e\nf = lambda e: setattr(e, 'Root', object)\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-lambda-free", source := "import errors as e\nf = lambda: setattr(e, 'Root', object)\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(0, "e")] },
  { name := "scope-comprehension-target", source := "import errors as e\nvalues = [setattr(e, 'Root', object) for e in ()]\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(1, "e")] },
  { name := "scope-comprehension-free", source := "import errors as e\nvalues = [setattr(e, 'Root', object) for value in ()]\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(0, "e")] },
  { name := "scope-comprehension-first-iterable", source := "import errors as e\nvalues = [e for e in [setattr(e, 'Root', object)]]\n",
    imports := [((0, "e"), "errors")], edges := [], written := [(0, "e")] },
  { name := "scope-comprehension-walrus", source := "import errors as e\nvalues = [(other := e) for value in (0,)]\nother.Root = object\n",
    imports := [((0, "e"), "errors")], edges := [((0, "other"), (0, "e"))], written := [(0, "other")] }]

-- Public candidate controls for the separate may-bind scan, not only alias extraction.
def implicitScopeRows : List Json := [
  "values = [Root for Root in ()]\ndef target():\n    raise Child()\n",
  "def target():\n    values = [Root for Root in ()]\n    raise Child()\n",
  "value = lambda: (Root := object())\ndef target():\n    raise Child()\n",
  "def target():\n    value = lambda: (Root := object())\n    raise Child()\n"
].zipIdx |>.map fun (body, index) =>
  strictRow s!"implicit-scope-{index}"
    [("service.py", definition "Root" "Exception" ++ definition "Child" "Root" ++ body)]
    (if eligible (graph true) childId rootId 0 1 true true then [("Child", "Root")] else [])

def aliasFiles (c : AliasCase) : List (String × String) := [
  ("errors.py", definition "Root" "Exception" ++ definition "Child" "Root"),
  ("unrelated.py", definition "Root" "Exception"), ("patcher.py", c.source),
  ("service.py", "from errors import Root, Child\ndef target():\n    raise Child()\n")]

def aliasRows : List Json := (aliasCases ++ scopeCases).map fun c =>
  strictRow ("alias-" ++ c.name) (aliasFiles c)
    (if eligible (graph true) childId rootId 0 1 true (aliasTrusted c.edges c.written c.imports "errors")
      then [("Child", "Root")] else [])

def aliasExtractionRows : List Json := (aliasCases ++ scopeCases).map fun c =>
  let facts := Json.mkObj [
    ("imports", toJson (c.imports.map fun (name, origin) =>
      Json.arr #[toJson name, toJson origin, toJson (0 : Nat)])),
    ("assignments", toJson c.edges.eraseDups), ("writes", toJson c.written.eraseDups),
    ("affected", toJson (writeClosure c.edges.length c.edges c.written))]
  let base := row ("alias-extraction-" ++ c.name) "alias-extraction" "internal-fixture"
    (aliasFiles c) (observation [])
  match base with
  | .obj fields => .obj (fields.insert "expected_alias_facts" facts)
  | _ => base

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

def rows := candidateRows ++ loadRows ++ relativeRows ++ reservedRows ++ resourceRows ++ depthRows ++ collisionRows ++ attributeRows ++ snapshotRows ++ aliasRows ++ implicitScopeRows ++ aliasExtractionRows
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
    ("reject-all", healthy),
    ("builtin-consumes-step", ancestry 256 (chainGraph 256) ⟨1, 255⟩ && constructible 256 (chainGraph 256) ⟨1, 255⟩ &&
      !(ancestry 255 (chainGraph 256) ⟨1, 255⟩ && constructible 255 (chainGraph 256) ⟨1, 255⟩)),
    ("namespace-overwrite", namedEligible (graph true) globals (locals 0) 1 2 0 1 true true !=
      namedEligible (graph true) (collisionGlobals true) (locals 0) 1 2 0 1 true true),
    ("write-only-invalidates-spelling", providerTrusted (writeAliases (some 1)) [10] 1 != !([10].contains (20 : Nat)))]
  for (name, detected) in checks do
    unless detected do throw (IO.userError ("undetected " ++ name))
  let edges : AliasEdges := [((0, "first"), (0, "e")), ((0, "other"), (0, "first"))]
  let written : List ScopedName := [(0, "other")]
  let imports : List (ScopedName × String) := [((0, "e"), "errors")]
  let trusted := aliasTrusted edges written imports "errors"
  let ignoresAliases := !imports.any (fun (name, _) => written.contains name)
  let onePass := !imports.any (fun (name, _) => (expandWrites edges written).contains name)
  let reversed := aliasTrusted (edges.map fun (target, source) => (source, target)) written imports "errors"
  for (name, broken) in [("ignore-assignment", ignoresAliases), ("single-pass", onePass), ("reverse-edge", reversed)] do
    unless trusted != broken do throw (IO.userError ("undetected " ++ name))
  let separated :=  aliasTrusted [] [(1, "e")] [((0, "e"), "errors")] "errors"
  let flattened := aliasTrusted [] [(0, "e")] [((0, "e"), "errors")] "errors"
  unless separated != flattened do throw (IO.userError "undetected flattened-scope")
  IO.println s!"alias domain: cases={aliasCases.length + scopeCases.length} max_edges=3; fixed-point checked before trust; three propagation variants detected"
  IO.println s!"sensitivity: 18 broken variants detected; strict={candidateRows.length + loadRows.length + relativeRows.length + reservedRows.length + resourceRows.length + depthRows.length + collisionRows.length + attributeRows.length + aliasRows.length + implicitScopeRows.length} internal-fixture={snapshotRows.length + aliasExtractionRows.length}"

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
