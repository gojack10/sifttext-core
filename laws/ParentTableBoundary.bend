import Base
import ./Spec.bend as S
import ./successor/validation/ParentTable.bend as P

def parent(ns: List<&2, S.Node>, x: Nat) -> Nat:
  P.parent(P.build(ns), x)

def rooted(fuel: Nat, x: Nat, ns: List<&2, S.Node>, roots: List<&2, Nat>) -> Bool:
  P.rooted(fuel, x, P.build(ns), roots)

law zero_first_parent:
  {parent([S.Node{0n, 0n, [7n], []}, S.Node{9n, 0n, [7n], []}], 7n) == 0n : Nat}
def zero_first_parent():
  {==}

law first_parent_order:
  {parent([S.Node{9n, 0n, [7n], []}, S.Node{0n, 0n, [7n], []}], 7n) == 9n : Nat}
def first_parent_order():
  {==}

law repeated_child_same_parent:
  {parent([S.Node{9n, 0n, [7n, 7n], []}], 7n) == 9n : Nat}
def repeated_child_same_parent():
  {==}

law missing_parent_zero_root:
  {rooted(1n, 7n, [], [0n]) == True{} : Bool}
def missing_parent_zero_root():
  {==}

law fuel_zero_checks_current_only:
  {rooted(0n, 7n, [S.Node{9n, 0n, [7n], []}], [9n]) == False{} : Bool}
def fuel_zero_checks_current_only():
  {==}

law fuel_one_reaches_parent:
  {rooted(1n, 7n, [S.Node{9n, 0n, [7n], []}], [9n]) == True{} : Bool}
def fuel_one_reaches_parent():
  {==}

law zero_is_queryable_child:
  {parent([S.Node{9n, 0n, [0n], []}], 0n) == 9n : Nat}
def zero_is_queryable_child():
  {==}

law missing_parent_can_continue_through_zero:
  {rooted(2n, 7n, [S.Node{9n, 0n, [0n], []}], [9n]) == True{} : Bool}
def missing_parent_can_continue_through_zero():
  {==}

law duplicate_node_ids_keep_edges:
  {parent([S.Node{9n, 0n, [], []}, S.Node{9n, 0n, [7n], []}], 7n) == 9n : Nat}
def duplicate_node_ids_keep_edges():
  {==}

law sparse_ids:
  {parent([S.Node{31n, 0n, [63n], []}], 63n) == 31n : Nat}
def sparse_ids():
  {==}
