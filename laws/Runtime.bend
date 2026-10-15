import Base
import ./Spec.bend as S
import ./Core.bend as C

# Provisional transport; this parser/IO/compiler is outside the graph theorem.
type IdRead is Data:
  IdRead{got: List<&2, Nat>, rest: List<&2, Nat>}
type RefRead is Data:
  RefRead{got: List<&2, S.Ref>, rest: List<&2, Nat>}
type NodeRead is Data:
  NodeRead{got: S.Node, rest: List<&2, Nat>}
type NodesRead is Data:
  NodesRead{got: List<&2, S.Node>, rest: List<&2, Nat>}

def cons_number(n: Maybe<&2, Nat>, tail: Maybe<&2, List<&2, Nat>>) -> Maybe<&2, List<&2, Nat>>:
  match n tail:
    case Some{x} Some{xs}:
      Some{x <> xs}
    case a b:
      None{}

def numbers(ss: List<&2, String>) -> Maybe<&2, List<&2, Nat>>:
  match ss:
    case Nil{}:
      Some{[]}
    case h <> t:
      cons_number(Nat.read(h), numbers(t))

def cons_ids(h: Nat, r: Maybe<&2, IdRead>) -> Maybe<&2, IdRead>:
  match r:
    case Some{IdRead{got, rest}}:
      Some{IdRead{h <> got, rest}}
    case None{}:
      None{}

def take_ids(n: Nat, xs: List<&2, Nat>) -> Maybe<&2, IdRead>:
  match n xs:
    case 0n rest:
      Some{IdRead{[], rest}}
    case 1n+p Nil{}:
      None{}
    case 1n+p h <> t:
      cons_ids(h, take_ids(p, t))

def cons_refs(a: Nat, b: Nat, r: Maybe<&2, RefRead>) -> Maybe<&2, RefRead>:
  match r:
    case Some{RefRead{got, rest}}:
      Some{RefRead{S.Ref{a, b} <> got, rest}}
    case None{}:
      None{}

def take_refs(n: Nat, xs: List<&2, Nat>) -> Maybe<&2, RefRead>:
  match n xs:
    case 0n rest:
      Some{RefRead{[], rest}}
    case 1n+p a <> b <> t:
      cons_refs(a, b, take_refs(p, t))
    case 1n+p rest:
      None{}

def node_refs(i: Nat, p: Nat, c: List<&2, Nat>, r: Maybe<&2, RefRead>) -> Maybe<&2, NodeRead>:
  match r:
    case Some{RefRead{refs, rest}}:
      Some{NodeRead{S.Node{i, p, c, refs}, rest}}
    case None{}:
      None{}

def node_children(i: Nat, p: Nat, r: Maybe<&2, IdRead>) -> Maybe<&2, NodeRead>:
  match r:
    case Some{IdRead{children, n <> tail}}:
      node_refs(i, p, children, take_refs(n, tail))
    case other:
      None{}

def one_node(xs: List<&2, Nat>) -> Maybe<&2, NodeRead>:
  match xs:
    case i <> p <> n <> t:
      node_children(i, p, take_ids(n, t))
    case rest:
      None{}

def cons_nodes(node: S.Node, r: Maybe<&2, NodesRead>) -> Maybe<&2, NodesRead>:
  match r:
    case Some{NodesRead{nodes, rest}}:
      Some{NodesRead{node <> nodes, rest}}
    case None{}:
      None{}

def node_next(r: Maybe<&2, NodeRead>, next: List<&2, Nat> -> Maybe<&2, NodesRead>) -> Maybe<&2, NodesRead>:
  match r:
    case Some{NodeRead{node, rest}}:
      cons_nodes(node, next(rest))
    case None{}:
      None{}

def take_nodes(n: Nat, xs: List<&2, Nat>) -> Maybe<&2, NodesRead>:
  match n:
    case 0n:
      Some{NodesRead{[], xs}}
    case 1n+p:
      node_next(one_node(xs), rest => take_nodes(p, rest))

