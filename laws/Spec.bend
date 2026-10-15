import Base

# Meaning-bearing executable specification. Frozen before the runtime wrapper.
# UUID/payload/reason tokens are injective native encodings, not product IDs.
type Ref is Data:
  Ref{target: Nat, reason: Nat}

type Node is Data:
  Node{id: Nat, payload: Nat, children: List<&2, Nat>, refs: List<&2, Ref>}

type Graph is Data:
  Graph{nodes: List<&2, Node>, roots: List<&2, Nat>, assets: Nat}

type Request is Data:
  OpMove{node: Nat, parent: Nat}
  Reorder{node: Nat, after: Nat}
  Rename{node: Nat, payload: Nat}

type Outcome is Data:
  Reject{}
  Accept{graph: Graph}

def member(xs: List<&2, Nat>, +x: Nat) -> Bool:
  match xs:
    case Nil{}:
      False{}
    case h <> t:
      Nat.is_eq(h, x) || member(t, x)

def unique(xs: List<&2, Nat>) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case +h <> +t:
      Bool.not(member(t, h)) && unique(t)

def ids(ns: List<&2, Node>) -> List<&2, Nat>:
  match ns:
    case Nil{}:
      Nil{}
    case Node{i, p, c, r} <> t:
      i <> ids(t)

def count(xs: List<&2, Nat>, +x: Nat) -> Nat:
  match xs:
    case Nil{}:
      0n
    case h <> t:
      Nat.add(U32.to_nat(Bool.to_u32(Nat.is_eq(h, x))), count(t, x))

def parents(ns: List<&2, Node>, +x: Nat) -> Nat:
  match ns:
    case Nil{}:
      0n
    case Node{i, p, c, r} <> t:
      Nat.add(count(c, x), parents(t, x))

def pick_parent(i: Nat, fallback: Nat, yes: Bool) -> Nat:
  match yes:
    case False{}:
      fallback
    case True{}:
      i

def parent(ns: List<&2, Node>, +x: Nat) -> Nat:
  match ns:
    case Nil{}:
      0n
    case Node{i, p, c, r} <> t:
      pick_parent(i, parent(t, x), member(c, x))

def rooted(fuel: Nat, +x: Nat, +ns: List<&2, Node>, +roots: List<&2, Nat>) -> Bool:
  match fuel:
    case 0n:
      member(roots, x)
    case 1n+f:
      member(roots, x) || rooted(f, parent(ns, x), ns, roots)

def ancestor(fuel: Nat, +x: Nat, +a: Nat, +ns: List<&2, Node>) -> Bool:
  match fuel:
    case 0n:
      Nat.is_eq(x, a)
    case 1n+f:
      Nat.is_eq(x, a) || ancestor(f, parent(ns, x), a, ns)

def members(xs: List<&2, Nat>, +keys: List<&2, Nat>) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case h <> t:
      member(keys, h) && members(t, keys)

def refs_valid(rs: List<&2, Ref>, +keys: List<&2, Nat>) -> Bool:
  match rs:
    case Nil{}:
      True{}
    case Ref{target, reason} <> t:
      member(keys, target) && Nat.is_ne(reason, 0n) && refs_valid(t, keys)

def nodes_valid(xs: List<&2, Node>, +ns: List<&2, Node>, +roots: List<&2, Nat>, +keys: List<&2, Nat>, +fuel: Nat) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case Node{+i, p, +c, r} <> t:
      Nat.is_ne(i, 0n) && unique(c) && members(c, keys) && refs_valid(r, keys)
        && Nat.is_eq(Nat.add(count(roots, i), parents(ns, i)), 1n)
        && rooted(fuel, i, ns, roots) && nodes_valid(t, ns, roots, keys, fuel)

def valid(g: Graph) -> Bool:
  match g:
    case Graph{+ns, +roots, assets}:
      +keys = ids(ns)
      unique(keys) && unique(roots) && members(roots, keys)
        && nodes_valid(ns, ns, roots, keys, List.length(&2, Node, ns))

def remove(xs: List<&2, Nat>, +x: Nat) -> List<&2, Nat>:
  match xs:
    case Nil{}:
      Nil{}
    case +h <> +t:
      Bool.pick(List<&2, Nat>, Nat.is_eq(h, x), remove(t, x), h <> remove(t, x))

def insert(xs: List<&2, Nat>, +x: Nat, +after: Nat) -> List<&2, Nat>:
  match xs:
    case Nil{}:
      Nil{}
    case +h <> +t:
      Bool.pick(List<&2, Nat>, Nat.is_eq(h, after), h <> x <> t, h <> insert(t, x, after))

