namespace HoiminOracle.SourceDeclaration

inductive Header | blank | comment (label : Option String) | code

def marker := "coding".toUTF8.toList

def labelByte (byte : UInt8) : Bool :=
  (48 ≤ byte && byte ≤ 57) || (65 ≤ byte && byte ≤ 90) || (97 ≤ byte && byte ≤ 122) ||
    ([45, 46, 95] : List UInt8).contains byte

def labelAt (suffix : List UInt8) : Option String := do
  if !marker.isPrefixOf suffix then none else
    match suffix.drop marker.length with
    | sep :: rest =>
      if sep != 58 && sep != 61 then none else
        let token := (rest.dropWhile fun byte => byte == 32 || byte == 9).takeWhile labelByte
        if token.isEmpty then none else some (String.ofList (token.map (Char.ofNat ∘ UInt8.toNat)))
    | [] => none

def classifyHeader (line : List UInt8) : Header :=
  match line.dropWhile (fun byte => ([32, 9, 12] : List UInt8).contains byte) with
  | [] => .blank
  | 35 :: comment => .comment ((List.range comment.length).findSome? fun index => labelAt (comment.drop index))
  | _ => .code

def breakLine (bytes : List UInt8) : List UInt8 × List UInt8 :=
  let line := bytes.takeWhile fun byte => byte != 13 && byte != 10
  let rest := match bytes.drop line.length with
    | 13 :: 10 :: rest => rest
    | _ :: rest => rest
    | [] => []
  (line, rest)

def headersWith (nextLine : List UInt8 → List UInt8 × List UInt8) : Nat → List UInt8 → List Header
  | 0, _ => []
  | n + 1, bytes =>
    let (line, rest) := nextLine bytes
    classifyHeader line :: headersWith nextLine n rest

def firstLabel : List Header → Option String
  | [] | .code :: _ => none
  | .comment (some label) :: _ => some label
  | .blank :: rest | .comment none :: rest => firstLabel rest

def leadingBom : List UInt8 := [239, 187, 191]

def stripBom (bytes : List UInt8) : Bool × List UInt8 :=
  if leadingBom.isPrefixOf bytes then (true, bytes.drop leadingBom.length) else (false, bytes)

def normalize (label : String) : String := label.toLower.replace "_" "-"

def compatibleUtf8 (normalized : String) : Bool := normalized == "utf-8" || normalized.startsWith "utf-8-"

inductive Codec | utf8 | ascii | latin1

def supportedCodec (normalized : String) : Option Codec :=
  if compatibleUtf8 normalized || normalized == "utf8" then some .utf8
  else if ["ascii", "us-ascii", "646"].contains normalized then some .ascii
  else if ["latin1", "latin-1", "iso-8859-1", "iso-latin-1", "iso8859-1", "l1", "latin", "cp819", "ibm819"].contains normalized ||
      ["latin-1-", "iso-8859-1-", "iso-latin-1-"].any (fun family => normalized.startsWith family) then some .latin1
  else none

inductive Reason | bomConflict | unsupportedCodec | invalidUtf8 | nonAscii (index : Nat)

structure Failure where
  label : Option String
  reason : Reason

def selectCodec (bom : Bool) : Option String → Except Failure Codec
  | none => .ok .utf8
  | some label =>
    let normalized := normalize label
    if bom && !compatibleUtf8 normalized then .error ⟨some label, .bomConflict⟩
    else match supportedCodec normalized with
      | none => .error ⟨some label, .unsupportedCodec⟩
      | some codec => .ok codec

def decodeUsing (select : Bool → Option String → Except Failure Codec) (bom : Bool) (declared : Option String)
    (bytes : List UInt8) : Except Failure (Codec × String) := do
  let codec ← select bom declared
  match codec with
  | .latin1 => return (codec, String.ofList (bytes.map (Char.ofNat ∘ UInt8.toNat)))
  | .ascii =>
    if let some index := bytes.findIdx? (fun byte => 128 ≤ byte) then
      throw ⟨declared, .nonAscii index⟩
    else return (codec, String.ofList (bytes.map (Char.ofNat ∘ UInt8.toNat)))
  | .utf8 =>
    match String.fromUTF8? ⟨bytes.toArray⟩ with
    | some text => return (codec, text)
    | none => throw ⟨declared, .invalidUtf8⟩

def decodeWith (lookup : List UInt8 → Option String) (select : Bool → Option String → Except Failure Codec)
    (bytes : List UInt8) : Except Failure (Codec × String) :=
  let (bom, commentBytes) := stripBom bytes
  decodeUsing select bom (lookup commentBytes) bytes

def decode := decodeWith (firstLabel ∘ headersWith breakLine 2) selectCodec

theorem code_blocks_later_declarations (following : List Header) : firstLabel (.code :: following) = none := rfl

theorem first_label_wins (label : String) (following : List Header) :
    firstLabel (.comment (some label) :: following) = some label := rfl

theorem two_line_search (bytes : List UInt8) :
    let (first, remaining) := breakLine bytes
    let (second, _) := breakLine remaining
    firstLabel (headersWith breakLine 2 bytes) = firstLabel [classifyHeader first, classifyHeader second] := rfl

theorem undeclared_defaults_to_utf8 (bom : Bool) : selectCodec bom none = .ok .utf8 := rfl

theorem incompatible_bom_precedes_payload_errors (label : String) (raw : List UInt8)
    (invalid : compatibleUtf8 (normalize label) = false) :
    decodeUsing selectCodec true (some label) raw = .error ⟨some label, .bomConflict⟩ := by
  simp [decodeUsing, selectCodec, invalid, bind, Except.bind]

end HoiminOracle.SourceDeclaration
