import Base
import ../../Spec.bend as S
import ./Delta.bend as D

def or_false_left(+a: Bool, +b: Bool, empty: {a || b == False{} : Bool}) -> {a == False{} : Bool}:
  match a:
    case False{}:
      {==}
    case True{}:
      empty

def or_false_right(+a: Bool, +b: Bool, empty: {a || b == False{} : Bool}) -> {b == False{} : Bool}:
  match a:
    case False{}:
      empty
    case True{}:
      Empty.absurd({b == False{} : Bool}, D.false_true(Equal.sym(Bool, True{}, False{}, empty)))

def nat_equal_refl(+x: Nat) -> {Nat.is_eq(x, x) == True{} : Bool}:
  match x:
    case 0n:
      {==}
    case 1n+p:
      nat_equal_refl(p)

def count_absent(+xs: List<&2, Nat>, +x: Nat, absent: {S.member(xs, x) == False{} : Bool}) -> {S.count(xs, x) == 0n : Nat}:
  match xs:
    case Nil{}:
      {==}
    case +h <> +t:
      D.duplicate_for(Bool, S.member(h <> t, x), False{}, absent, {S.count(h <> t, x) == 0n : Nat}, head_no => tail_no =>
        Equal.trans(Nat, S.count(h <> t, x), S.count(t, x), 0n,
          Equal.cong(Bool, Nat, b => Nat.add(U32.to_nat(Bool.to_u32(b)), S.count(t, x)), Nat.is_eq(h, x), False{},
            or_false_left(Nat.is_eq(h, x), S.member(t, x), head_no)),
          count_absent(t, x, or_false_right(Nat.is_eq(h, x), S.member(t, x), tail_no))))

def parent_member(+ns: List<&2, S.Node>, +x: Nat, one: {S.parents(ns, x) == 1n : Nat}) ->
  {S.member(S.ids(ns), S.parent(ns, x)) == True{} : Bool}:
  match ns:
    case Nil{}:
      Empty.absurd({S.member(S.ids([]), S.parent([], x)) == True{} : Bool},
        D.successor_zero(0n, Equal.sym(Nat, 0n, 1n, one)))
    case S.Node{+i, p, +children, refs} <> +t:
      D.bool_cases(S.member(children, x), S.member(S.ids(S.Node{i, p, children, refs} <> t), S.parent(S.Node{i, p, children, refs} <> t, x)),
        no => D.duplicate_bool(S.member(children, x), False{}, no,
          S.member(S.ids(S.Node{i, p, children, refs} <> t), S.parent(S.Node{i, p, children, refs} <> t, x)), parent_no => count_no =>
          D.true_of_equal(S.member(S.ids(S.Node{i, p, children, refs} <> t), S.parent(S.Node{i, p, children, refs} <> t, x)),
            S.member(i <> S.ids(t), S.parent(t, x)),
            Equal.cong(Bool, Bool, b => S.member(i <> S.ids(t), S.pick_parent(i, S.parent(t, x), b)), S.member(children, x), False{}, parent_no),
            D.or_right_true(Nat.is_eq(i, S.parent(t, x)), S.member(S.ids(t), S.parent(t, x)),
              parent_member(t, x,
                Equal.trans(Nat, S.parents(t, x), S.parents(S.Node{i, p, children, refs} <> t, x), 1n,
                  Equal.sym(Nat, S.parents(S.Node{i, p, children, refs} <> t, x), S.parents(t, x),
                    Equal.cong(Nat, Nat, n => Nat.add(n, S.parents(t, x)), S.count(children, x), 0n, count_absent(children, x, count_no))), one))))),
        yes => D.true_of_equal(S.member(S.ids(S.Node{i, p, children, refs} <> t), S.parent(S.Node{i, p, children, refs} <> t, x)),
          S.member(i <> S.ids(t), i),
          Equal.cong(Bool, Bool, b => S.member(i <> S.ids(t), S.pick_parent(i, S.parent(t, x), b)), S.member(children, x), True{}, yes),
          D.or_left_true(Nat.is_eq(i, i), S.member(S.ids(t), i), nat_equal_refl(i))))

law placement_closed:
  for +ns: List<&2, S.Node>
  for +roots: List<&2, Nat>
  for +x: Nat
  for placed: {Nat.is_eq(Nat.add(S.count(roots, x), S.parents(ns, x)), 1n) == True{} : Bool}
  {S.member(roots, x) || S.member(S.ids(ns), S.parent(ns, x)) == True{} : Bool}
def placement_closed(ns, roots, x, placed):
  D.bool_cases(S.member(roots, x), S.member(roots, x) || S.member(S.ids(ns), S.parent(ns, x)),
    no => D.or_right_true(S.member(roots, x), S.member(S.ids(ns), S.parent(ns, x)),
      parent_member(ns, x,
        Equal.trans(Nat, S.parents(ns, x), Nat.add(S.count(roots, x), S.parents(ns, x)), 1n,
          Equal.sym(Nat, Nat.add(S.count(roots, x), S.parents(ns, x)), S.parents(ns, x),
            Equal.cong(Nat, Nat, n => Nat.add(n, S.parents(ns, x)), S.count(roots, x), 0n, count_absent(roots, x, no))),
          D.nat_equal(Nat.add(S.count(roots, x), S.parents(ns, x)), 1n, placed)))),
    yes => D.or_left_true(S.member(roots, x), S.member(S.ids(ns), S.parent(ns, x)), yes))
