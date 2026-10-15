import Base
import ../Spec.bend as S
import ../Runtime.bend as R
import ./validation/ParentTableGraph.bend as G

# Only validation changes computation; parser and mutations keep frozen meaning.
def dispatch(kind: Nat, x: Nat, y: Nat, g: S.Graph) -> IO(Unit):
  match kind:
    case 0n:
      R.validate_answer(G.valid(g))
    case other:
      R.dispatch(other, x, y, g)

def run_graph(kind: Nat, x: Nat, y: Nat, got: Maybe<&2, S.Graph>) -> IO(Unit):
  match got:
    case None{}:
      IO.die(Unit, 1, "bad graph")
    case Some{g}:
      dispatch(kind, x, y, g)

def run_words(got: Maybe<&2, List<&2, Nat>>) -> IO(Unit):
  match got:
    case Some{1n <> kind <> x <> y <> t}:
      run_graph(kind, x, y, R.graph(t))
    case other:
      IO.die(Unit, 1, "bad wire")

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
