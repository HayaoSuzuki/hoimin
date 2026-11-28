namespace HoiminOracle.ChangedConfig

def maxContext : Nat := 1073741823

inductive Selector | source | file | line | symbol
  deriving DecidableEq, BEq, ReflBEq, LawfulBEq

structure Request where
  selectors : List Selector
  enabled : Bool
  context : Option Nat
  base : Option String

structure Scope where
  context : Nat
  base : Option String
  deriving DecidableEq, BEq

inductive Issue
  | invalidCliContext
  | cliContextRequiresChanged
  | missingSelector
  | changedRequiresSource
  | symbolRequiresSource
  | contextRequiresChanged
  | diffBaseRequiresChanged
  | contextTooLarge (context : Nat)
  deriving DecidableEq, BEq

def validateSelection (selectors : List Selector) (scope : Option Scope) :
    Except Issue (Option Scope) :=
  if selectors.isEmpty && scope.isNone then .error .missingSelector
  else if Selector.source ∉ selectors then
    if scope.isSome then .error .changedRequiresSource
    else if Selector.symbol ∈ selectors then .error .symbolRequiresSource
    else .ok scope
  else .ok scope

def fromRaw (request : Request) : Except Issue (Option Scope) :=
  let context := request.context.getD 0
  if context > maxContext then .error (.contextTooLarge context)
  else if request.enabled then
    validateSelection request.selectors (some ⟨context, request.base⟩)
  else if context != 0 then .error .contextRequiresChanged
  else if request.base.isSome then .error .diffBaseRequiresChanged
  else validateSelection request.selectors none

def fromCli (request : Request) : Except Issue (Option Scope) :=
  if request.context.getD 0 > maxContext then .error .invalidCliContext
  else if request.context.isSome && !request.enabled then .error .cliContextRequiresChanged
  else if request.selectors.isEmpty && !request.enabled then .error .missingSelector
  else fromRaw request

def savedScope (request : Request) : Option Scope :=
  if request.enabled then some ⟨request.context.getD 0, request.base⟩ else none

def fromSaved (selectors : List Selector) (scope : Option Scope) : Except Issue (Option Scope) :=
  if let some changed := scope then
    if changed.context > maxContext then .error (.contextTooLarge changed.context)
    else validateSelection selectors scope
  else validateSelection selectors none

theorem accepted_changed_requires_source (selectors : List Selector) (scope : Scope)
    (h : validateSelection selectors (some scope) = .ok (some scope)) :
    .source ∈ selectors := by
  by_cases source : .source ∈ selectors
  · exact source
  · simp [validateSelection, source] at h

theorem bounded_context_doubles_safely (context : Nat) (bound : context ≤ maxContext) :
    2 * context ≤ 2147483647 := by
  unfold maxContext at bound
  omega

theorem explicit_zero_requires_switch (selectors : List Selector) (base : Option String) :
    fromCli ⟨selectors, false, some 0, base⟩ = .error .cliContextRequiresChanged := rfl

end HoiminOracle.ChangedConfig