def finish_graph(roots: List<&2, Nat>, assets: Nat, r: Maybe<&2, NodesRead>) -> Maybe<&2, S.Graph>:
  match r:
    case Some{NodesRead{ns, Nil{}}}:
      Some{S.Graph{ns, roots, assets}}
    case other:
      None{}

def graph_roots(assets: Nat, r: Maybe<&2, IdRead>) -> Maybe<&2, S.Graph>:
  match r:
    case Some{IdRead{roots, n <> tail}}:
      finish_graph(roots, assets, take_nodes(n, tail))
    case other:
      None{}

def graph(xs: List<&2, Nat>) -> Maybe<&2, S.Graph>:
  match xs:
    case assets <> nr <> t:
      graph_roots(assets, take_ids(nr, t))
    case rest:
      None{}

def show_ids(xs: List<&2, Nat>) -> String:
  match xs:
    case Nil{}:
      ""
    case h <> t:
      Nat.show(h) ++ " " ++ show_ids(t)

def show_refs(xs: List<&2, S.Ref>) -> String:
  match xs:
    case Nil{}:
      ""
    case S.Ref{target, reason} <> t:
      Nat.show(target) ++ " " ++ Nat.show(reason) ++ " " ++ show_refs(t)

def show_nodes(xs: List<&2, S.Node>) -> String:
  match xs:
    case Nil{}:
      ""
    case S.Node{i, p, +c, +r} <> t:
      Nat.show(i) ++ " " ++ Nat.show(p) ++ " " ++ Nat.show(List.length(&2, Nat, c)) ++ " " ++ show_ids(c)
        ++ Nat.show(List.length(&2, S.Ref, r)) ++ " " ++ show_refs(r) ++ show_nodes(t)

def show_graph(g: S.Graph) -> String:
  match g:
    case S.Graph{+ns, +roots, assets}:
      Nat.show(assets) ++ " " ++ Nat.show(List.length(&2, Nat, roots)) ++ " " ++ show_ids(roots)
        ++ Nat.show(List.length(&2, S.Node, ns)) ++ " " ++ show_nodes(ns)

def answer(result: S.Outcome) -> IO(Unit):
  match result:
    case S.Reject{}:
      IO.print("STG1 REJECT")
    case S.Accept{g}:
      IO.print("STG1 OK " ++ String.trim(show_graph(g)))

def validate_answer(ok: Bool) -> IO(Unit):
  match ok:
    case False{}:
      IO.print("STG1 REJECT")
    case True{}:
      IO.print("STG1 VALID")

def dispatch(kind: Nat, x: Nat, y: Nat, g: S.Graph) -> IO(Unit):
  match kind:
    case 0n:
      validate_answer(S.valid(g))
    case 1n:
      answer(C.execute(g, S.OpMove{x, y}))
    case 2n:
      answer(C.execute(g, S.Reorder{x, y}))
    case 3n:
      answer(C.execute(g, S.Rename{x, y}))
    case other:
      IO.die(Unit, 1, "bad operation")

def run_graph(kind: Nat, x: Nat, y: Nat, got: Maybe<&2, S.Graph>) -> IO(Unit):
  match got:
    case None{}:
      IO.die(Unit, 1, "bad graph")
    case Some{g}:
      dispatch(kind, x, y, g)

def run_words(got: Maybe<&2, List<&2, Nat>>) -> IO(Unit):
  match got:
    case Some{1n <> kind <> x <> y <> t}:
      run_graph(kind, x, y, graph(t))
    case other:
      IO.die(Unit, 1, "bad wire")

def read_result(read: File & Result<&1, &1, U32 & String, String>) -> IO(Unit):
  match read:
    case (handle, result):
      do IO<Unit>:
        File.close(handle)
        input : String <- IO.pass(String, result)
        run_words(numbers(String.split(String.trim(input), ' ')))

def main() -> IO(Unit):
  do IO<Unit>:
    path : String <- IO.try(String, IO.get_env("SIFTTEXT_WIRE"))
    file : File <- IO.try(File, File.open(path, "r"))
    read : File & Result<&1, &1, U32 & String, String> <- File.read(file, 1048576)
    read_result(read)
