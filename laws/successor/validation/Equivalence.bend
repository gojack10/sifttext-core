import Base
import ../../Spec.bend as S
import ./Rooted.bend as R

law delayed_or_equivalent:
  for done: Bool
  for next: Unit -> Bool
  {R.or_delayed(done, next) == Bool.or(done, next(Unit{})) : Bool}

def delayed_or_equivalent(done, next):
  match done:
    case False{}:
      {==}
    case True{}:
      {==}

law delayed_parent_equivalent:
  for i: Nat
  for fallback: Unit -> Nat
  for yes: Bool
  {R.pick_parent(i, fallback, yes) == S.pick_parent(i, fallback(Unit{}), yes) : Nat}

def delayed_parent_equivalent(i, fallback, yes):
  match yes:
    case False{}:
      {==}
    case True{}:
      {==}

law parent_equivalent:
  for +ns: List<&2, S.Node>
  for +x: Nat
  {R.parent(ns, x) == S.parent(ns, x) : Nat}

def parent_equivalent(ns, x):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, r} <> +t:
      %parent_equivalent(t, x) :
        {R.pick_parent(i, unused => R.parent(t, x), S.member(c, x))
          == S.pick_parent(i, _, S.member(c, x)) : Nat}
      %delayed_parent_equivalent(i, unused => R.parent(t, x), S.member(c, x)) :
        {R.pick_parent(i, unused => R.parent(t, x), S.member(c, x)) == _ : Nat}
      {==}

law rooted_equivalent:
  for +fuel: Nat
  for +x: Nat
  for +ns: List<&2, S.Node>
  for +roots: List<&2, Nat>
  {R.rooted(fuel, x, ns, roots) == S.rooted(fuel, x, ns, roots) : Bool}

def rooted_equivalent(fuel, x, ns, roots):
  match fuel:
    case 0n:
      {==}
    case 1n+f:
      %parent_equivalent(ns, x) :
        {R.or_delayed(S.member(roots, x), unused => R.rooted(f, R.parent(ns, x), ns, roots))
          == Bool.or(S.member(roots, x), S.rooted(f, _, ns, roots)) : Bool}
      %rooted_equivalent(f, R.parent(ns, x), ns, roots) :
        {R.or_delayed(S.member(roots, x), unused => R.rooted(f, R.parent(ns, x), ns, roots))
          == Bool.or(S.member(roots, x), _) : Bool}
      %delayed_or_equivalent(S.member(roots, x), unused => R.rooted(f, R.parent(ns, x), ns, roots)) :
        {R.or_delayed(S.member(roots, x), unused => R.rooted(f, R.parent(ns, x), ns, roots)) == _ : Bool}
      {==}
