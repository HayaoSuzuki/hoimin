import Std

namespace HoiminOracle.CleanupCapability

inductive Strategy where
  | retained
  | postInspection
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

inductive Binding where
  | owned
  | outsideLink
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

inductive Phase where
  | idle
  | inspected
  | bound
  | complete
  | rejected
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

inductive Event where
  | inspect
  | bind
  | swap
  | effect
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

structure State where
  strategy : Strategy
  binding : Binding := .owned
  phase : Phase := .idle
  observedOwned : Bool := false
  handleBound : Bool := false
  ownedWritable : Bool := false
  outsideWritable : Bool := false
  deriving BEq, ReflBEq, LawfulBEq, DecidableEq, Repr

def State.initial (strategy : Strategy) : State :=
  { strategy, handleBound := strategy == .retained }

def inspectState (state : State) : State :=
  if state.phase == .idle then
    { state with
      phase := .inspected
      observedOwned := state.binding == .owned }
  else state

def bindState (state : State) : State :=
  if state.phase != .inspected then state
  else if !state.observedOwned then { state with phase := .complete }
  else match state.strategy with
    | .retained => { state with phase := .bound }
    | .postInspection =>
        if state.binding == .owned then
          { state with phase := .bound, handleBound := true }
        else { state with phase := .rejected }

def effectState (state : State) : State :=
  if state.phase == .bound && state.handleBound then
    { state with phase := .complete, ownedWritable := true }
  else state

def step (state : State) : Event → State
  | .inspect => inspectState state
  | .bind => bindState state
  | .swap => { state with binding := .outsideLink }
  | .effect => effectState state

def brokenBindState (state : State) : State :=
  if state.phase != .inspected then state
  else if state.observedOwned then { state with phase := .bound }
  else { state with phase := .complete }

def brokenEffectState (state : State) : State :=
  if state.phase != .bound then state
  else match state.binding with
    | .owned => { state with phase := .complete, ownedWritable := true }
    | .outsideLink =>
        { state with phase := .complete, outsideWritable := true }

def brokenStep (state : State) : Event → State
  | .inspect => inspectState state
  | .bind => brokenBindState state
  | .swap => { state with binding := .outsideLink }
  | .effect => brokenEffectState state

def runWith (next : State → Event → State) : State → List Event → State
  | state, [] => state
  | state, event :: rest => runWith next (next state event) rest

def run (strategy : Strategy) (events : List Event) : State :=
  runWith step (State.initial strategy) events

def brokenRun (strategy : Strategy) (events : List Event) : State :=
  runWith brokenStep (State.initial strategy) events

end HoiminOracle.CleanupCapability
