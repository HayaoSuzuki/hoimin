import HoiminOracle.SourceDeclarationModel

namespace HoiminOracle.AnalyzerSourceAdmission

/-- Only parser outcomes relevant to admission, after strict decoding. -/
inductive ParsedShape | candidate | empty | invalidSyntax
  deriving DecidableEq

inductive Source | missing | present (bytes : List UInt8) (parsed : ParsedShape)

inductive Fatal | read | encoding (failure : SourceDeclaration.Failure)

def inspect : Source → Except Fatal ParsedShape
  | .missing => .error .read
  | .present bytes parsed =>
    match SourceDeclaration.decode bytes with
    | .error failure => .error (.encoding failure)
    | .ok _ => .ok parsed

structure Target where
  path : String
  source : Source

structure Scan where
  accepted : List (String × ParsedShape)
  failure : Option (String × Fatal)

def scanWith (inspectSource : Source → Except Fatal ParsedShape) : List Target → Scan
  | [] => ⟨[], none⟩
  | target :: rest =>
    match inspectSource target.source with
    | .error failure => ⟨[], some (target.path, failure)⟩
    | .ok parsed =>
      let tail := scanWith inspectSource rest
      ⟨(target.path, parsed) :: tail.accepted, tail.failure⟩

def scan := scanWith inspect

theorem decode_failure_precedes_parsing (bytes : List UInt8) (parsed : ParsedShape)
    (failure : SourceDeclaration.Failure) (h : SourceDeclaration.decode bytes = .error failure) :
    inspect (.present bytes parsed) = .error (.encoding failure) := by
  simp [inspect, h]

theorem invalid_syntax_continues (path : String) (bytes : List UInt8)
    (decoded : SourceDeclaration.Codec × String) (rest : List Target)
    (h : SourceDeclaration.decode bytes = .ok decoded) :
    scan (⟨path, .present bytes .invalidSyntax⟩ :: rest) =
      ⟨(path, .invalidSyntax) :: (scan rest).accepted, (scan rest).failure⟩ := by
  simp [scan, scanWith, inspect, h]

theorem fatal_prefix_ignores_suffix (earlier later : List Target)
    (failure : String × Fatal) (h : (scan earlier).failure = some failure) :
    scan (earlier ++ later) = scan earlier := by
  induction earlier with
  | nil => simp [scan, scanWith] at h
  | cons target rest ih =>
    cases hs : inspect target.source with
    | error fatal => simp [scan, scanWith, hs]
    | ok parsed =>
      have tailFailure : (scan rest).failure = some failure := by
        simpa [scan, scanWith, hs] using h
      have tailEquality := ih tailFailure
      change scanWith inspect (target :: (rest ++ later)) =
        scanWith inspect (target :: rest)
      simp only [scanWith, hs]
      rw [show scanWith inspect (rest ++ later) = scanWith inspect rest from tailEquality]

end HoiminOracle.AnalyzerSourceAdmission
