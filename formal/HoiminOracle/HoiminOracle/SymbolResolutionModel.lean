import HoiminOracle.PathOrderModel

namespace HoiminOracle.SymbolResolution

structure File where
  path : String
  isPython : Bool := true
  definitions : List String := []

structure Selector where
  module : String
  qualname : String

abbrev Binding := String × String

structure Target where
  path : String
  symbols : List String

def qualified (source relative : String) : String :=
  if source.isEmpty then relative else source ++ "/" ++ relative

def moduleFile (source module : String) : String :=
  qualified source (module.replace "." "/" ++ ".py")

def packageFile (source module : String) : String :=
  qualified source (module.replace "." "/" ++ "/__init__.py")

def searchPaths (module : String) (sources : List String) : List String :=
  sources.flatMap fun source => [moduleFile source module, packageFile source module]

def available (files : List File) (path : String) : Bool :=
  files.any fun file => file.isPython && file.path == path

def resolveModule (sources : List String) (files : List File) (module : String) : Option String :=
  (searchPaths module sources).find? (available files)

def bindSelectors (findModule : String → Option String) : List Selector → Except String (List Binding)
  | [] => .ok []
  | selector :: rest =>
    match findModule selector.module with
    | none => .error selector.module
    | some path => (bindSelectors findModule rest).map ((path, selector.qualname) :: ·)

def containsPath (source path : String) : Bool :=
  source.isEmpty || path == source || path.startsWith (source ++ "/")

def attachSymbols (sources : List String) (files : List File) (bindings : List Binding) : List Target :=
  let paths := (files.filter fun file => file.isPython && sources.any (containsPath · file.path)).map (·.path)
  paths.eraseDups.mergeSort PathOrder.le |>.map fun path =>
    ⟨path, (bindings.filterMap fun (assigned, qualname) => if assigned == path then some qualname else none)
      |>.eraseDups.mergeSort (· ≤ ·)⟩

def resolveWith (findModule : String → Option String) (sources : List String) (files : List File)
    (selectors : List Selector) : Except String (List Target) :=
  (bindSelectors findModule selectors).map (attachSymbols sources files)

def resolve (sources : List String) (files : List File) (selectors : List Selector) : Except String (List Target) :=
  resolveWith (resolveModule sources files) sources files selectors

def firstMissingDefinition (files : List File) (targets : List Target) : Option Binding :=
  targets.findSome? fun target =>
    let definitions := ((files.find? (·.path == target.path)).map (·.definitions)).getD []
    (target.symbols.find? fun name => !definitions.contains name).map (target.path, ·)

theorem module_file_precedes_package (source module : String) (others : List String) (files : List File)
    (present : available files (moduleFile source module) = true) :
    resolveModule (source :: others) files module = some (moduleFile source module) := by
  simp [resolveModule, searchPaths, present]

theorem package_precedes_later_sources (source module : String) (others : List String) (files : List File)
    (noModule : available files (moduleFile source module) = false)
    (hasPackage : available files (packageFile source module) = true) :
    resolveModule (source :: others) files module = some (packageFile source module) := by
  simp [resolveModule, searchPaths, noModule, hasPackage]

theorem unavailable_source_is_skipped (source module : String) (others : List String) (files : List File)
    (noModule : available files (moduleFile source module) = false)
    (noPackage : available files (packageFile source module) = false) :
    resolveModule (source :: others) files module = resolveModule others files module := by
  simp [resolveModule, searchPaths, noModule, noPackage]

theorem first_unresolved_selector_fails (sources : List String) (files : List File)
    (selector : Selector) (rest : List Selector) (missing : resolveModule sources files selector.module = none) :
    resolve sources files (selector :: rest) = .error selector.module := by
  simp [resolve, resolveWith, bindSelectors, missing, Except.map]

end HoiminOracle.SymbolResolution
