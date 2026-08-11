import HoiminOracle.ExceptionMatchBindingModel

namespace HoiminOracle.ExceptionMatchBinding

open BindingFlow

theorem handler_type_precedes_target
    (incoming : Env) (target : Name) (body : Env → Exits) :
    (observeHandler incoming target body).typeEntry = incoming := by rfl

theorem handler_body_has_target
    (incoming : Env) (target : Name) (body : Env → Exits) :
    (observeHandler incoming target body).bodyEntry.get target = .shadowed := by
  cases target <;> rfl

theorem handler_cleanup_fallthrough (target : Name) (environment : Env) :
    (cleanupHandlerExits target
      (.categoryOnly .fallthrough environment)).fallthrough =
        some (deleteName environment target) := by rfl

theorem handler_cleanup_break (target : Name) (environment : Env) :
    (cleanupHandlerExits target (.categoryOnly .break environment)).breaks =
      [deleteName environment target] := by rfl

theorem handler_cleanup_continue (target : Name) (environment : Env) :
    (cleanupHandlerExits target
      (.categoryOnly .continue environment)).continues =
        [deleteName environment target] := by rfl

theorem handler_cleanup_terminate (target : Name) (environment : Env) :
    (cleanupHandlerExits target
      (.categoryOnly .terminate environment)).terminates =
        [deleteName environment target] := by rfl

theorem handler_cleanup_preserves_other_name
    (environment : Env) (target observed : Name) (different : target ≠ observed) :
    (deleteName environment target).get observed = environment.get observed := by
  cases target <;> cases observed <;> simp_all [deleteName, Env.set, Env.get]

theorem handler_join_is_meet (left right : Env) :
    meetOption (some left) (some right) = some (left.meet right) := by rfl

theorem failed_pattern_reaches_next_case (pattern : PatternResult) :
    (advanceCase pattern none).nextCase = pattern.failed := by rfl

theorem false_guard_reaches_next_case
    (pattern : PatternResult) (afterGuard : Env) :
    (advanceCase pattern (some false) afterGuard).nextCase =
      meetOption pattern.failed (some afterGuard) := by rfl

theorem irrefutable_case_has_no_unmatched_path (matched : Env) :
    (advanceCase { matched, failed := none } none).nextCase = none := by rfl

theorem match_join_includes_refutable_unmatched
    (completed : List Env) (unmatched : Env) :
    finishMatch (some unmatched) completed =
      meetAll? (completed ++ [unmatched]) := by rfl

private def brokenBindBeforeType (incoming : Env) (target : Name)
    (body : Env → Exits) : HandlerObservation :=
  { observeHandler incoming target body with
      typeEntry := bindTarget incoming target }

private def brokenCleanupFallthroughOnly (target : Name) (body : Exits) : Exits :=
  { body with
      fallthrough := body.fallthrough.map
        (fun environment => deleteName environment target) }

private def brokenHandlerJoin (left _right : Env) : Env := left

private def brokenDiscardPatternFailure (pattern : PatternResult) : CaseStep :=
  { body := some pattern.matched, nextCase := none }

private def brokenDiscardGuardFailure
    (pattern : PatternResult) (_afterGuard : Env) : CaseStep :=
  { body := none, nextCase := pattern.failed }

private def brokenRetainIrrefutableUnmatched (incoming : Env) : Option Env :=
  some incoming

example :
    brokenBindBeforeType
        { source := .known .builtin, destination := .known .typing }
        .source Exits.fallthroughOnly ≠
      observeHandler
        { source := .known .builtin, destination := .known .typing }
        .source Exits.fallthroughOnly := by
  decide

example :
    brokenCleanupFallthroughOnly .destination
        (.categoryOnly .terminate
          { source := .known .builtin, destination := .known .typing }) ≠
      cleanupHandlerExits .destination
        (.categoryOnly .terminate
          { source := .known .builtin, destination := .known .typing }) := by
  decide

example :
    brokenHandlerJoin
        { source := .known .builtin, destination := .known .typing }
        { source := .shadowed, destination := .known .typing } ≠
      ({ source := .known .builtin, destination := .known .typing } : Env).meet
        { source := .shadowed, destination := .known .typing } := by
  decide

example :
    brokenDiscardPatternFailure
        { matched :=
            { source := .shadowed, destination := .known .typing }
          failed := some
            { source := .known .builtin, destination := .known .typing } } ≠
      advanceCase
        { matched :=
            { source := .shadowed, destination := .known .typing }
          failed := some
            { source := .known .builtin, destination := .known .typing } }
        none := by
  decide

example :
    brokenDiscardGuardFailure
        { matched :=
            { source := .shadowed, destination := .known .typing }
          failed := some
            { source := .known .builtin, destination := .known .typing } }
        { source := .shadowed, destination := .known .typing } ≠
      advanceCase
        { matched :=
            { source := .shadowed, destination := .known .typing }
          failed := some
            { source := .known .builtin, destination := .known .typing } }
        (some false)
        { source := .shadowed, destination := .known .typing } := by
  decide

example :
    brokenRetainIrrefutableUnmatched
        { source := .known .builtin, destination := .known .typing } ≠
      (advanceCase
        { matched :=
            { source := .known .builtin, destination := .known .typing }
          failed := none }
        none).nextCase := by
  decide

end HoiminOracle.ExceptionMatchBinding
