import HoiminOracle.TargetPathModel
import Lean.Data.Json

namespace HoiminOracle.TargetPath

open HoiminOracle.ImportRoot (Platform reduce)

private structure Fixture where
  id : String
  root : String
  input : String
  rootParts : List String
  selector : Selector
  platform : Option Platform := none

private def windowsFold (part : String) : String := String.ofList (part.toList.map fun c =>
  match c with
  | 'ä' => 'Ä'
  | 'ς' | 'σ' => 'Σ'
  | other => other.toUpper)

private def compareParts (platform : Platform) (left right : String) : Bool :=
  if platform == .windows then windowsFold left == windowsFold right else left == right

private def small : Nat → List (List String)
  | 0 => [[]]
  | size + 1 => ["pkg", "file.py", "", ".", ".."].flatMap fun component =>
      (small size).map (component :: ·)

private def relativeCases : List Fixture := ((List.range 4).flatMap small).zipIdx.map fun (parts, i) =>
  { id := s!"relative-{i}", root := "/Project", input := "./" ++ String.intercalate "/" parts,
    rootParts := ["Project"], selector := .relative parts }

private def absoluteCases : List Fixture := ((List.range 3).flatMap small).zipIdx.flatMap fun (parts, i) => [
  ⟨s!"posix-absolute-{i}", "/Project", "/Project/" ++ String.intercalate "/" parts,
    ["Project"], .absolute ("Project" :: parts), some .posix⟩,
  ⟨s!"windows-absolute-{i}", "C:/Project", "c:\\PROJECT/" ++ String.intercalate "/" parts,
    ["C:", "Project"], .absolute (["c:", "PROJECT"] ++ parts), some .windows⟩]

private def fixtures : List Fixture := relativeCases ++ absoluteCases ++ [
  { id := "empty-selector", root := "/Project", input := "", rootParts := ["Project"], selector := .relative [] },
  ⟨"posix-exact-root", "/Project", "/Project", ["Project"], .absolute ["Project"], some .posix⟩,
  ⟨"posix-case-sensitive", "/Project", "/project/file.py", ["Project"],
    .absolute ["project", "file.py"], some .posix⟩,
  ⟨"posix-sibling-prefix", "/Project", "/ProjectBackup/file.py", ["Project"],
    .absolute ["ProjectBackup", "file.py"], some .posix⟩,
  ⟨"posix-too-short", "/Project/sub", "/Project", ["Project", "sub"], .absolute ["Project"], some .posix⟩,
  ⟨"posix-absolute-dot-before-root-component", "/Project/sub", "/Project/./sub/file.py", ["Project", "sub"],
    .absolute ["Project", ".", "sub", "file.py"], some .posix⟩,
  ⟨"posix-prefix-requires-matching-before-reduction", "/Project", "/outside/../Project/file.py", ["Project"],
    .absolute ["outside", "..", "Project", "file.py"], some .posix⟩,
  ⟨"posix-backslash-is-literal", "/Project", "pkg\\file.py", ["Project"],
    .relative ["pkg\\file.py"], some .posix⟩,
  ⟨"posix-root-escape-cannot-reenter", "/Project", "/Project/../Project/file.py", ["Project"],
    .absolute ["Project", "..", "Project", "file.py"], some .posix⟩,
  ⟨"posix-root-filesystem", "/", "/pkg/file.py", [], .absolute ["pkg", "file.py"], some .posix⟩,
  ⟨"windows-exact-root", "C:/Project", "c:\\PROJECT", ["C:", "Project"], .absolute ["c:", "PROJECT"], some .windows⟩,
  ⟨"windows-sibling-prefix", "C:/Project", "C:/ProjectBackup/file.py", ["C:", "Project"],
    .absolute ["C:", "ProjectBackup", "file.py"], some .windows⟩,
  ⟨"windows-other-drive", "C:/Project", "D:/Project/file.py", ["C:", "Project"],
    .absolute ["D:", "Project", "file.py"], some .windows⟩,
  ⟨"windows-case-umlaut", "C:/Ä", "c:/ä/pkg/A.py", ["C:", "Ä"],
    .absolute ["c:", "ä", "pkg", "A.py"], some .windows⟩,
  ⟨"windows-case-final-sigma", "C:/Σ", "c:/ς/pkg/A.py", ["C:", "Σ"],
    .absolute ["c:", "ς", "pkg", "A.py"], some .windows⟩,
  ⟨"windows-no-multiscalar-fold", "C:/ß", "C:/SS/file.py", ["C:", "ß"],
    .absolute ["C:", "SS", "file.py"], some .windows⟩,
  ⟨"windows-rooted-no-drive", "C:/Project", "\\folder\\file.py", ["C:", "Project"], .invalidAnchor, some .windows⟩,
  ⟨"windows-drive-relative", "C:/Project", "C:folder/file.py", ["C:", "Project"], .invalidAnchor, some .windows⟩,
  ⟨"windows-suffix-cannot-start-with-drive-prefix", "C:/Project", "C:/Project/D:folder/file.py", ["C:", "Project"],
    .absolute ["C:", "Project", "D:folder", "file.py"], some .windows⟩,
  ⟨"windows-native-relative", "C:/Project", "pkg\\tmp/..\\File.py", ["C:", "Project"],
    .relative ["pkg", "tmp", "..", "File.py"], some .windows⟩,
  ⟨"windows-unc-within-share", "\\\\server\\share\\project", "\\\\SERVER\\SHARE\\PROJECT\\File.py", ["server", "share", "project"],
    .absolute ["SERVER", "SHARE", "PROJECT", "File.py"], some .windows⟩,
  ⟨"windows-unc-sibling-share", "\\\\server\\share\\project", "\\\\server\\share2\\project\\File.py", ["server", "share", "project"],
    .absolute ["server", "share2", "project", "File.py"], some .windows⟩]

