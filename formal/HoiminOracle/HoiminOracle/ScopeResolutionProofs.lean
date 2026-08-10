import HoiminOracle.ScopeResolutionModel

namespace HoiminOracle.ScopeResolution

theorem allowed_knowledge_is_builtin
    (knowledge : Knowledge)
    (allowed : knowledge.allows = true) :
    knowledge = .builtin := by
  cases knowledge <;> simp [Knowledge.allows] at allowed ⊢

theorem allowed_resolution_is_sound
    (candidate : Candidate)
    (actual : RuntimeBinding)
    (admitted : (resolve candidate).admits actual)
    (allowed : (resolve candidate).allows = true) :
    actual = .builtin := by
  have resolved : resolve candidate = .builtin :=
    allowed_knowledge_is_builtin (resolve candidate) allowed
  simpa [resolved, Knowledge.admits] using admitted

theorem allowed_replacement_is_sound
    (source destination : Knowledge)
    (sourceActual destinationActual : RuntimeBinding)
    (sourceAdmitted : source.admits sourceActual)
    (destinationAdmitted : destination.admits destinationActual)
    (allowed : allowsReplacement source destination = true) :
    sourceActual = .builtin ∧ destinationActual = .builtin := by
  simp only [allowsReplacement, Bool.and_eq_true] at allowed
  constructor
  · have resolved := allowed_knowledge_is_builtin source allowed.1
    simpa [resolved, Knowledge.admits] using sourceAdmitted
  · have resolved := allowed_knowledge_is_builtin destination allowed.2
    simpa [resolved, Knowledge.admits] using destinationAdmitted

theorem unrelated_sibling_does_not_change_resolution
    (candidate : Candidate)
    (siblings : List Frame) :
    resolve { candidate with unrelatedSiblings := siblings } = resolve candidate := by
  rfl

example :
    resolve {
      path := [
        functionFrame 1 .absent,
        moduleFrame .absent .absent
      ]
      unrelatedSiblings := [functionFrame 2 .bound]
    } = .builtin := by
  decide

example :
    resolve {
      path := [
        functionFrame 1 .bound,
        moduleFrame .absent .absent
      ]
    } = .shadowed := by
  decide

example :
    resolve {
      path := [
        functionFrame 2 .absent,
        functionFrame 1 .bound,
        moduleFrame .absent .absent
      ]
    } = .shadowed := by
  decide

example :
    resolve {
      path := [
        functionFrame 2 .absent,
        classFrame 1 .bound .bound,
        moduleFrame .absent .absent
      ]
    } = .builtin := by
  decide

example :
    resolve {
      path := [
        classFrame 1 .absent .bound,
        moduleFrame .absent .absent
      ]
    } = .builtin := by
  decide

example :
    resolve {
      path := [
        classFrame 1 .bound .bound,
        moduleFrame .absent .absent
      ]
    } = .shadowed := by
  decide

example :
    resolve {
      path := [
        comprehensionFrame 1 .bound,
        functionFrame 2 .absent,
        moduleFrame .absent .absent
      ]
    } = .shadowed := by
  decide

example :
    resolve {
      path := [
        functionFrame 2 .absent,
        moduleFrame .absent .absent
      ]
    } = .builtin := by
  decide

example :
    resolve {
      path := [
        functionFrame 1 .bound .global,
        moduleFrame .absent .bound
      ]
    } = .shadowed := by
  decide

example :
    resolve {
      path := [
        functionFrame 2 .absent .nonlocal,
        functionFrame 1 .bound,
        moduleFrame .absent .absent
      ]
    } = .shadowed := by
  decide

example :
    resolve {
      path := [
        functionFrame 1 .absent,
        moduleFrame .absent .unknown
      ]
    } = .unknown := by
  decide

example :
    (resolve {
      path := [
        functionFrame 1 .absent,
        moduleFrame .absent .unknown
      ]
    }).allows = false := by
  decide

example : allowsReplacement .builtin .shadowed = false := by
  decide

example : allowsReplacement .builtin .builtin = true := by
  decide

end HoiminOracle.ScopeResolution
