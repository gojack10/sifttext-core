# Delayed root membership recursion, proved equivalent in Equivalence.bend.
# ponytail: deep chains remain quadratic; indexed ancestry if they dominate.
import Base
import ../../Spec.bend as S

def or_delayed(done: Bool, next: Unit -> Bool) -> Bool:
  match done:
    case True{}:
      True{}
    case False{}:
      next(Unit{})

def rooted(fuel: Nat, +x: Nat, +ns: List<&2, S.Node>, +roots: List<&2, Nat>) -> Bool:
  match fuel:
    case 0n:
      S.member(roots, x)
    case 1n+f:
      or_delayed(S.member(roots, x), unused => rooted(f, S.parent(ns, x), ns, roots))
