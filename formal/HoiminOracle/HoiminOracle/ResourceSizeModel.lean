namespace HoiminOracle.ResourceSize

def maxBytes : Nat := 18446744073709551615

def units : List (String × Nat) :=
  [("GiB", 1073741824), ("MiB", 1048576), ("KiB", 1024),
   ("GB", 1000000000), ("MB", 1000000), ("KB", 1000), ("B", 1)]

def scaledBytes (value : String) : Option Nat :=
  let unit := units.find? fun (suffix, _) => value.endsWith suffix
  let (digits, scale) := match unit with
    | some (suffix, multiplier) => ((value.dropEnd suffix.length).toString, multiplier)
    | none => (value, 1)
  let unsigned := if digits.startsWith "+" then (digits.drop 1).toString else digits
  if unsigned.isEmpty || !(unsigned.toList.all Char.isDigit) then none
  else
    let number := unsigned.foldl (fun total digit => total * 10 + (digit.toNat - '0'.toNat)) 0
    some (number * scale)

def parseBytes (value : String) : Option Nat :=
  (scaledBytes value).filter (· ≤ maxBytes)

theorem parsed_bytes_fit (value : String) (bytes : Nat) (parsed : parseBytes value = some bytes) :
    bytes ≤ maxBytes := by
  cases scaled : scaledBytes value with
  | none => simp [parseBytes, scaled] at parsed
  | some amount =>
    by_cases fits : amount ≤ maxBytes
    · simp [parseBytes, scaled, Option.filter, fits] at parsed
      simpa [← parsed] using fits
    · simp [parseBytes, scaled, Option.filter, fits] at parsed

inductive Field
  | memory | output | copy | workspace | reserve
  deriving DecidableEq, BEq

def fields : List Field := [.memory, .output, .copy, .workspace, .reserve]

def Field.flag : Field → String
  | .memory => "max-memory" | .output => "max-output" | .copy => "max-copy-size"
  | .workspace => "max-workspace-size" | .reserve => "min-free-space"

def Field.defaultValue : Field → String
  | .memory | .copy => "1GiB"
  | .output => "1MiB"
  | .workspace => "8GiB"
  | .reserve => "10GiB"

structure Request where
  sizes : List (Field × String) := []
  jobs : Nat := 1
  processes : Nat := 64

inductive Rejection
  | invalidBytes (field : Field) (value : String)
  | zeroSize (field : Field)
  | zeroJobs
  | invalidProcesses
  | jobsAboveMaximum (jobs : Nat)
  deriving DecidableEq, BEq

structure Limits where
  sizes : List (Field × Nat)
  jobs : Nat
  processes : Nat
  deriving DecidableEq, BEq

def resolveWith (parser : String → Option Nat) (order : List Field)
    (request : Request) : Except Rejection Limits := do
  let sizes ← order.mapM fun field => do
    let supplied := (request.sizes.find? fun (key, _) => key == field).map Prod.snd
    let value := supplied.getD field.defaultValue
    match parser value with
    | some bytes => pure (field, bytes)
    | none => throw (.invalidBytes field value)
  if request.jobs == 0 then throw .zeroJobs
  if request.processes == 0 || request.processes > 4294967295 then throw .invalidProcesses
  if request.jobs > 256 then throw (.jobsAboveMaximum request.jobs)
  match sizes.find? fun (_, bytes) => bytes == 0 with
  | some (field, _) => throw (.zeroSize field)
  | none => pure ⟨sizes, request.jobs, request.processes⟩

def resolve := resolveWith parseBytes fields

end HoiminOracle.ResourceSize
