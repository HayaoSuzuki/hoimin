namespace HoiminOracle.PathOrder

/-- Order of canonical slash-separated path keys in `Utf8PathBuf` maps. -/
def le (left right : String) : Bool :=
  (left.splitOn "/").compareLex compare (right.splitOn "/") != .gt

theorem compare_after_shared_ancestors (ancestors left right : List String) :
    (ancestors ++ left).compareLex compare (ancestors ++ right) = left.compareLex compare right := by
  induction ancestors with
  | nil => rfl
  | cons name rest ih => simp [List.compareLex, ih]

theorem ancestor_precedes_descendant (ancestor : List String) (name : String) (rest : List String) :
    ancestor.compareLex compare (ancestor ++ name :: rest) = .lt := by
  simpa [List.compareLex] using compare_after_shared_ancestors ancestor [] (name :: rest)

end HoiminOracle.PathOrder
