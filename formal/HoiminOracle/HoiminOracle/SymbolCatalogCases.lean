import HoiminOracle.SymbolCatalogModel
import Lean.Data.Json

namespace HoiminOracle.SymbolCatalog

private def root : Suite := .empty
private def literal (source : String) (tail : Suite := .empty) : Suite := .line ⟨source, none, ""⟩ tail
private def flag (value : Bool) (tail : Suite := .empty) : Suite := .line ⟨"flag = ", some value, ""⟩ tail
private def block (header : String) (body : Suite) (tail : Suite := .empty) : Suite :=
  .block ⟨header, none, ""⟩ body tail

private inductive Role | function | asynchronous | classDef

private def heading (role : Role) (name : String) : String :=
  match role with
  | .function => "def " ++ name ++ "():"
  | .asynchronous => "async def " ++ name ++ "():"
  | .classDef => "class " ++ name ++ ":"

private def define (role : Role) (name : String) (body : Suite) (tail : Suite := .empty) : Suite :=
  .definition name [⟨heading role name, none, ""⟩] body tail

private structure Site where
  start : Nat
  scope : Name
  value : Bool
  deriving BEq

private structure Rendered where
  source : String := ""
  sites : List Site := []

private def join (left right : Rendered) : Rendered :=
  ⟨left.source ++ right.source, left.sites ++ right.sites⟩

private def truth (value : Bool) := if value then "True" else "False"

private def emitLine (ending indent : String) (scope : Name) (start : Nat) (line : Line) : Rendered :=
  let initial := indent ++ line.before
  ⟨initial ++ (line.value.map truth).getD "" ++ line.after ++ ending,
    line.value.toList.map fun value => ⟨start + initial.utf8ByteSize, scope, value⟩⟩

private def emitHeadings (ending indent : String) (scope : Name) : Nat → List Line → Rendered
  | _, [] => {}
  | start, line :: more =>
    let head := emitLine ending indent scope start line
    join head (emitHeadings ending indent scope (start + head.source.utf8ByteSize) more)

private def render (ending : String) : String → Name → Nat → Suite → Rendered
  | _, _, _, .empty => {}
  | indent, scope, start, .line line tail =>
    let head := emitLine ending indent scope start line
    join head (render ending indent scope (start + head.source.utf8ByteSize) tail)
  | indent, scope, start, .block heading body tail =>
    let head := emitLine ending indent scope start heading
    let subtree := join head (render ending (indent ++ "    ") scope (start + head.source.utf8ByteSize) body)
    join subtree (render ending indent scope (start + subtree.source.utf8ByteSize) tail)
  | indent, scope, start, .definition name heading body tail =>
    let inner := scope ++ [name]
    let head := emitHeadings ending indent inner start heading
    let subtree := join head (render ending (indent ++ "    ") inner (start + head.source.utf8ByteSize) body)
    join subtree (render ending indent scope (start + subtree.source.utf8ByteSize) tail)

private structure Fixture where
  id : String
  program : Suite
  parents : Name := ["src", "pkg"]
  stem : String := "subject"
  ending : String := "\n"

private def roles : List (String × Role) := [("def", .function), ("async", .asynchronous), ("class", .classDef)]

private def common (outer child : Role) : Suite :=
  flag true (define outer "outer"
    (flag false (define child "child"
      (flag true (define .function "leaf" (flag false)))
      (define .function "childish" (flag true))))
    (define .function "outerish" (flag false) (flag true)))

private def matrix : List Fixture := roles.flatMap fun (first, outer) =>
  roles.map fun (second, child) =>
    { id := s!"{first}-{second}", program := common outer child }

private def focused : List Fixture := [
  { id := "empty-module", program := root },
  {
    id := "no-symbols"
    program := literal "import package as outer"
      (.line ⟨"alias = lambda: ", some true, ""⟩ (literal "text = 'def fake(): pass'"))
  },
  {
    id := "duplicate-definitions"
    program := block "if condition:"
      (define .function "outer" (flag true (define .function "left" (flag false))))
      (block "else:" (define .classDef "outer" (flag false (define .asynchronous "right" (flag true))))
        (define .function "outer" (flag false)))
  },
  {
    id := "if-elif-else"
    program := define .function "outer" (.block ⟨"if ", some false, ":"⟩
      (define .classDef "left" (flag true))
      (block "elif condition:" (define .function "middle" (flag false))
        (block "else:" (define .asynchronous "right" (flag true)))))
  },
  {
    id := "try-branches"
    program := define .classDef "outer"
      (block "try:" (define .function "try_body" (flag true))
        (block "except Exception:" (define .function "handler" (flag false))
          (block "else:" (define .function "success" (flag true))
            (block "finally:" (define .asynchronous "cleanup" (flag false))))))
  },
  {
    id := "match-branches"
    program := define .function "outer" (block "match value:"
      (block "case 0:" (define .function "first" (flag true))
        (block "case _:" (define .classDef "second" (flag false)))))
  },
  {
    id := "async-suites"
    program := define .asynchronous "outer" (block "async with manager():"
      (block "async for item in items:" (define .function "inside" (flag true))
        (block "else:" (define .classDef "outside" (flag false)))))
  },
  {
    id := "headers"
    program := .definition "outer"
      [⟨"@decorate(", some true, ")"⟩, ⟨"@other(", some false, ")"⟩,
       ⟨"def outer(value: Literal[True] = ", some false, "):"⟩]
      (.definition "Child" [⟨"class Child(metaclass=choose(", some true, ")):"⟩] (flag false) root)
      (flag true)
  },
  {
    id := "unreachable-definition"
    program := define .function "outer"
      (literal "return" (define .function "after_return" (literal "pass")))
      (define .classDef "empty" (literal "pass"))
  },
  {
    id := "unicode-crlf"
    ending := "\r\n"
    program := literal "# 雪" (define .classDef "箱" (define .function "実行" (flag true))
      (define .function "箱庭" (flag false) (flag true)))
  },
  { id := "package-init", stem := "__init__", program := common .classDef .function },
  { id := "root-module", parents := [], program := common .function .asynchronous }]

