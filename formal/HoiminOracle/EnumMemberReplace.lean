import Std
namespace EnumMemberReplace

-- Value-group identifiers are established by the Rust literal/auto index.
-- A canonical list contains exactly one representative per identifier.
def destinations (source : Nat) (canonical : List Nat) : List Nat :=
  canonical.filter (fun value => value != source)

theorem no_same_value (source : Nat) (canonical : List Nat) :
    source ∉ destinations source canonical := by
  simp [destinations]

theorem destination_exists (source value : Nat) (canonical : List Nat)
    (h : value ∈ destinations source canonical) : value ∈ canonical := by
  exact (List.mem_filter.mp h).1

theorem no_alias_inflation (source : Nat) (canonical : List Nat)
    (h : canonical.Nodup) : (destinations source canonical).Nodup := by
  exact h.filter _

theorem count_bound (source : Nat) (canonical : List Nat) :
    (destinations source canonical).length ≤ canonical.length := by
  exact List.length_filter_le _ _

-- A/a in StrEnum share group 0; B is group 1.
example : destinations 0 [0, 1] = [1] := by decide
example : destinations 0 [0] = [] := by decide
example : destinations 1 [0, 1, 2] = [0, 2] := by decide
-- Broken spelling-based filtering would retain the alias of group 0.
example : [0, 1] ≠ destinations 0 [0, 1] := by decide
end EnumMemberReplace
