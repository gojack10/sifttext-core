import Base
import ../../Spec.bend as S
import ./ParentTable.bend as P

# Conditional graph-preservation computation. Native code supplies an exact
# same-law valid base and the complete semantic placement frontier; this wire
# mode is not an unconditional graph-validity verdict for arbitrary inputs.
def references(ns: List<&2, S.Node>, +keys: List<&2, Nat>) -> Bool:
  match ns:
    case Nil{}:
      True{}
    case S.Node{i, p, c, refs} <> t:
      S.refs_valid(refs, keys) && references(t, keys)

def placements(frontier: List<&2, Nat>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>,
  +keys: List<&2, Nat>, +fuel: Nat, +table: List<&2, Nat>) -> Bool:
  match frontier:
    case Nil{}:
      True{}
    case +i <> t:
      S.member(keys, i) && Nat.is_eq(Nat.add(S.count(roots, i), S.parents(ns, i)), 1n)
        && P.rooted(fuel, i, table, roots) && placements(t, ns, roots, keys, fuel, table)

def preserved(g: S.Graph, frontier: List<&2, Nat>) -> Bool:
  match g:
    case S.Graph{+ns, +roots, assets}:
      +keys = S.ids(ns)
      references(ns, keys) && placements(frontier, ns, roots, keys, List.length(&2, S.Node, ns), P.build(ns))

def verdict(ok: Bool) -> IO(Unit):
  match ok:
    case True{}:
      IO.print("STGD1 PRESERVED")
    case False{}:
      IO.print("STGD1 REJECT")
