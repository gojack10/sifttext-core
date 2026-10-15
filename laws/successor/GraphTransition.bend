import Base
import ../Spec.bend as S
import ../Runtime.bend as R

# Conditional preservation, NOT validation: S.valid(before) is a native-held
# premise established by the unchanged full-law executable for this exact base.
# Payload is absent from every predicate in S.valid; all other graph data agrees.
def same_ids(a: List<&2, Nat>, b: List<&2, Nat>) -> Bool:
  match a b:
    case Nil{} Nil{}:
      True{}
    case x <> xs y <> ys:
      Nat.is_eq(x, y) && same_ids(xs, ys)
    case x y:
      False{}

def same_refs(a: List<&2, S.Ref>, b: List<&2, S.Ref>) -> Bool:
  match a b:
    case Nil{} Nil{}:
      True{}
    case S.Ref{x, r} <> xs S.Ref{y, s} <> ys:
      Nat.is_eq(x, y) && Nat.is_eq(r, s) && same_refs(xs, ys)
    case x y:
      False{}

def same_nodes(a: List<&2, S.Node>, b: List<&2, S.Node>) -> Bool:
  match a b:
    case Nil{} Nil{}:
      True{}
    case S.Node{x, p, cs, rs} <> xs S.Node{y, q, ds, ss} <> ys:
      Nat.is_eq(x, y) && same_ids(cs, ds) && same_refs(rs, ss) && same_nodes(xs, ys)
    case x y:
      False{}

def preserves(a: S.Graph, b: S.Graph) -> Bool:
  match a b:
    case S.Graph{ns, roots, assets} S.Graph{ms, other, resources}:
      Nat.is_eq(assets, resources) && same_ids(roots, other) && same_nodes(ns, ms)

def verdict(ok: Bool) -> IO(Unit):
  match ok:
    case True{}:
      IO.print("STGT1 PRESERVED")
    case False{}:
      IO.print("STGT1 FALLBACK")

def answer(a: Maybe<&2, S.Graph>, b: Maybe<&2, S.Graph>) -> IO(Unit):
  match a b:
    case Some{x} Some{y}:
      verdict(preserves(x, y))
    case x y:
      IO.die(Unit, 1, "bad transition graphs")

def split_graphs(got: Maybe<&2, R.IdRead>) -> IO(Unit):
  match got:
    case Some{R.IdRead{before, after}}:
      answer(R.graph(before), R.graph(after))
    case other:
      IO.die(Unit, 1, "bad transition length")

def run_words(got: Maybe<&2, List<&2, Nat>>) -> IO(Unit):
  match got:
    case Some{1n <> length <> tail}:
      split_graphs(R.take_ids(length, tail))
    case other:
      IO.die(Unit, 1, "bad transition wire")

def read_result(read: File & Result<&1, &1, U32 & String, String>) -> IO(Unit):
  match read:
    case (handle, result):
      do IO<Unit>:
        File.close(handle)
        input : String <- IO.pass(String, result)
        run_words(R.numbers(String.split(String.trim(input), ' ')))

def main() -> IO(Unit):
  do IO<Unit>:
    path : String <- IO.try(String, IO.get_env("SIFTTEXT_WIRE"))
    file : File <- IO.try(File, File.open(path, "r"))
    read : File & Result<&1, &1, U32 & String, String> <- File.read(file, 1048576)
    read_result(read)
