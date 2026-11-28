namespace HoiminOracle.SourceEscape

inductive Result
  | rejected | character (value : Char) | preserved (value : Char) | continuation
  deriving BEq

structure Observation where
  result : Result
  consumed : Nat
  lookup : Option String := none
  deriving BEq

def digit (base : Nat) (ch : Char) : Option Nat := do
  let value ←
    if '0' ≤ ch && ch ≤ '9' then some (ch.toNat - '0'.toNat)
    else if 'A' ≤ ch && ch ≤ 'F' then some (10 + ch.toNat - 'A'.toNat)
    else if 'a' ≤ ch && ch ≤ 'f' then some (10 + ch.toNat - 'a'.toNat)
    else none
  if value < base then some value else none

def number (base : Nat) (chars : List Char) : Option Nat :=
  (chars.mapM (digit base)).map (List.foldl (fun value d => base * value + d) 0)

def scalarResult (code : Nat) : Result :=
  if code > 0x10ffff || (0xd800 ≤ code && code ≤ 0xdfff) then .rejected
  else .character (Char.ofNat code)

def fixedWidth (parse : List Char → Option Nat) (width : Nat) (rest : List Char) : Observation :=
  let chunk := rest.take width
  if chunk.length != width then ⟨.rejected, 1, none⟩
  else match parse chunk with
    | none => ⟨.rejected, 1, none⟩
    | some value => ⟨scalarResult value, 1 + width, none⟩

def standardEscapes : List (Char × Char) :=
  [('\\', '\\'), ('\'', '\''), ('"', '"'), ('a', Char.ofNat 7), ('b', Char.ofNat 8),
   ('f', Char.ofNat 12), ('t', '\t'), ('n', '\n'), ('r', '\r'), ('v', Char.ofNat 11)]

structure Rules where
  hexadecimal : List Char → Option Nat := number 16
  octalWidth : Nat := 3
  wrapOctalBytes : Bool := true
  scanNameBytes : Nat := 128
  joinCrlf : Bool := true

def namedEscape (lookup : String → Option Char) (scanBytes : Nat) : List Char → Observation
  | '{' :: rest =>
    let nameChars := rest.takeWhile (· != '}')
    let name := String.ofList nameChars
    if name.utf8ByteSize < scanBytes && rest[nameChars.length]? == some '}' then
      match lookup name with
      | some value => ⟨.character value, 3 + name.utf8ByteSize, some name⟩
      | none => ⟨.rejected, 1, some name⟩
    else ⟨.rejected, 1, none⟩
  | _ => ⟨.rejected, 1, none⟩

def decodeWith (rules : Rules) (bytes : Bool) (lookup : String → Option Char) : List Char → Observation
  | [] => ⟨.rejected, 0, none⟩
  | '\n' :: _ => ⟨.continuation, 1, none⟩
  | '\r' :: rest => ⟨.continuation, if rules.joinCrlf && rest.head? == some '\n' then 2 else 1, none⟩
  | 'x' :: rest => fixedWidth rules.hexadecimal 2 rest
  | marker :: rest =>
    if !bytes && marker == 'u' then fixedWidth rules.hexadecimal 4 rest
    else if !bytes && marker == 'U' then fixedWidth rules.hexadecimal 8 rest
    else if !bytes && marker == 'N' then namedEscape lookup rules.scanNameBytes rest
    else if let some first := digit 8 marker then
      let tailDigits := (rest.take (rules.octalWidth - 1)).takeWhile fun ch => (digit 8 ch).isSome
      let code := (tailDigits.filterMap (digit 8)).foldl (fun value d => 8 * value + d) first
      ⟨scalarResult (if bytes && rules.wrapOctalBytes then code % 256 else code),
       1 + tailDigits.length, none⟩
    else
      match standardEscapes.lookup marker with
      | some value => ⟨.character value, 1, none⟩
      | none => ⟨.preserved marker, (String.singleton marker).utf8ByteSize, none⟩

def decode := decodeWith {}

theorem crlf_is_one_continuation (bytes : Bool) (lookup : String → Option Char) (rest : List Char) :
    decode bytes lookup ('\r' :: '\n' :: rest) = ⟨.continuation, 2, none⟩ := rfl

theorem unicode_markers_stay_literal_in_bytes (lookup : String → Option Char) (rest : List Char) :
    decode true lookup ('u' :: rest) = ⟨.preserved 'u', 1, none⟩ ∧
    decode true lookup ('U' :: rest) = ⟨.preserved 'U', 1, none⟩ ∧
    decode true lookup ('N' :: rest) = ⟨.preserved 'N', 1, none⟩ := by
  exact ⟨rfl, rfl, rfl⟩

theorem malformed_digits_keep_cursor_after_marker (parse : List Char → Option Nat)
    (width : Nat) (rest : List Char) (bad : parse (rest.take width) = none) :
    fixedWidth parse width rest = ⟨.rejected, 1, none⟩ := by
  simp [fixedWidth, bad]

theorem invalid_scalar_consumes_digits (parse : List Char → Option Nat)
    (width : Nat) (rest : List Char) (code : Nat) (complete : (rest.take width).length = width)
    (parsed : parse (rest.take width) = some code) (invalid : scalarResult code = .rejected) :
    fixedWidth parse width rest = ⟨.rejected, 1 + width, none⟩ := by
  simp [fixedWidth, complete, parsed, invalid]

theorem bytes_octal_value_below_256 (value : Nat) : value % 256 < 256 :=
  Nat.mod_lt _ (by decide)

theorem long_name_never_reaches_provider (lookup : String → Option Char) (scanBytes : Nat) (rest : List Char)
    (tooLong : scanBytes ≤ (String.ofList (rest.takeWhile (· != '}'))).utf8ByteSize) :
    namedEscape lookup scanBytes ('{' :: rest) = ⟨.rejected, 1, none⟩ := by
  simp [namedEscape, Nat.not_lt_of_ge tooLong]

end HoiminOracle.SourceEscape
