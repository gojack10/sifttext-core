import Base
import ../Runtime.bend as R

# Migration relation only. Native code binds UUIDs, bytes, original Git custody
# and complete batches. This predicate is not a theorem about filesystem writes.
# Each logical scalar uses four range-checked limbs; never reconstruct a u64
# as a Nat or send it through the 48-bit decimal scalar reader.
type U64 is Data:
  U64{a: Nat, b: Nat, c: Nat, d: Nat}
type Read is Data:
  Read{got: List<&2, U64>, rest: List<&2, U64>}

def zero() -> U64:
  U64{0n, 0n, 0n, 0n}

def one() -> U64:
  U64{0n, 0n, 0n, 1n}

def equal(x: U64, y: U64) -> Bool:
  match x y:
    case U64{a, b, c, d} U64{e, f, g, h}:
      Nat.is_eq(a, e) && Nat.is_eq(b, f) && Nat.is_eq(c, g) && Nat.is_eq(d, h)

def less(x: U64, y: U64) -> Bool:
  match x y:
    case U64{+a, +b, +c, d} U64{+e, +f, +g, h}:
      Nat.is_lt(a, e) || (Nat.is_eq(a, e) && (Nat.is_lt(b, f)
        || (Nat.is_eq(b, f) && (Nat.is_lt(c, g) || (Nat.is_eq(c, g) && Nat.is_lt(d, h))))))

def at_most(+x: U64, +y: U64) -> Bool:
  less(x, y) || equal(x, y)

def add_digit(a: Nat, b: Nat, carry: Nat) -> Nat & Nat:
  Nat.divmod(Nat.add(Nat.add(a, b), carry), 65536n)

