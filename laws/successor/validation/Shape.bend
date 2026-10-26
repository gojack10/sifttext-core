# Validation ignores payload/assets and only observes reference reasons as nonzero.
import Base
import ../../Spec.bend as S

def reason(n: Nat) -> Nat:
  match n:
    case 0n:
      0n
    case 1n+k:
      1n

def reasons(rs: List<&2, S.Ref>) -> List<&2, S.Ref>:
  match rs:
    case Nil{}:
      Nil{}
    case S.Ref{target, why} <> t:
      S.Ref{target, reason(why)} <> reasons(t)

law reason_nonzero:
  for +n: Nat
  {Nat.is_ne(reason(n), 0n) == Nat.is_ne(n, 0n) : Bool}
def reason_nonzero(n):
  match n:
    case 0n:
      {==}
    case 1n+k:
      {==}

law reasons_valid:
  for +rs: List<&2, S.Ref>
  for +keys: List<&2, Nat>
  {S.refs_valid(reasons(rs), keys) == S.refs_valid(rs, keys) : Bool}
def reasons_valid(rs, keys):
  match rs:
    case Nil{}:
      {==}
    case S.Ref{+target, +why} <> +t:
      %reason_nonzero(why) :
        {S.refs_valid(reasons(S.Ref{target, why} <> t), keys)
          == S.member(keys, target) && _ && S.refs_valid(t, keys) : Bool}
      %reasons_valid(t, keys) :
        {S.member(keys, target) && Nat.is_ne(reason(why), 0n) && S.refs_valid(reasons(t), keys)
          == S.member(keys, target) && Nat.is_ne(reason(why), 0n) && _ : Bool}
      {==}

def nodes(ns: List<&2, S.Node>) -> List<&2, S.Node>:
  match ns:
    case Nil{}:
      Nil{}
    case S.Node{i, p, c, r} <> t:
      S.Node{i, 0n, c, reasons(r)} <> nodes(t)

def erase(g: S.Graph) -> S.Graph:
  match g:
    case S.Graph{ns, roots, assets}:
      S.Graph{nodes(ns), roots, 0n}

law ids:
  for +ns: List<&2, S.Node>
  {S.ids(nodes(ns)) == S.ids(ns) : List<&2, Nat>}
def ids(ns):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %ids(t) : {S.ids(nodes(S.Node{i, p, c, r} <> t)) == i <> _ : List<&2, Nat>}
      {==}

law length:
  for +ns: List<&2, S.Node>
  {List.length(&2, S.Node, nodes(ns)) == List.length(&2, S.Node, ns) : Nat}
def length(ns):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %length(t) : {List.length(&2, S.Node, nodes(S.Node{i, p, c, r} <> t)) == 1n+_ : Nat}
      {==}

law parents:
  for +ns: List<&2, S.Node>
  for +x: Nat
  {S.parents(nodes(ns), x) == S.parents(ns, x) : Nat}
def parents(ns, x):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %parents(t, x) : {S.parents(nodes(S.Node{i, p, c, r} <> t), x) == Nat.add(S.count(c, x), _) : Nat}
      {==}

law parent:
  for +ns: List<&2, S.Node>
  for +x: Nat
  {S.parent(nodes(ns), x) == S.parent(ns, x) : Nat}
def parent(ns, x):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %parent(t, x) : {S.parent(nodes(S.Node{i, p, c, r} <> t), x) == S.pick_parent(i, _, S.member(c, x)) : Nat}
      {==}

law rooted:
  for +fuel: Nat
  for +x: Nat
  for +ns: List<&2, S.Node>
  for +roots: List<&2, Nat>
  {S.rooted(fuel, x, nodes(ns), roots) == S.rooted(fuel, x, ns, roots) : Bool}
def rooted(fuel, x, ns, roots):
  match fuel:
    case 0n:
      {==}
    case 1n+f:
      %parent(ns, x) : {S.rooted(1n+f, x, nodes(ns), roots) == S.member(roots, x) || S.rooted(f, _, ns, roots) : Bool}
      %rooted(f, S.parent(nodes(ns), x), ns, roots) :
        {S.member(roots, x) || S.rooted(f, S.parent(nodes(ns), x), nodes(ns), roots)
          == S.member(roots, x) || _ : Bool}
      {==}

