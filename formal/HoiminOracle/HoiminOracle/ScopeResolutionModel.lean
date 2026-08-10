import Std

namespace HoiminOracle.ScopeResolution

inductive RuntimeBinding
  | builtin
  | shadowed
  deriving Repr, DecidableEq, BEq

inductive Knowledge
  | builtin
  | shadowed
  | unknown
  deriving Repr, DecidableEq, BEq

def Knowledge.admits : Knowledge → RuntimeBinding → Prop
  | .builtin, actual => actual = .builtin
  | .shadowed, actual => actual = .shadowed
  | .unknown, _ => True

def Knowledge.allows : Knowledge → Bool
  | .builtin => true
  | .shadowed | .unknown => false

inductive BindingFact
  | absent
  | bound
  | unknown
  deriving Repr, DecidableEq, BEq

inductive ScopeKind
  | module
  | function
  | class
  | comprehension
  deriving Repr, DecidableEq, BEq

inductive Directive
  | normal
  | global
  | nonlocal
  deriving Repr, DecidableEq, BEq

structure Frame where
  id : Nat
  kind : ScopeKind
  directive : Directive := .normal
  before : BindingFact := .absent
  whole : BindingFact := .absent
  deriving Repr, DecidableEq, BEq

def factResolution (fact : BindingFact) (fallback : Knowledge) : Knowledge :=
  match fact with
  | .absent => fallback
  | .bound => .shadowed
  | .unknown => .unknown

mutual
  def resolveModuleWhole : List Frame → Knowledge
    | [] => .builtin
    | frame :: rest =>
        if frame.kind == .module then
          factResolution frame.whole .builtin
        else
          resolveModuleWhole rest

  def resolveNonlocal : List Frame → Knowledge
    | [] => .unknown
    | frame :: rest =>
        match frame.kind with
        | .function | .comprehension =>
            match frame.whole with
            | .bound => .shadowed
            | .unknown => .unknown
            | .absent => resolveNonlocal rest
        | .module | .class => resolveNonlocal rest

  def resolveFrom : List Frame → Bool → Knowledge
    | [], _ => .builtin
    | frame :: rest, direct =>
        match frame.kind with
        | .module =>
            factResolution (if direct then frame.before else frame.whole) .builtin
        | .function | .comprehension =>
            match frame.directive with
            | .global => resolveModuleWhole rest
            | .nonlocal => resolveNonlocal rest
            | .normal => factResolution frame.whole (resolveFrom rest false)
        | .class =>
            match frame.directive with
            | .global => resolveModuleWhole rest
            | .nonlocal => resolveNonlocal rest
            | .normal =>
                if direct then
                  factResolution frame.before (resolveFrom rest false)
                else
                  resolveFrom rest false
end

structure Candidate where
  path : List Frame
  unrelatedSiblings : List Frame := []
  deriving Repr, DecidableEq, BEq

def resolve (candidate : Candidate) : Knowledge :=
  resolveFrom candidate.path true

def moduleFrame (before whole : BindingFact) : Frame where
  id := 0
  kind := .module
  before
  whole

def functionFrame
    (id : Nat)
    (whole : BindingFact)
    (directive : Directive := .normal) : Frame where
  id
  kind := .function
  directive
  whole

def classFrame
    (id : Nat)
    (before whole : BindingFact)
    (directive : Directive := .normal) : Frame where
  id
  kind := .class
  directive
  before
  whole

def comprehensionFrame (id : Nat) (whole : BindingFact) : Frame where
  id
  kind := .comprehension
  whole

end HoiminOracle.ScopeResolution
