import HoiminOracle.ComprehensionBindingModel
import Lean.Data.Json
open HoiminOracle.BindingFlow HoiminOracle.ComprehensionBinding
structure Case where
  id : String
  source : String
  path : List Frame
  name : Name := .source
  dropFrames : Nat := 1
  iteration : Bool := false
  observation : Option Bool := none -- some true: first iterable; some false: later sibling
def function : Frame := functionFrame 2 emptyEnv
def cases : List Case := [
  { id := "module_source", path := [comp, module]
    source := "custom = lambda values: 'custom'\n[(any := custom) for _ in [0]]\nassert any is custom\nresult = any([False, True])\n" },
  { id := "module_destination", path := [comp, module], name := .destination
    source := "custom = lambda values: 'custom'\n[(all := custom) for _ in [0]]\nassert all is custom\nresult = any([False, True])\n" },
  { id := "nested", path := [{ comp with id := 3 }, comp, module], dropFrames := 2
    source := "custom = lambda values: 'custom'\n[[(any := custom) for _ in [0]] for _ in [0]]\nassert any is custom\nresult = any([False, True])\n" },
  { id := "empty_module", path := [comp, module]
    source := "import builtins\n[(any := 0) for _ in []]\nassert any is builtins.any\nresult = any([False, True])\n" },
  { id := "lazy_module", path := [comp, module]
    source := "import builtins\ng = ((any := 0) for _ in [0])\nassert any is builtins.any\nresult = any([False, True])\n" },
  { id := "static_function", path := [comp, function, module]
    source := "def f():\n    result = any([False, True])\n    [(any := 0) for _ in []]\ntry:\n    f()\nexcept UnboundLocalError:\n    pass\nelse:\n    raise AssertionError('expected static local')\n" },
  { id := "global", path := [comp, { function with directive := .global }, module], dropFrames := 2
    source := "custom = lambda values: 'custom'\ndef f():\n    global any\n    [(any := custom) for _ in [0]]\nf()\nassert any is custom\nresult = any([False, True])\n" },
  { id := "nonlocal", path := [comp, { function with directive := .nonlocal }, functionFrame 3 (emptyEnv.set .source .shadowed), module]
    source := "def outer():\n    any = lambda values: 'old'\n    def inner():\n        nonlocal any\n        [(any := lambda values: 'new') for _ in [0]]\n        return any([False, True])\n    assert inner() == 'new'\nouter()\n" },
  { id := "lambda_boundary", path := [function, comp, module], dropFrames := 2
    source := "[(lambda: (any := 0))() for _ in [0]]\nassert any([False, True]) is True\n" },
  { id := "function_boundary", path := [comp, function, module], dropFrames := 2
    source := "def f():\n    [(any := 0) for _ in [0]]\nf()\nassert any([False, True]) is True\n" },
  { id := "iteration_local", path := [comp, module], iteration := true
    source := "[any for any in [0]]\nassert any([False, True]) is True\n" },
  { id := "first_list_source", path := [comp, module], observation := some true, name := .source
    source := "values = [(any := 0, item)[1] for item in [any((0, 1))]]\nassert next(iter(values)) is True\n" },
  { id := "first_list_destination", path := [comp, module], observation := some true, name := .destination
    source := "values = [(all := 0, item)[1] for item in [any((0, 1))]]\nassert next(iter(values)) is True\n" },
  { id := "first_set_destination", path := [comp, module], observation := some true, name := .destination
    source := "values = {(all := 0, item)[1] for item in [any((0, 1))]}\nassert next(iter(values)) is True\n" },
  { id := "first_dict_destination", path := [comp, module], observation := some true, name := .destination
    source := "values = {item: (all := 0) for item in [any((0, 1))]}\nassert next(iter(values)) is True\n" },
  { id := "first_generator_destination", path := [comp, module], observation := some true, name := .destination
    source := "values = ((all := 0, item)[1] for item in [any((0, 1))])\nassert next(iter(values)) is True\n" },
  { id := "first_static_function", path := [comp, function, module], observation := some true
    source := "def f():\n    return [(any := 0) for item in [any((0, 1))]]\ntry:\n    f()\nexcept UnboundLocalError:\n    pass\nelse:\n    raise AssertionError('expected static local')\n" },
  { id := "post_sibling", path := [comp, module], observation := some false, name := .destination
    source := "values = ([(all := 0) for _ in range(1)], any((0, 1)))\n" }
]
def owner (item : Case) : Option Nat :=
  if item.iteration then item.path.head?.map Frame.id else destination item.name item.path
