import Base
import ../../Spec.bend as S
import ./Delta.bend as D
import ./DenseIdentity.bend as I
import ./PlacementIndex.bend as P
import ./ParentTableGraph.bend as L

def claims(+id: Nat, +depth: Nat, edges: P.Index, roots: P.Index) -> Nat:
  Nat.add(P.count(P.lookup(roots, depth, id)), P.count(P.lookup(edges, depth, id)))

def claims_equivalent(+id: Nat, +depth: Nat, +ns: List<&2, S.Node>, +rs: List<&2, Nat>) ->
  {claims(id, depth, P.build(ns, depth), P.roots(rs, depth)) ==
    Nat.add(S.count(rs, id), S.parents(ns, id)) : Nat}:
  +observed = claims(id, depth, P.build(ns, depth), P.roots(rs, depth))
  +root_count = P.count(P.lookup(P.roots(rs, depth), depth, id))
  %P.root_count_equivalent(rs, depth, id) : {observed == Nat.add(_, S.parents(ns, id)) : Nat}
  %P.count_equivalent(ns, depth, id) : {observed == Nat.add(root_count, _) : Nat}
  {==}

def row(id: Nat, cs: List<&2, Nat>, children: Bool, refs: Bool, placement: Nat,
  rooted: Bool, tail: Bool) -> Bool:
  Nat.is_ne(id, 0n) && S.unique(cs) && children && refs
    && Nat.is_eq(placement, 1n) && rooted && tail

def nodes(rows: List<&2, S.Node>, +limit: Nat, +depth: Nat, +edges: P.Index, +roots: P.Index) -> Bool:
  match rows:
    case Nil{}:
      True{}
    case S.Node{+id, payload, +children, refs} <> rest:
      row(id, children, I.members(children, limit), I.references(refs, limit),
        claims(id, depth, edges, roots), P.rooted(limit, id, depth, edges, roots),
        nodes(rest, limit, depth, edges, roots))

def nodes_equivalent(+rows: List<&2, S.Node>, +ns: List<&2, S.Node>,
  +rs: List<&2, Nat>, +depth: Nat, proof: {I.dense(ns, 1n) == True{} : Bool}) ->
  {nodes(rows, List.length(&2, S.Node, ns), depth, P.build(ns, depth), P.roots(rs, depth)) ==
    S.nodes_valid(rows, ns, rs, S.ids(ns), List.length(&2, S.Node, ns)) : Bool}:
  match rows:
    case Nil{}:
      {==}
    case S.Node{+id, payload, +children, +refs} <> +rest:
      +limit = List.length(&2, S.Node, ns)
      +keys = S.ids(ns)
      +edges = P.build(ns, depth)
      +roots = P.roots(rs, depth)
      +observed = nodes(S.Node{id, payload, children, refs} <> rest, limit, depth, edges, roots)
      +old_child = S.members(children, keys)
      +old_refs = S.refs_valid(refs, keys)
      +old_claims = Nat.add(S.count(rs, id), S.parents(ns, id))
      +old_root = S.rooted(limit, id, ns, rs)
      +old_tail = S.nodes_valid(rest, ns, rs, keys, limit)
      +new_child = I.members(children, limit)
      +new_refs = I.references(refs, limit)
      +new_claims = claims(id, depth, edges, roots)
      +new_root = P.rooted(limit, id, depth, edges, roots)
      +shape = I.dense(ns, 1n)
      +expected = row(id, children, old_child, old_refs, old_claims, old_root, old_tail)
      D.duplicate_for(Bool, shape, True{}, proof, {observed == expected : Bool},
        local => tail =>
          D.duplicate_for(Bool, shape, True{}, local, {observed == expected : Bool},
            membership => references =>
              (%I.members_equivalent(children, ns, membership) :
                 {observed == row(id, children, _, old_refs, old_claims, old_root, old_tail) : Bool}
               %I.references_equivalent(refs, ns, references) :
                 {observed == row(id, children, new_child, _, old_claims, old_root, old_tail) : Bool}
               %claims_equivalent(id, depth, ns, rs) :
                 {observed == row(id, children, new_child, new_refs, _, old_root, old_tail) : Bool}
               %P.rooted_equivalent(limit, id, depth, ns, rs) :
                 {observed == row(id, children, new_child, new_refs, new_claims, _, old_tail) : Bool}
               %nodes_equivalent(rest, ns, rs, depth, tail) :
                 {observed == row(id, children, new_child, new_refs, new_claims, new_root, _) : Bool}
               {==})))

def index_depth(limit: Nat) -> Nat:
  1n+U32.log2(U32.from_nat(limit))

def indexed(+ns: List<&2, S.Node>, +rs: List<&2, Nat>) -> Bool:
  +limit = List.length(&2, S.Node, ns)
  +depth = index_depth(limit)
  S.unique(rs) && I.members(rs, limit)
    && nodes(ns, limit, depth, P.build(ns, depth), P.roots(rs, depth))

def indexed_equivalent(+ns: List<&2, S.Node>, +rs: List<&2, Nat>, +assets: Nat,
  proof: {I.dense(ns, 1n) == True{} : Bool}) ->
  {indexed(ns, rs) == S.valid(S.Graph{ns, rs, assets}) : Bool}:
  +limit = List.length(&2, S.Node, ns)
  +depth = index_depth(limit)
  +keys = S.ids(ns)
  +shape = I.dense(ns, 1n)
  +observed = indexed(ns, rs)
  +expected = S.valid(S.Graph{ns, rs, assets})
  D.duplicate_for(Bool, shape, True{}, proof, {observed == expected : Bool},
    unique => rest =>
      D.duplicate_for(Bool, shape, True{}, rest, {observed == expected : Bool},
        roots => rows =>
          (%Equal.sym(Bool, S.unique(keys), True{}, I.dense_unique(ns, 1n, unique)) :
             {observed == _ && S.unique(rs) && S.members(rs, keys) && S.nodes_valid(ns, ns, rs, keys, limit) : Bool}
           %I.members_equivalent(rs, ns, roots) :
             {observed == S.unique(rs) && _ && S.nodes_valid(ns, ns, rs, keys, limit) : Bool}
           %nodes_equivalent(ns, ns, rs, depth, rows) :
             {observed == S.unique(rs) && I.members(rs, limit) && _ : Bool}
           {==})))

def choose(dense: Bool, g: S.Graph) -> Bool:
  match dense g:
    case False{} sparse:
      L.valid(sparse)
    case True{} S.Graph{ns, rs, assets}:
      indexed(ns, rs)

def valid(g: S.Graph) -> Bool:
  match g:
    case S.Graph{+ns, rs, assets}:
      choose(I.dense(ns, 1n), S.Graph{ns, rs, assets})

law valid_equivalent:
  for +g: S.Graph
  {valid(g) == S.valid(g) : Bool}

def valid_equivalent(g):
  match g:
    case S.Graph{+ns, +rs, +assets}:
      +input = {S.Graph{ns, rs, assets} : S.Graph}
      +shape = I.dense(ns, 1n)
      +observed = valid(input)
      +expected = S.valid(input)
      D.eq_cases(shape, Bool, observed, expected,
        sparse =>
          (%Equal.sym(Bool, shape, False{}, sparse) : {choose(_, input) == expected : Bool}
           L.valid_equivalent(input)),
        dense =>
          D.duplicate_for(Bool, shape, True{}, dense, {observed == expected : Bool},
            branch => proof =>
              (%Equal.sym(Bool, shape, True{}, branch) : {choose(_, input) == expected : Bool}
               indexed_equivalent(ns, rs, assets, proof))))
