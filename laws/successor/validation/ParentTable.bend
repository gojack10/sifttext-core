# Direct parent table; ParentTableGraph guards sparse raw-wire child identities.
# ponytail: unary direct addressing uses O(max identity) space and O(identity)
# lookup. A proved binary trie is needed before claiming logarithmic indexing.
import Base
import ../../Spec.bend as S
import ./Rooted.bend as R
import ./Equivalence.bend as E

def parent(m: List<&2, Nat>, x: Nat) -> Nat:
  match m x:
    case Nil{} _:
      0n
    case h <> t 0n:
      h
    case h <> t 1n+p:
      parent(t, p)

def put(m: List<&2, Nat>, x: Nat, i: Nat) -> List<&2, Nat>:
  match m x:
    case Nil{} 0n:
      [i]
    case Nil{} 1n+p:
      0n <> put([], p, i)
    case h <> t 0n:
      i <> t
    case h <> t 1n+p:
      h <> put(t, p, i)

law parent_empty:
  for +x: Nat
  {parent([], x) == 0n : Nat}
def parent_empty(x):
  {==}

law parent_put:
  for +m: List<&2, Nat>
  for +x: Nat
  for +i: Nat
  for +y: Nat
  {parent(put(m, x, i), y) == S.pick_parent(i, parent(m, y), Nat.is_eq(x, y)) : Nat}
def parent_put(m, x, i, y):
  match m x y:
    case Nil{} 0n 0n:
      {==}
    case Nil{} 0n 1n+yp:
      {==}
    case Nil{} 1n+xp 0n:
      {==}
    case Nil{} 1n+xp 1n+yp:
      %parent_put([], xp, i, yp) :
        {parent(put([], xp, i), yp) == _ : Nat}
      {==}
    case h <> t 0n 0n:
      {==}
    case h <> t 0n 1n+yp:
      {==}
    case h <> t 1n+xp 0n:
      {==}
    case h <> +t 1n+xp 1n+yp:
      %parent_put(t, xp, i, yp) :
        {parent(put(t, xp, i), yp) == _ : Nat}
      {==}

def children(cs: List<&2, Nat>, +i: Nat, m: List<&2, Nat>) -> List<&2, Nat>:
  match cs:
    case Nil{}:
      m
    case x <> t:
      put(children(t, i, m), x, i)

law pick_same:
  for +i: Nat
  for +fallback: Nat
  for +a: Bool
  for +b: Bool
  {S.pick_parent(i, S.pick_parent(i, fallback, b), a) == S.pick_parent(i, fallback, Bool.or(a, b)) : Nat}
def pick_same(i, fallback, a, b):
  match a:
    case False{}:
      {==}
    case True{}:
      {==}

law parent_children:
  for +cs: List<&2, Nat>
  for +i: Nat
  for +m: List<&2, Nat>
  for +y: Nat
  {parent(children(cs, i, m), y) == S.pick_parent(i, parent(m, y), S.member(cs, y)) : Nat}
def parent_children(cs, i, m, y):
  match cs:
    case Nil{}:
      {==}
    case +x <> +t:
      %pick_same(i, parent(m, y), Nat.is_eq(x, y), S.member(t, y)) :
        {parent(children(x <> t, i, m), y) == _ : Nat}
      %parent_children(t, i, m, y) :
        {parent(children(x <> t, i, m), y)
          == S.pick_parent(i, _, Nat.is_eq(x, y)) : Nat}
      %parent_put(children(t, i, m), x, i, y) :
        {parent(children(x <> t, i, m), y) == _ : Nat}
      {==}

def build(ns: List<&2, S.Node>) -> List<&2, Nat>:
  match ns:
    case Nil{}:
      []
    case S.Node{i, p, c, r} <> t:
      children(c, i, build(t))

law parent_equivalent:
  for +ns: List<&2, S.Node>
  for +x: Nat
  {parent(build(ns), x) == S.parent(ns, x) : Nat}
def parent_equivalent(ns, x):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, r} <> +t:
      %parent_equivalent(t, x) :
        {parent(build(S.Node{i, p, c, r} <> t), x)
          == S.pick_parent(i, _, S.member(c, x)) : Nat}
      %parent_children(c, i, build(t), x) :
        {parent(build(S.Node{i, p, c, r} <> t), x) == _ : Nat}
      {==}

def rooted(fuel: Nat, +x: Nat, +m: List<&2, Nat>, +roots: List<&2, Nat>) -> Bool:
  match fuel:
    case 0n:
      S.member(roots, x)
    case 1n+f:
      R.or_delayed(S.member(roots, x), unused => rooted(f, parent(m, x), m, roots))

law rooted_equivalent:
  for +fuel: Nat
  for +x: Nat
  for +ns: List<&2, S.Node>
  for +roots: List<&2, Nat>
  {rooted(fuel, x, build(ns), roots) == S.rooted(fuel, x, ns, roots) : Bool}
def rooted_equivalent(fuel, x, ns, roots):
  match fuel:
    case 0n:
      {==}
    case 1n+f:
      %E.delayed_or_equivalent(S.member(roots, x), unused => S.rooted(f, S.parent(ns, x), ns, roots)) :
        {rooted(1n+f, x, build(ns), roots) == _ : Bool}
      %rooted_equivalent(f, S.parent(ns, x), ns, roots) :
        {rooted(1n+f, x, build(ns), roots)
          == R.or_delayed(S.member(roots, x), unused => _) : Bool}
      %parent_equivalent(ns, x) :
        {rooted(1n+f, x, build(ns), roots)
          == R.or_delayed(S.member(roots, x), unused => rooted(f, _, build(ns), roots)) : Bool}
      {==}
