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

private def brokenOmitHandlerCleanup (_target : Name) (body : Exits) : Exits :=
  body

private def brokenHandlerJoin (left _right : Env) : Env := left

private def brokenUsePrePatternFailure
    (prePattern : Env) (pattern : PatternResult) : CaseStep :=
  { body := some pattern.matched, nextCase := some prePattern }

private def brokenUsePreGuardFailure (preGuard : Env) : CaseStep :=
  { body := none, nextCase := some preGuard }

private def brokenDiscardRefutableUnmatched (completed : List Env) : Option Env :=
  finishMatch none completed

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
    brokenOmitHandlerCleanup .destination
        (.categoryOnly .fallthrough
          { source := .known .builtin, destination := .known .typing }) ≠
      cleanupHandlerExits .destination
        (.categoryOnly .fallthrough
          { source := .known .builtin, destination := .known .typing }) := by
  decide

example :
    brokenOmitHandlerCleanup .destination
        (.categoryOnly .break
          { source := .known .builtin, destination := .known .typing }) ≠
      cleanupHandlerExits .destination
        (.categoryOnly .break
          { source := .known .builtin, destination := .known .typing }) := by
  decide

example :
    brokenOmitHandlerCleanup .destination
        (.categoryOnly .continue
          { source := .known .builtin, destination := .known .typing }) ≠
      cleanupHandlerExits .destination
        (.categoryOnly .continue
          { source := .known .builtin, destination := .known .typing }) := by
  decide

example :
    brokenOmitHandlerCleanup .destination
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
    brokenUsePrePatternFailure
        { source := .known .builtin, destination := .known .typing }
        { matched :=
            { source := .shadowed, destination := .known .typing }
          failed := some
            { source := .known .builtin, destination := .shadowed } } ≠
      advanceCase
        { matched :=
            { source := .shadowed, destination := .known .typing }
          failed := some
            { source := .known .builtin, destination := .shadowed } }
        none := by
  decide

example :
    brokenUsePreGuardFailure
        { source := .known .builtin, destination := .known .typing } ≠
      advanceCase
        { matched :=
            { source := .known .builtin, destination := .known .typing }
          failed := none }
        (some false)
        { source := .known .builtin, destination := .shadowed } := by
  decide

example :
    brokenDiscardRefutableUnmatched
        [{ source := .known .builtin, destination := .known .typing }] ≠
      finishMatch
        (some { source := .known .builtin, destination := .shadowed })
        [{ source := .known .builtin, destination := .known .typing }] := by
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
