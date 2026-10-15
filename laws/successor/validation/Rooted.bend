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

# Return the first matching parent exactly as Spec.parent does, without evaluating
# the unused suffix when a parent is found. Invalid duplicate-parent inputs retain
# the same first-match result; complete graph validation still rejects them.
def pick_parent(i: Nat, fallback: Unit -> Nat, yes: Bool) -> Nat:
  match yes:
    case False{}:
      fallback(Unit{})
    case True{}:
      i

def parent(ns: List<&2, S.Node>, +x: Nat) -> Nat:
  match ns:
    case Nil{}:
      0n
    case S.Node{i, p, c, r} <> t:
      pick_parent(i, unused => parent(t, x), S.member(c, x))

def rooted(fuel: Nat, +x: Nat, +ns: List<&2, S.Node>, +roots: List<&2, Nat>) -> Bool:
  match fuel:
    case 0n:
      S.member(roots, x)
    case 1n+f:
      or_delayed(S.member(roots, x), unused => rooted(f, parent(ns, x), ns, roots))