private def expected (item : Fixture) : Option String :=
  let platform := item.platform.getD .posix
  normalize platform (compareParts platform) item.rootParts item.selector

private def fixtureJson (item : Fixture) : Lean.Json :=
  let platform := match item.platform with
    | none => "any"
    | some .posix => "posix"
    | some .windows => "windows"
  Lean.Json.mkObj [("schema", Lean.toJson (1 : Nat)), ("mode", .str "strict"),
    ("id", .str item.id), ("platform", .str platform), ("root", .str item.root),
    ("input", .str item.input), ("normalized", Lean.toJson (expected item))]

def corpus : String := String.join (fixtures.map fun item => (fixtureJson item).compress ++ "\n")

private def clampParents (stack : List String) : List String → List String
  | [] => stack.reverse
  | part :: rest =>
    if part == "" || part == "." then clampParents stack rest
    else if part == ".." then clampParents (stack.drop 1) rest
    else clampParents (part :: stack) rest

def sensitivity : IO UInt32 := do
  for (name, broken) in [
      ("target_path_match_partial_component", fun item : Fixture =>
        let platform := item.platform.getD .posix
        let fold part := if platform == .windows then windowsFold part else part
        normalize platform (fun root part => (fold part).startsWith (fold root)) item.rootParts item.selector),
      ("target_path_clamp_parent_at_root", fun item =>
        let platform := item.platform.getD .posix
        (relativeComponents platform (compareParts platform) item.rootParts item.selector).map fun parts =>
          String.intercalate "/" (clampParents [] parts)),
      ("target_path_case_sensitive_on_windows", fun item =>
        normalize (item.platform.getD .posix) (· == ·) item.rootParts item.selector),
      ("target_path_case_insensitive_on_posix", fun item =>
        normalize (item.platform.getD .posix) (compareParts .windows) item.rootParts item.selector),
      ("target_path_reject_root_itself", fun item => (expected item).filter fun path => !path.isEmpty)] do
    match fixtures.find? (fun item => broken item != expected item) with
    | some item => IO.println s!"family={name} detected=true case={item.id}"
    | none =>
      IO.eprintln s!"target path corpus missed broken family {name}"
      return 2
  return 0

end HoiminOracle.TargetPath