def summaryFor (item : Case) (first : Bool) : Summary :=
  let writes := [{ name := item.name, owner := owner item }]
  if first then .comprehension (.observe item.dropFrames) writes
  else .sequence (.comprehension .empty writes) (.observe item.dropFrames)
def expected (item : Case) : Bool :=
  match item.observation with
  | none => allows ((writeAt item.name (owner item) item.path).drop item.dropFrames)
  | some first => (evaluate item.path (summaryFor item first)).headD false
def render : String := String.join (cases.map fun item =>
  (Lean.Json.mkObj [
    ("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"),
    ("id", .str item.id), ("source", .str item.source),
    ("owner", Lean.toJson (owner item)), ("expected_present", Lean.toJson (expected item))
  ]).compress ++ "\n")
-- Deliberate broken variants, kept outside production and imported proofs.
def brokenIgnoreDirective (path : List Frame) : Option Nat :=
  (containing path).head?.map Frame.id

def brokenCrossFunction (path : List Frame) : Option Nat :=
  (path.find? (fun frame => frame.kind == .module)).map Frame.id

def routingSensitivity : Bool :=
  let globalPath := [comp, { function with directive := .global }, module]
  let nonlocalPath := [comp, { function with directive := .nonlocal },
    functionFrame 3 (emptyEnv.set .source .shadowed), module]
  let lambdaPath := [function, comp, module]
  let staticPath := [comp, function, module]
  allows ((brokenCurrentWrite .source [comp, module]).drop 1) &&
  !allows ((namedWrite .source [comp, module]).drop 1) &&
  brokenIgnoreDirective globalPath != destination .source globalPath &&
  brokenIgnoreDirective nonlocalPath != destination .source nonlocalPath &&
  allows ((writeAt .source (brokenIgnoreDirective globalPath) globalPath).drop 2) &&
  !allows ((namedWrite .source globalPath).drop 2) &&
  brokenCrossFunction lambdaPath != destination .source lambdaPath &&
  !allows ((writeAt .source (brokenCrossFunction lambdaPath) lambdaPath).drop 2) &&
  allows ((namedWrite .source lambdaPath).drop 2) &&
  -- Broken zero-iteration rule: omit the static declaration altogether.
  allows (staticPath.drop 1) && !allows ((namedWrite .source staticPath).drop 1)
-- Broken execution modes: early publication, omitted lazy/empty publication,
-- and omitted static declaration. Compare each to the same source row.
def orderSensitivity : Bool :=
  let firstRows := cases.filter (fun item => item.observation == some true)
  let earlyDetected := firstRows.any fun item =>
    expected item != allows ((writeAt item.name (owner item) item.path).drop item.dropFrames)
  let noStaticDetected := firstRows.any fun item =>
    expected item != (interpret item.path (summaryFor item true)).1.headD false
  let noPublicationDetected := cases.any fun item =>
    item.observation == some false && expected item != allows (item.path.drop item.dropFrames)
  let lazyDetected := cases.any fun item =>
    item.id == "lazy_module" && expected item != allows (item.path.drop item.dropFrames)
  let emptyDetected := cases.any fun item =>
    item.id == "empty_module" && expected item != allows (item.path.drop item.dropFrames)
  earlyDetected && noStaticDetected && noPublicationDetected && lazyDetected && emptyDetected

def sensitivity : Bool := routingSensitivity && orderSensitivity
def main (args : List String) : IO UInt32 := do
  unless sensitivity do
    IO.eprintln "named-binding routing sensitivity failed"
    return 2
  let args := if args.head? == some "--" then args.drop 1 else args
  match args with
  | ["--output", path] => IO.FS.writeFile path render; return 0
  | ["--check", path] =>
      if (← IO.FS.readFile path) == render then return 0 else return 1
  | ["--stats"] | ["--sensitivity"] =>
      IO.println s!"cases={cases.length} max_frames=4 sensitivity={sensitivity}"
      return 0
  | _ => IO.eprintln "use --output PATH, --check PATH, or --stats"; return 2
