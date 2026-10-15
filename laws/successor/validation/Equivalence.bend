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
      %rooted_equivalent(f, S.parent(ns, x), ns, roots) :
        {R.or_delayed(S.member(roots, x), unused => R.rooted(f, S.parent(ns, x), ns, roots))
          == Bool.or(S.member(roots, x), _) : Bool}
      %delayed_or_equivalent(S.member(roots, x), unused => R.rooted(f, S.parent(ns, x), ns, roots)) :
        {R.or_delayed(S.member(roots, x), unused => R.rooted(f, S.parent(ns, x), ns, roots)) == _ : Bool}
      {==}