def add_top(hi: Nat, mid: Nat, lo: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (carry, top):
      U64{top, hi, mid, lo}

def add_b(a: Nat, e: Nat, mid: Nat, lo: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (carry, hi):
      add_top(hi, mid, lo, add_digit(a, e, carry))

def add_c(a: Nat, b: Nat, e: Nat, f: Nat, lo: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (carry, mid):
      add_b(a, e, mid, lo, add_digit(b, f, carry))

def add_d(a: Nat, b: Nat, c: Nat, e: Nat, f: Nat, g: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (carry, lo):
      add_c(a, b, e, f, lo, add_digit(c, g, carry))

def add(x: U64, y: U64) -> U64:
  match x y:
    case U64{a, +b, +c, d} U64{e, +f, +g, h}:
      add_d(a, b, c, e, f, g, add_digit(d, h, 0n))

def sub_digit_result(value: Nat & Nat) -> Nat & Nat:
  match value:
    case (carry, digit):
      (Nat.sub(1n, carry), digit)

def sub_digit(a: Nat, b: Nat, borrow: Nat) -> Nat & Nat:
  sub_digit_result(Nat.divmod(Nat.sub(Nat.add(a, 65536n), Nat.add(b, borrow)), 65536n))

def sub_top(hi: Nat, mid: Nat, lo: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (borrow, top):
      U64{top, hi, mid, lo}

def sub_b(a: Nat, e: Nat, mid: Nat, lo: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (borrow, hi):
      sub_top(hi, mid, lo, sub_digit(a, e, borrow))

def sub_c(a: Nat, b: Nat, e: Nat, f: Nat, lo: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (borrow, mid):
      sub_b(a, e, mid, lo, sub_digit(b, f, borrow))

def sub_d(a: Nat, b: Nat, c: Nat, e: Nat, f: Nat, g: Nat, digit: Nat & Nat) -> U64:
  match digit:
    case (borrow, lo):
      sub_c(a, b, e, f, lo, sub_digit(c, g, borrow))

# Callers establish y <= x. Rank addition is bounded by the actual count.
def subtract(x: U64, y: U64) -> U64:
  match x y:
    case U64{a, b, c, d} U64{e, f, g, h}:
      sub_d(a, b, c, e, f, g, sub_digit(d, h, 0n))

def shifted(rank: U64, actual: U64) -> Bool:
  match rank:
    case U64{+a, +b, +c, +d}:
      Nat.is_lt(a, 64n) && equal(actual, U64{
        Nat.add(Nat.mul(a, 1024n), Nat.div(b, 64n)),
        Nat.add(Nat.mul(Nat.mod(b, 64n), 1024n), Nat.div(c, 64n)),
        Nat.add(Nat.mul(Nat.mod(c, 64n), 1024n), Nat.div(d, 64n)),
        Nat.mul(Nat.mod(d, 64n), 1024n)})

def has(xs: List<&2, U64>, +x: U64) -> Bool:
  match xs:
    case Nil{}:
      False{}
    case h <> t:
      equal(h, x) || has(t, x)

def unique_next(+h: U64, seen: List<&2, U64>, found: Bool, next: List<&2, U64> -> List<&2, U64>) -> List<&2, U64>:
  match found:
    case True{}:
      next(seen)
    case False{}:
      h <> next(h <> seen)

def unique(xs: List<&2, U64>, +seen: List<&2, U64>) -> List<&2, U64>:
  match xs:
    case Nil{}:
      []
    case +h <> t:
      unique_next(h, seen, has(seen, h), more => unique(t, more))

def valid_hints(xs: List<&2, U64>, +count: U64) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case +h <> t:
      less(zero(), h) && at_most(h, count) && valid_hints(t, count)

# Listed members take their first-occurrence rank. An omitted ordinal r takes
# |H| + r - |{h in H : h < r}|, appending omissions in original UUID order.
def add_below(ok: Bool, below: U64) -> U64:
  match ok:
    case True{}:
      add(below, one())
    case False{}:
      below

def rank_next(h: U64, r: U64, index: U64, below: U64, found: Bool, next: U64 -> U64) -> U64:
  match found:
    case True{}:
      index
    case False{}:
      next(add_below(less(h, r), below))

def rank(xs: List<&2, U64>, +r: U64, +index: U64, below: U64) -> U64:
  match xs:
    case Nil{}:
      add(subtract(r, below), subtract(index, one()))
    case +h <> t:
      rank_next(h, r, index, below, equal(h, r), less_count => rank(t, r, add(index, one()), less_count))

def position_result(hints: List<&2, U64>, ordinal: U64, old: U64, position: U64, mode: Bool) -> Bool:
  match mode:
    case True{}:
      shifted(rank(hints, ordinal, one(), zero()), position)
    case False{}:
      equal(old, position)

def rows(xs: List<&2, U64>, +left: U64, +slot: U64, +mode: U64, +hints: List<&2, U64>) -> Bool:
  match xs:
    case Nil{}:
      equal(left, zero())
    case +ordinal <> old <> position <> tail:
      less(zero(), left) && equal(ordinal, slot)
        && position_result(hints, ordinal, old, position, equal(mode, one()))
        && rows(tail, subtract(left, one()), add(slot, one()), mode, hints)
    case other:
      False{}

def cons_take(h: U64, got: Maybe<&2, Read>) -> Maybe<&2, Read>:
  match got:
    case Some{Read{xs, rest}}:
      Some{Read{h <> xs, rest}}
    case None{}:
      None{}

def take_empty(done: Bool) -> Maybe<&2, Read>:
  match done:
    case True{}:
      Some{Read{[], []}}
    case False{}:
      None{}

def take_next(h: U64, t: List<&2, U64>, done: Bool, rest: Maybe<&2, Read>) -> Maybe<&2, Read>:
  match done:
    case True{}:
      Some{Read{[], h <> t}}
    case False{}:
      cons_take(h, rest)

def take(xs: List<&2, U64>, +n: U64) -> Maybe<&2, Read>:
  match xs:
    case Nil{}:
      take_empty(equal(n, zero()))
    case +h <> +t:
      take_next(h, t, equal(n, zero()), take(t, subtract(n, one())))

def check_hints(got: Maybe<&2, Read>, +mode: U64, +count: U64, start: U64, length: U64) -> Bool:
  match got:
    case Some{Read{+hints, tail}}:
      valid_hints(hints, count) && rows(tail, length, add(start, one()), mode, unique(hints, []))
    case None{}:
      False{}

def check(xs: List<&2, U64>) -> Bool:
  match xs:
    case version <> +mode <> +before <> +after <> +start <> +length <> +q <> tail:
      equal(version, one()) && (equal(mode, zero()) || equal(mode, one()))
        && equal(before, after) && at_most(start, after) && at_most(length, subtract(after, start))
        && at_most(length, U64{0n, 0n, 0n, 4096n}) && at_most(q, U64{0n, 0n, 0n, 28339n})
        && (equal(mode, one()) || equal(q, zero()))
        && check_hints(take(tail, q), mode, before, start, length)
    case other:
      False{}

def cons_word(h: U64, tail: Maybe<&2, List<&2, U64>>) -> Maybe<&2, List<&2, U64>>:
  match tail:
    case Some{xs}:
      Some{h <> xs}
    case None{}:
      None{}

def word_checked(h: U64, tail: Maybe<&2, List<&2, U64>>, valid: Bool) -> Maybe<&2, List<&2, U64>>:
  match valid:
    case True{}:
      cons_word(h, tail)
    case False{}:
      None{}

def words(xs: List<&2, Nat>) -> Maybe<&2, List<&2, U64>>:
  match xs:
    case Nil{}:
      Some{[]}
    case +a <> +b <> +c <> +d <> tail:
      word_checked(U64{a, b, c, d}, words(tail),
        Nat.is_lt(a, 65536n) && Nat.is_lt(b, 65536n) && Nat.is_lt(c, 65536n) && Nat.is_lt(d, 65536n))
    case other:
      None{}

def verdict(ok: Bool) -> IO(Unit):
  match ok:
    case True{}:
      IO.print("STOM1 VALID")
    case False{}:
      IO.print("STOM1 REJECT")

def run_values(got: Maybe<&2, List<&2, U64>>) -> IO(Unit):
  match got:
    case Some{values}:
      verdict(check(values))
    case None{}:
      verdict(False{})

def run(got: Maybe<&2, List<&2, Nat>>) -> IO(Unit):
  match got:
    case Some{xs}:
      run_values(words(xs))
    case None{}:
      verdict(False{})

def read_result(read: File & Result<&1, &1, U32 & String, String>) -> IO(Unit):
  match read:
    case (handle, result):
      do IO<Unit>:
        File.close(handle)
        input : String <- IO.pass(String, result)
        run(R.numbers(String.split(String.trim(input), ' ')))

def main() -> IO(Unit):
  do IO<Unit>:
    path : String <- IO.try(String, IO.get_env("SIFTTEXT_WIRE"))
    file : File <- IO.try(File, File.open(path, "r"))
    read : File & Result<&1, &1, U32 & String, String> <- File.read(file, 1048576)
    read_result(read)