law nodes_valid:
  for +xs: List<&2, S.Node>
  for +ns: List<&2, S.Node>
  for +roots: List<&2, Nat>
  for +keys: List<&2, Nat>
  for +fuel: Nat
  {S.nodes_valid(nodes(xs), nodes(ns), roots, keys, fuel) == S.nodes_valid(xs, ns, roots, keys, fuel) : Bool}
def nodes_valid(xs, ns, roots, keys, fuel):
  match xs:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %reasons_valid(r, keys) :
        {S.nodes_valid(nodes(S.Node{i, p, c, r} <> t), nodes(ns), roots, keys, fuel)
          == Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && _
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(ns, i)), 1n)
            && S.rooted(fuel, i, ns, roots) && S.nodes_valid(t, ns, roots, keys, fuel) : Bool}
      %parents(ns, i) :
        {S.nodes_valid(nodes(S.Node{i, p, c, r} <> t), nodes(ns), roots, keys, fuel)
          == Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(reasons(r), keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), _), 1n)
            && S.rooted(fuel, i, ns, roots) && S.nodes_valid(t, ns, roots, keys, fuel) : Bool}
      %rooted(fuel, i, ns, roots) :
        {Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(reasons(r), keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(nodes(ns), i)), 1n)
            && S.rooted(fuel, i, nodes(ns), roots) && S.nodes_valid(nodes(t), nodes(ns), roots, keys, fuel)
          == Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(reasons(r), keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(nodes(ns), i)), 1n)
            && _ && S.nodes_valid(t, ns, roots, keys, fuel) : Bool}
      %nodes_valid(t, ns, roots, keys, fuel) :
        {Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(reasons(r), keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(nodes(ns), i)), 1n)
            && S.rooted(fuel, i, nodes(ns), roots) && S.nodes_valid(nodes(t), nodes(ns), roots, keys, fuel)
          == Nat.is_ne(i, 0n) && S.unique(c) && S.members(c, keys) && S.refs_valid(reasons(r), keys)
            && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(nodes(ns), i)), 1n)
            && S.rooted(fuel, i, nodes(ns), roots) && _ : Bool}
      {==}

law valid:
  for +g: S.Graph
  {S.valid(erase(g)) == S.valid(g) : Bool}
def valid(g):
  match g:
    case S.Graph{+ns, +roots, assets}:
      %ids(ns) :
        {S.valid(erase(S.Graph{ns, roots, assets}))
          == S.unique(_) && S.unique(roots) && S.members(roots, S.ids(ns))
            && S.nodes_valid(ns, ns, roots, S.ids(ns), List.length(&2, S.Node, ns)) : Bool}
      %ids(ns) :
        {S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, S.ids(nodes(ns)))
            && S.nodes_valid(nodes(ns), nodes(ns), roots, S.ids(nodes(ns)), List.length(&2, S.Node, nodes(ns)))
          == S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, _)
            && S.nodes_valid(ns, ns, roots, S.ids(ns), List.length(&2, S.Node, ns)) : Bool}
      %ids(ns) :
        {S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, S.ids(nodes(ns)))
            && S.nodes_valid(nodes(ns), nodes(ns), roots, S.ids(nodes(ns)), List.length(&2, S.Node, nodes(ns)))
          == S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, S.ids(nodes(ns)))
            && S.nodes_valid(ns, ns, roots, _, List.length(&2, S.Node, ns)) : Bool}
      %length(ns) :
        {S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, S.ids(nodes(ns)))
            && S.nodes_valid(nodes(ns), nodes(ns), roots, S.ids(nodes(ns)), List.length(&2, S.Node, nodes(ns)))
          == S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, S.ids(nodes(ns)))
            && S.nodes_valid(ns, ns, roots, S.ids(nodes(ns)), _) : Bool}
      %nodes_valid(ns, ns, roots, S.ids(nodes(ns)), List.length(&2, S.Node, nodes(ns))) :
        {S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, S.ids(nodes(ns)))
            && S.nodes_valid(nodes(ns), nodes(ns), roots, S.ids(nodes(ns)), List.length(&2, S.Node, nodes(ns)))
          == S.unique(S.ids(nodes(ns))) && S.unique(roots) && S.members(roots, S.ids(nodes(ns))) && _ : Bool}
      {==}