def moved(ns: List<&2, Node>, +x: Nat, +dst: Nat) -> List<&2, Node>:
  match ns:
    case Nil{}:
      Nil{}
    case Node{+i, p, c, r} <> t:
      +rest = remove(c, x)
      next = Bool.pick(List<&2, Nat>, Nat.is_eq(i, dst), List.append(&2, Nat, rest, [x]), rest)
      Node{i, p, next, r} <> moved(t, x, dst)

def reordered(ns: List<&2, Node>, +x: Nat, +after: Nat, +par: Nat) -> List<&2, Node>:
  match ns:
    case Nil{}:
      Nil{}
    case Node{+i, p, +c, r} <> t:
      next = Bool.pick(List<&2, Nat>, Nat.is_eq(i, par), insert(remove(c, x), x, after), c)
      Node{i, p, next, r} <> reordered(t, x, after, par)

def renamed(ns: List<&2, Node>, +x: Nat, +payload: Nat) -> List<&2, Node>:
  match ns:
    case Nil{}:
      Nil{}
    case Node{+i, p, c, r} <> t:
      Node{i, Bool.pick(Nat, Nat.is_eq(i, x), payload, p), c, r} <> renamed(t, x, payload)

def gate(ok: Bool, g: Graph) -> Outcome:
  match ok:
    case False{}:
      Reject{}
    case True{}:
      Accept{g}

def checked(+g: Graph, ok: Bool) -> Outcome:
  gate(ok && valid(g), g)

def transition(+g: Graph, req: Request) -> Outcome:
  match g req:
    case Graph{+ns, +roots, assets} OpMove{+x, +dst}:
      +keys = ids(ns)
      ok = valid(Graph{ns, roots, assets}) && member(keys, x) && member(keys, dst) && Bool.not(member(roots, x))
        && Bool.not(ancestor(List.length(&2, Node, ns), dst, x, ns))
      checked(Graph{moved(ns, x, dst), roots, assets}, ok)
    case Graph{+ns, roots, assets} Reorder{+x, +after}:
      +par = parent(ns, x)
      ok = valid(Graph{ns, roots, assets}) && Nat.is_ne(par, 0n) && Nat.is_ne(x, after)
        && Nat.is_eq(par, parent(ns, after))
      checked(Graph{reordered(ns, x, after, par), roots, assets}, ok)
    case Graph{+ns, roots, assets} Rename{+x, +payload}:
      ok = valid(Graph{ns, roots, assets}) && member(ids(ns), x) && Nat.is_ne(payload, 0n)
      checked(Graph{renamed(ns, x, payload), roots, assets}, ok)

def before() -> Graph:
  Graph{[Node{1n, 11n, [2n, 3n, 4n], []}, Node{2n, 12n, [], [Ref{5n, 1n}]},
    Node{3n, 13n, [5n], []}, Node{4n, 14n, [], []}, Node{5n, 15n, [], []},
    Node{6n, 16n, [7n], []}, Node{7n, 17n, [], [Ref{3n, 2n}]}], [1n, 6n], 99n}

def after_move() -> Graph:
  Graph{[Node{1n, 11n, [2n, 4n], []}, Node{2n, 12n, [3n], [Ref{5n, 1n}]},
    Node{3n, 13n, [5n], []}, Node{4n, 14n, [], []}, Node{5n, 15n, [], []},
    Node{6n, 16n, [7n], []}, Node{7n, 17n, [], [Ref{3n, 2n}]}], [1n, 6n], 99n}

def after_reorder() -> Graph:
  Graph{[Node{1n, 11n, [2n, 4n, 3n], []}, Node{2n, 12n, [], [Ref{5n, 1n}]},
    Node{3n, 13n, [5n], []}, Node{4n, 14n, [], []}, Node{5n, 15n, [], []},
    Node{6n, 16n, [7n], []}, Node{7n, 17n, [], [Ref{3n, 2n}]}], [1n, 6n], 99n}

def after_rename() -> Graph:
  Graph{[Node{1n, 11n, [2n, 3n, 4n], []}, Node{2n, 12n, [], [Ref{5n, 1n}]},
    Node{3n, 18n, [5n], []}, Node{4n, 14n, [], []}, Node{5n, 15n, [], []},
    Node{6n, 16n, [7n], []}, Node{7n, 17n, [], [Ref{3n, 2n}]}], [1n, 6n], 99n}