private def fixtures := matrix ++ focused

private def location (item : Fixture) : Name := modulePath item.parents item.stem

private def selectorText (selector : Selector) : String :=
  (selector.modulePath.map (fun requested => dotted requested ++ ":")).getD "" ++ dotted selector.name

private def queries (item : Fixture) : List (List Selector) :=
  let allNames := (definitions [] item.program).eraseDups
  let actualModule := location item
  let selectedName := allNames.head?.getD ["unavailable"]
  let unqualified : List (List Selector) := allNames.map (fun name => [⟨name, none⟩])
  let variants : List (List Selector) :=
    [[⟨selectedName, some actualModule⟩],
     [⟨selectedName, some actualModule.getLast?.toList⟩],
     [⟨selectedName, some ["elsewhere"]⟩],
     [⟨selectedName, some ["ubject"]⟩],
     [⟨["outer", "chil"], none⟩],
     [⟨["oute"], none⟩],
     [⟨selectedName, none⟩, ⟨selectedName, none⟩, ⟨["missing"], some ["elsewhere"]⟩]]
  ([[]] ++ unqualified ++ variants).eraseDups

private def filterSites (admit : Name → List Selector → Name → Bool)
    (item : Fixture) (rendered : Rendered) (selectors : List Selector) : List Site :=
  rendered.sites.filter (fun site => admit (location item) selectors site.scope)

private def siteJson (site : Site) : Lean.Json :=
  Lean.Json.mkObj [("start", Lean.toJson site.start), ("original", .str (truth site.value)),
    ("replacement", .str (truth (!site.value))),
    ("symbol", if site.scope.isEmpty then .null else .str (dotted site.scope))]

private def fixtureJson (item : Fixture) : Lean.Json :=
  let output := render item.ending "" [] 0 item.program
  let cases := (queries item).map fun selectors => Lean.Json.mkObj [
    ("selectors", Lean.toJson (selectors.map selectorText)),
    ("candidates", .arr ((filterSites selected item output selectors).map siteJson).toArray)]
  let names := ((definitions [] item.program).map dotted).eraseDups.mergeSort (· ≤ ·)
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"), ("id", .str item.id),
    ("path", .str (String.intercalate "/" (item.parents ++ [item.stem ++ ".py"]))),
    ("source", .str output.source), ("definitions", Lean.toJson names),
    ("queries", .arr cases.toArray)]

def corpus : String := String.join (fixtures.map fun fixture => (fixtureJson fixture).compress ++ "\n")

def sensitivity : IO UInt32 := do
  for (family, broken) in [
      ("symbol_catalog_exact_only", fun actual selectors scope =>
        selectors.isEmpty || (availableSelectors actual selectors).contains scope),
      ("symbol_catalog_match_character_prefix", fun actual selectors scope =>
        selectors.isEmpty || (availableSelectors actual selectors).any (fun q => (dotted scope).startsWith (dotted q))),
      ("symbol_catalog_ignore_module", fun actual selectors scope =>
        selected actual (selectors.map fun q => { q with modulePath := none }) scope),
      ("symbol_catalog_match_module_character_suffix", fun actual selectors scope =>
        selectors.isEmpty || (!scope.isEmpty && selectors.any fun q =>
          q.modulePath.all (fun requested => (dotted actual).endsWith (dotted requested)) &&
            q.name.isPrefixOf scope)),
      ("symbol_catalog_empty_filtered_set_means_all", fun actual selectors scope =>
        (availableSelectors actual selectors).isEmpty || selected actual selectors scope)] do
    let witness := fixtures.findSome? fun item =>
      let output := render item.ending "" [] 0 item.program
      (queries item).findSome? fun selectors =>
        if filterSites selected item output selectors != filterSites broken item output selectors then
          some s!"{item.id} selectors={selectors.map selectorText}" else none
    match witness with
    | some detail => IO.println s!"family={family} detected=true case={detail}"
    | none =>
      IO.eprintln s!"symbol catalog corpus missed broken family {family}"
      return 2
  let flattened := fun item => (definitions [] item.program).map (·.getLast?.getD "")
  match fixtures.find? (fun item => flattened item != (definitions [] item.program).map dotted) with
  | some item =>
    IO.println s!"family=symbol_catalog_drop_enclosing_definitions detected=true case={item.id}"
    return 0
  | none =>
    IO.eprintln "symbol catalog corpus missed definitions without qualifiers"
    return 2

end HoiminOracle.SymbolCatalog
