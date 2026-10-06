import Std
namespace ContainerElementDelete
inductive Kind where | list | tuple | dict deriving DecidableEq
structure Container (α : Type) where
  kind : Kind
  entries : List α

def removeOne (c : Container α) (n : Nat) : Container α :=
  ⟨c.kind, c.entries.take n ++ c.entries.drop (n + 1)⟩
theorem kind_preserved (c : Container α) (n : Nat) : (removeOne c n).kind = c.kind := rfl
theorem retained_order (c : Container α) (n : Nat) :
    (removeOne c n).entries = c.entries.take n ++ c.entries.drop (n + 1) := rfl
theorem singleton_empty (kind : Kind) (a : α) :
    (removeOne ⟨kind, [a]⟩ 0).entries = [] := rfl
theorem two_tuple_stays_tuple (a b : α) :
    (removeOne ⟨.tuple, [a,b]⟩ 0).kind = .tuple := rfl
theorem one_fewer (c : Container α) (n : Nat) (h : n < c.entries.length) :
    (removeOne c n).entries.length + 1 = c.entries.length := by
  simp [removeOne, List.length_take, List.length_drop]
  omega
-- Dict entries are pairs and are removed together.
example : (removeOne ⟨.dict, [("user_id",7),("enabled",1)]⟩ 1).entries = [("user_id",7)] := rfl
example : (removeOne ⟨.tuple, [1,2]⟩ 0).kind ≠ .list := by decide
end ContainerElementDelete
