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

end HoiminOracle.ScopeResolution
