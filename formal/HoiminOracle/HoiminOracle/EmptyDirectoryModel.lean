import Lean
open Lean
namespace HoiminOracle.EmptyDirectory
inductive Entry where
 | absent | file | directory
 deriving Repr, DecidableEq, BEq

def wanted (selected excluded : Bool) : Bool := selected && !excluded
def restored (selected excluded : Bool) : Entry :=
 if wanted selected excluded then .directory else .absent
def reset (selected excluded : Bool) (_entry : Entry) (_extra : Bool) : Entry × Bool :=
 (restored selected excluded, false)
set_option maxHeartbeats 10000 in
theorem idempotent (selected excluded : Bool) (entry : Entry) (extra : Bool) :
 reset selected excluded (reset selected excluded entry extra).1
   (reset selected excluded entry extra).2 = reset selected excluded entry extra := by rfl
set_option maxHeartbeats 10000 in
theorem excluded_absent (selected : Bool) : restored selected true = .absent := by
 cases selected <;> decide
set_option maxHeartbeats 10000 in
theorem selected_present : restored true false = .directory := by decide

def label : Entry → String
 | .absent => "absent"
 | .file => "file"
 | .directory => "directory"
def render (selected excluded : Bool) (entry : Entry) (extra : Bool) : Json := Json.mkObj [
 ("schema",toJson (1:Nat)),("mode",toJson "strict"),
 ("id",toJson s!"{selected}-{excluded}-{label entry}-{extra}"),
 ("selected",toJson selected),("excluded",toJson excluded),
 ("initial",toJson (label entry)),("extra",toJson extra),
 ("directory",toJson (wanted selected excluded)),("extra_after",toJson false)]
def corpus : String := String.join <|
 [false,true].flatMap fun selected => [false,true].flatMap fun excluded =>
 [Entry.absent,.file,.directory].flatMap fun entry =>
 [false,true].map fun extra => (render selected excluded entry extra).compress ++ "\n"
def sensitivity : Bool :=
 (Entry.absent != restored true false) &&
 (Entry.directory != restored true true) &&
 ((reset true false .file true).2 != true)
end HoiminOracle.EmptyDirectory
