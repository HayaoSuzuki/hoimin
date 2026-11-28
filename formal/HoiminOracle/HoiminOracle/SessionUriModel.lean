import Std

namespace HoiminOracle.SessionUri

def percent : Nat := 37
def question : Nat := 63
def scheme : List Nat := [102, 105, 108, 101, 58]
def options : List Nat := [question, 105, 109, 109, 117, 116, 97, 98, 108, 101, 61, 49]

def hexDigit (value : Nat) : Nat :=
  if value < 10 then 48 + value else 55 + value

def readHexDigit (code : Nat) : Option Nat :=
  if 48 ≤ code ∧ code ≤ 57 then some (code - 48)
  else if 65 ≤ code ∧ code ≤ 70 then some (code - 55)
  else none

theorem readHexDigit_hexDigit (value : Nat) (bounded : value < 16) :
    readHexDigit (hexDigit value) = some value := by
  unfold readHexDigit hexDigit
  split
  · rename_i small
    have decimal : 48 ≤ 48 + value ∧ 48 + value ≤ 57 := by omega
    simp [decimal]
  · rename_i large
    have notDecimal : ¬(48 ≤ 55 + value ∧ 55 + value ≤ 57) := by omega
    have upper : 65 ≤ 55 + value ∧ 55 + value ≤ 70 := by omega
    simp [notDecimal, upper]

def escapeByte (byte : UInt8) : List Nat :=
  [percent, hexDigit (byte.toNat / 16), hexDigit (byte.toNat % 16)]

def escapePath (bytes : List UInt8) : List Nat :=
  bytes.flatMap escapeByte

def decodePath : List Nat → Option (List UInt8)
  | [] => some []
  | marker :: high :: low :: rest => do
    if marker ≠ percent then none else do
      let hi ← readHexDigit high
      let lo ← readHexDigit low
      let tail ← decodePath rest
      return UInt8.ofNat (16 * hi + lo) :: tail
  | _ => none

theorem decode_escapePath (bytes : List UInt8) :
    decodePath (escapePath bytes) = some bytes := by
  induction bytes with
  | nil => rfl
  | cons byte tail ih =>
    have byteBound : byte.toNat < 256 := byte.toBitVec.isLt
    have hiBound : byte.toNat / 16 < 16 := by omega
    have loBound : byte.toNat % 16 < 16 := by omega
    have sum : 16 * (byte.toNat / 16) + byte.toNat % 16 = byte.toNat := by omega
    change decodePath (escapeByte byte ++ escapePath tail) = some (byte :: tail)
    simp [escapeByte, decodePath, readHexDigit_hexDigit, hiBound, loBound, ih, sum,
      bind, Option.bind, -UInt8.ofNat_add, -UInt8.ofNat_mul]

def rawUri (bytes : List UInt8) : List Nat :=
  scheme ++ escapePath bytes ++ options

def render (codes : List Nat) : String :=
  String.ofList (codes.map Char.ofNat)

def immutableUri (bytes : List UInt8) : String := render (rawUri bytes)

theorem hexDigit_is_path_code (value : Nat) (bounded : value < 16) :
    hexDigit value < 128 ∧ hexDigit value ≠ question ∧ hexDigit value ≠ 35 ∧ hexDigit value ≠ 47 := by
  unfold hexDigit question
  split <;> omega

theorem escaped_path_codes (bytes : List UInt8) (code : Nat)
    (member : code ∈ escapePath bytes) : code < 128 ∧ code ≠ question ∧ code ≠ 35 ∧ code ≠ 47 := by
  obtain ⟨byte, _, escaped⟩ := List.mem_flatMap.mp member
  have limit : byte.toNat < 256 := byte.toBitVec.isLt
  simp only [escapeByte, List.mem_cons, List.not_mem_nil, or_false] at escaped
  rcases escaped with equal | equal | equal
  · subst code
    decide
  · rw [equal]
    exact hexDigit_is_path_code _ (by omega)
  · rw [equal]
    exact hexDigit_is_path_code _ (by omega)

def splitOptions (codes : List Nat) : List Nat × List Nat :=
  match codes with
  | [] => ([], [])
  | code :: rest =>
    if code = question then ([], codes)
    else let (path, query) := splitOptions rest; (code :: path, query)

private theorem splitOptions_at_delimiter (codes tail : List Nat)
    (noDelimiter : ∀ code ∈ codes, code ≠ question) :
    splitOptions (codes ++ question :: tail) = (codes, question :: tail) := by
  induction codes with
  | nil => simp [splitOptions]
  | cons code rest ih =>
    have notQuestion := noDelimiter code (by simp)
    have restSafe : ∀ candidate ∈ rest, candidate ≠ question := by
      intro candidate mem
      exact noDelimiter candidate (List.mem_cons_of_mem _ mem)
    simp [splitOptions, notQuestion, ih restSafe]

def parseImmutable (codes : List Nat) : Option (List UInt8) :=
  if codes.take scheme.length = scheme then
    let (path, query) := splitOptions (codes.drop scheme.length)
    if query = options then decodePath path else none
  else none

theorem parse_rawUri (bytes : List UInt8) : parseImmutable (rawUri bytes) = some bytes := by
  have safe : ∀ code ∈ escapePath bytes, code ≠ question := by
    intro code member
    exact (escaped_path_codes bytes code member).2.1
  have splitQuery : splitOptions (escapePath bytes ++ options) = (escapePath bytes, options) :=
    splitOptions_at_delimiter (escapePath bytes) _ safe
  simp [parseImmutable, rawUri, scheme, splitQuery, decode_escapePath]

private theorem ascii_rawUri (bytes : List UInt8) (code : Nat) (member : code ∈ rawUri bytes) :
    code < 128 := by
  simp only [rawUri, List.mem_append] at member
  rcases member with (inScheme | inPath) | inOptions
  · simp only [scheme, List.mem_cons, List.not_mem_nil, or_false] at inScheme
    omega
  · exact (escaped_path_codes bytes code inPath).1
  · simp only [options, question, List.mem_cons, List.not_mem_nil, or_false] at inOptions
    omega

private theorem render_recovers_ascii (codes : List Nat) (ascii : ∀ code ∈ codes, code < 128) :
    (render codes).toList.map Char.toNat = codes := by
  simp only [render, String.toList_ofList, List.map_map]
  induction codes with
  | nil => rfl
  | cons code tail ih =>
    have valid : code.isValidChar := Or.inl (by have := ascii code (by simp); omega)
    have recovered : (Char.ofNat code).toNat = code := by
      simp [Char.ofNat, valid, Char.ofNatAux, Char.toNat, UInt32.toNat]
    simp [recovered, ih (fun c mem => ascii c (List.mem_cons_of_mem _ mem))]

theorem parse_immutableUri (bytes : List UInt8) :
    parseImmutable ((immutableUri bytes).toList.map Char.toNat) = some bytes := by
  rw [immutableUri, render_recovers_ascii _ (ascii_rawUri bytes), parse_rawUri]

theorem immutableUri_injective {left right : List UInt8} (same : immutableUri left = immutableUri right) :
    left = right := by
  have recovered := congrArg (fun uri : String => parseImmutable (uri.toList.map Char.toNat)) same
  simpa [parse_immutableUri] using recovered

end HoiminOracle.SessionUri
