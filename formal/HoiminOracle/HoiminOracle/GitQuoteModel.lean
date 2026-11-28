namespace HoiminOracle.GitQuote

inductive Diagnostic
  | framing | escape | octal | utf8

inductive NamedEscape
  | backslash | quote | bell | backspace | tab | newline | verticalTab | formFeed | carriageReturn

def NamedEscape.code : NamedEscape → Char
  | .backslash => '\\'
  | .quote => '"'
  | .bell => 'a'
  | .backspace => 'b'
  | .tab => 't'
  | .newline => 'n'
  | .verticalTab => 'v'
  | .formFeed => 'f'
  | .carriageReturn => 'r'

def NamedEscape.byte : NamedEscape → UInt8
  | .backslash => 92
  | .quote => 34
  | .bell => 7
  | .backspace => 8
  | .tab => 9
  | .newline => 10
  | .verticalTab => 11
  | .formFeed => 12
  | .carriageReturn => 13

def octalByte (value : Fin 512) : Except Diagnostic UInt8 :=
  if value.val < 256 then .ok (UInt8.ofNat value.val) else .error .octal

def octalDigits (value : Fin 512) : Nat × Nat × Nat :=
  (value.val / 64, value.val / 8 % 8, value.val % 8)

theorem octal_digits_represent (value : Fin 512) :
    let (first, second, third) := octalDigits value
    first < 8 ∧ second < 8 ∧ third < 8 ∧
      first * 64 + second * 8 + third = value.val := by
  have bound := value.isLt
  simp only [octalDigits]
  omega

theorem octal_byte_exact (value : Fin 512) (byte : UInt8) :
    octalByte value = .ok byte ↔ value.val = byte.toNat := by
  by_cases fits : value.val < 256
  · simp only [octalByte, if_pos fits, Except.ok.injEq]
    constructor
    · intro equal
      rw [← equal, UInt8.toNat_ofNat', Nat.mod_eq_of_lt fits]
    · intro equal
      rw [equal, UInt8.ofNat_toNat]
  · have bound := byte.toNat_lt
    simp [octalByte, fits, show value.val ≠ byte.toNat by omega]

inductive Chunk
  | literal (text : String)
  | named (escape : NamedEscape)
  | octal (value : Fin 512)

def Chunk.bytes (named : NamedEscape → UInt8) (octal : Fin 512 → Except Diagnostic UInt8) :
    Chunk → Except Diagnostic (List UInt8)
  | .literal text => .ok text.toUTF8.toList
  | .named escape => .ok [named escape]
  | .octal value => (octal value).map fun byte => [byte]

inductive QuoteForm
  | complete
  | missingOpeningQuote
  | missingClosingQuote
  | danglingBackslash
  | unknownEscape (code : Char)
  | malformedOctal (digits : String)

def QuoteForm.tailBytes : QuoteForm → Except Diagnostic (List UInt8)
  | .complete => .ok []
  | .missingOpeningQuote | .missingClosingQuote => .error .framing
  | .danglingBackslash | .unknownEscape _ => .error .escape
  | .malformedOctal _ => .error .octal

def assemble (bytes : Chunk → Except Diagnostic (List UInt8)) (form : QuoteForm) :
    List Chunk → Except Diagnostic (List UInt8)
  | [] => form.tailBytes
  | chunk :: remaining =>
    match bytes chunk with
    | .error reason => .error reason
    | .ok current => (assemble bytes form remaining).map (current ++ ·)

def validateUtf8 (bytes : List UInt8) : Except Diagnostic String :=
  match String.fromUTF8? ⟨bytes.toArray⟩ with
  | some text => .ok text
  | none => .error .utf8

def decodeWith (bytes : Chunk → Except Diagnostic (List UInt8))
    (validate : List UInt8 → Except Diagnostic String) (chunks : List Chunk) (form : QuoteForm) :
    Except Diagnostic String :=
  match form with
  | .missingOpeningQuote | .missingClosingQuote => .error .framing
  | other => (assemble bytes other chunks).bind validate

def decode := decodeWith (Chunk.bytes NamedEscape.byte octalByte) validateUtf8

theorem first_bad_chunk (bytes : Chunk → Except Diagnostic (List UInt8))
    (chunk : Chunk) (remaining : List Chunk) (form : QuoteForm) (error : Diagnostic)
    (bad : bytes chunk = .error error) :
    assemble bytes form (chunk :: remaining) = .error error := by
  simp [assemble, bad]

theorem successful_prefix_preserves_error (bytes : Chunk → Except Diagnostic (List UInt8))
    (before after : List Chunk) (form : QuoteForm) (error : Diagnostic)
    (leading : List UInt8) (ok : assemble bytes .complete before = .ok leading)
    (bad : assemble bytes form after = .error error) :
    assemble bytes form (before ++ after) = .error error := by
  induction before generalizing leading with
  | nil => exact bad
  | cons chunk rest ih =>
    cases current : bytes chunk with
    | error reason => simp [assemble, current] at ok
    | ok payload =>
      cases tail : assemble bytes .complete rest with
      | error reason => simp [assemble, current, tail, Except.map] at ok
      | ok suffix => simp [assemble, current, ih suffix tail, Except.map]

theorem framing_precedes_decoding (chunks : List Chunk) :
    decode chunks .missingClosingQuote = .error .framing := rfl

end HoiminOracle.GitQuote
