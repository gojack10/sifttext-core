# Checked equivalent validation computation; frozen Spec owns graph meaning.
import Base
import ../../Spec.bend as S
import ./ParentTable.bend as P

def nodes_valid(xs: List<&2, S.Node>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>, +keys: List<&2, Nat>, +fuel: Nat, +table: List<&2, Nat>) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case S.Node{+i, p, +c, r} <> t:
      Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(r, keys)
        && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(ns, i)), 1n)
        && P.rooted(fuel, i, table, roots) && nodes_valid(t, ns, roots, keys, fuel, table)

def valid(g: S.Graph) -> Bool:
  match g:
    case S.Graph{+ns, +roots, assets}:
      +keys = S.ids(ns)
      S.unique(keys) && S.unique(roots) && S.members(roots, keys)
        && nodes_valid(ns, ns, roots, keys, List.length(&2, S.Node, ns), P.build(ns))

law nodes_valid_equivalent:
  for +xs: List<&2, S.Node>
  for +ns: List<&2, S.Node>
  for +roots: List<&2, Nat>
  for +keys: List<&2, Nat>
  for +fuel: Nat
  {nodes_valid(xs, ns, roots, keys, fuel, P.build(ns)) == S.nodes_valid(xs, ns, roots, keys, fuel) : Bool}

def nodes_valid_equivalent(xs, ns, roots, keys, fuel):
  match xs:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %P.rooted_equivalent(fuel, i, ns, roots) :
        {nodes_valid(S.Node{i, p, c, r} <> t, ns, roots, keys, fuel, P.build(ns))
          == Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(r, keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(ns, i)), 1n)
            && _ && S.nodes_valid(t, ns, roots, keys, fuel) : Bool}
      %nodes_valid_equivalent(t, ns, roots, keys, fuel) :
        {Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(r, keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(ns, i)), 1n)
            && P.rooted(fuel, i, P.build(ns), roots) && nodes_valid(t, ns, roots, keys, fuel, P.build(ns))
          == Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(r, keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(ns, i)), 1n)
            && P.rooted(fuel, i, P.build(ns), roots) && _ : Bool}
      {==}

law valid_equivalent:
  for +g: S.Graph
  {valid(g) == S.valid(g) : Bool}

def valid_equivalent(g):
  match g:
    case S.Graph{+ns, +roots, assets}:
      %nodes_valid_equivalent(ns, ns, roots, S.ids(ns), List.length(&2, S.Node, ns)) :
        {valid(S.Graph{ns, roots, assets})
          == S.unique(S.ids(ns)) && S.unique(roots) && S.members(roots, S.ids(ns)) && _ : Bool}
      {==}
