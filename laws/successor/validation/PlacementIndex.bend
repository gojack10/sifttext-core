import Base
import ../../Spec.bend as S
import ./Delta.bend as D
import ./ParentClosure.bend as H
import ./Preservation.bend as V
import ./Rooted.bend as R
import ./Equivalence.bend as E

type Placement is Data:
  Placement{count: Nat, parent: Nat}

type Binding is Data:
  Binding{key: Nat, value: Placement}

type Index is Data:
  Unallocated{}
  Page{entries: List<&2, Binding>, low: Index, high: Index}

# Routing may collide. The full identity in each bucket owns key equality.
def route(depth: Nat, +key: U32) -> List<&2, Bool>:
  match depth:
    case 0n:
      []
    case 1n+rest:
      U32.is_ne(U32.and(key, 1), 0) <> route(rest, U32.shr(key))

def pick(found: Bool, value: Placement, tail: Unit -> Placement) -> Placement:
  match found:
    case False{}:
      tail(Unit{})
    case True{}:
      value

def entry(rows: List<&2, Binding>, +key: Nat) -> Placement:
  match rows:
    case Nil{}:
      Placement{0n, 0n}
    case Binding{id, value} <> tail:
      pick(Nat.is_eq(id, key), value, unused => entry(tail, key))

def get(index: Index, bits: List<&2, Bool>, +key: Nat) -> Placement:
  match index bits:
    case Unallocated{} _:
      Placement{0n, 0n}
    case Page{entries, low, high} Nil{}:
      entry(entries, key)
    case Page{entries, low, high} False{} <> rest:
      get(low, rest, key)
    case Page{entries, low, high} True{} <> rest:
      get(high, rest, key)

def put(bits: List<&2, Bool>, index: Index, key: Nat, value: Placement) -> Index:
  match bits index:
    case Nil{} Unallocated{}:
      Page{[Binding{key, value}], Unallocated{}, Unallocated{}}
    case Nil{} Page{entries, low, high}:
      Page{Binding{key, value} <> entries, low, high}
    case False{} <> rest Unallocated{}:
      Page{[], put(rest, Unallocated{}, key, value), Unallocated{}}
    case False{} <> rest Page{entries, low, high}:
      Page{entries, put(rest, low, key, value), high}
    case True{} <> rest Unallocated{}:
      Page{[], Unallocated{}, put(rest, Unallocated{}, key, value)}
    case True{} <> rest Page{entries, low, high}:
      Page{entries, low, put(rest, high, key, value)}

def entry_same(+rows: List<&2, Binding>, +key: Nat, +value: Placement) ->
  {entry(Binding{key, value} <> rows, key) == value : Placement}:
  %H.nat_equal_refl(key) :
    {entry(Binding{key, value} <> rows, key) == pick(_, value, unused => entry(rows, key)) : Placement}
  {==}

def entry_other(+rows: List<&2, Binding>, +key: Nat, +query: Nat, +value: Placement,
  different: {Nat.is_eq(key, query) == False{} : Bool}) ->
  {entry(Binding{key, value} <> rows, query) == entry(rows, query) : Placement}:
  %different : {entry(Binding{key, value} <> rows, query) == pick(_, value, unused => entry(rows, query)) : Placement}
  {==}

def get_put_same(+bits: List<&2, Bool>, +index: Index, +key: Nat, +value: Placement) ->
  {get(put(bits, index, key, value), bits, key) == value : Placement}:
  match bits index:
    case Nil{} Unallocated{}:
      entry_same([], key, value)
    case Nil{} Page{+entries, low, high}:
      entry_same(entries, key, value)
    case False{} <> +rest Unallocated{}:
      get_put_same(rest, Unallocated{}, key, value)
    case False{} <> +rest Page{entries, +low, high}:
      get_put_same(rest, low, key, value)
    case True{} <> +rest Unallocated{}:
      get_put_same(rest, Unallocated{}, key, value)
    case True{} <> +rest Page{entries, low, +high}:
      get_put_same(rest, high, key, value)

def get_put_other(+bits: List<&2, Bool>, +query_bits: List<&2, Bool>, +index: Index,
  +key: Nat, +query: Nat, +value: Placement, different: {Nat.is_eq(key, query) == False{} : Bool}) ->
  {get(put(bits, index, key, value), query_bits, query) == get(index, query_bits, query) : Placement}:
  match bits query_bits index:
    case Nil{} Nil{} Unallocated{}:
      entry_other([], key, query, value, different)
    case Nil{} Nil{} Page{+entries, low, high}:
      entry_other(entries, key, query, value, different)
    case Nil{} False{} <> rest Unallocated{}:
      {==}
    case Nil{} False{} <> rest Page{entries, low, high}:
      {==}
    case Nil{} True{} <> rest Unallocated{}:
      {==}
    case Nil{} True{} <> rest Page{entries, low, high}:
      {==}
    case False{} <> rest Nil{} Unallocated{}:
      {==}
    case False{} <> rest Nil{} Page{entries, low, high}:
      {==}
    case True{} <> rest Nil{} Unallocated{}:
      {==}
    case True{} <> rest Nil{} Page{entries, low, high}:
      {==}
    case False{} <> +rest False{} <> +suffix Unallocated{}:
      get_put_other(rest, suffix, Unallocated{}, key, query, value, different)
    case False{} <> +rest False{} <> +suffix Page{entries, +low, high}:
      get_put_other(rest, suffix, low, key, query, value, different)
    case False{} <> rest True{} <> suffix Unallocated{}:
      {==}
    case False{} <> rest True{} <> suffix Page{entries, low, high}:
      {==}
    case True{} <> rest False{} <> suffix Unallocated{}:
      {==}
    case True{} <> rest False{} <> suffix Page{entries, low, high}:
      {==}
    case True{} <> +rest True{} <> +suffix Unallocated{}:
      get_put_other(rest, suffix, Unallocated{}, key, query, value, different)
    case True{} <> +rest True{} <> +suffix Page{entries, low, +high}:
      get_put_other(rest, suffix, high, key, query, value, different)

def lookup(index: Index, depth: Nat, +key: Nat) -> Placement:
  get(index, route(depth, U32.from_nat(key)), key)

def increment(row: Placement, parent: Nat) -> Placement:
  match row:
    case Placement{count, previous}:
      Placement{1n+count, parent}

def bump(+index: Index, depth: Nat, +key: Nat, parent: Nat) -> Index:
  +bits = route(depth, U32.from_nat(key))
  put(bits, index, key, increment(get(index, bits, key), parent))

def bump_same(+index: Index, +depth: Nat, +key: Nat, +parent: Nat) ->
  {lookup(bump(index, depth, key, parent), depth, key) == increment(lookup(index, depth, key), parent) : Placement}:
  get_put_same(route(depth, U32.from_nat(key)), index, key, increment(lookup(index, depth, key), parent))

def bump_other(+index: Index, +depth: Nat, +key: Nat, +query: Nat, +parent: Nat,
  different: {Nat.is_eq(key, query) == False{} : Bool}) ->
  {lookup(bump(index, depth, key, parent), depth, query) == lookup(index, depth, query) : Placement}:
  get_put_other(route(depth, U32.from_nat(key)), route(depth, U32.from_nat(query)), index,
    key, query, increment(lookup(index, depth, key), parent), different)

def bump_equal(+index: Index, +depth: Nat, +key: Nat, +query: Nat, +parent: Nat, equal: {key == query : Nat}) ->
  {lookup(bump(index, depth, key, parent), depth, query) == increment(lookup(index, depth, query), parent) : Placement}:
  %Equal.sym(Nat, key, query, equal) :
    {lookup(bump(index, depth, _, parent), depth, query) == increment(lookup(index, depth, query), parent) : Placement}
  bump_same(index, depth, query, parent)

def update(found: Bool, parent: Nat, row: Placement) -> Placement:
  match found:
    case False{}:
      row
    case True{}:
      increment(row, parent)

def bump_lookup(+index: Index, +depth: Nat, +key: Nat, +query: Nat, +parent: Nat) ->
  {lookup(bump(index, depth, key, parent), depth, query)
    == update(Nat.is_eq(key, query), parent, lookup(index, depth, query)) : Placement}:
  +same = Nat.is_eq(key, query)
  +old = lookup(index, depth, query)
  +observed = lookup(bump(index, depth, key, parent), depth, query)
  +expected = update(same, parent, old)
  D.eq_cases(same, Placement, observed, expected,
    different => D.duplicate_for(Bool, same, False{}, different, {observed == expected : Placement},
      key_differs => branch =>
        Equal.trans(Placement, observed, old, expected,
          bump_other(index, depth, key, query, parent, key_differs),
          Equal.sym(Placement, expected, old,
            Equal.cong(Bool, Placement, b => update(b, parent, old), same, False{}, branch)))),
    equal => D.duplicate_for(Bool, same, True{}, equal, {observed == expected : Placement},
      key_equals => branch =>
        Equal.trans(Placement, observed, increment(old, parent), expected,
          bump_equal(index, depth, key, query, parent, D.nat_equal(key, query, key_equals)),
          Equal.sym(Placement, expected, increment(old, parent),
            Equal.cong(Bool, Placement, b => update(b, parent, old), same, True{}, branch)))))

def children(cs: List<&2, Nat>, +parent: Nat, +depth: Nat, index: Index) -> Index:
  match cs:
    case Nil{}:
      index
    case key <> rest:
      bump(children(rest, parent, depth, index), depth, key, parent)

def combine(count: Nat, found: Bool, parent: Nat, row: Placement) -> Placement:
  match row:
    case Placement{old, previous}:
      Placement{Nat.add(count, old), S.pick_parent(parent, previous, found)}

def combine_empty(+parent: Nat, +row: Placement) ->
  {row == combine(0n, False{}, parent, row) : Placement}:
  match row:
    case Placement{count, previous}:
      {==}

def combine_step(+found: Bool, +count: Nat, +tail_found: Bool, +parent: Nat, +row: Placement) ->
  {update(found, parent, combine(count, tail_found, parent, row)) ==
    combine(Nat.add(U32.to_nat(Bool.to_u32(found)), count), found || tail_found, parent, row) : Placement}:
  match found row:
    case False{} Placement{old, previous}:
      {==}
    case True{} Placement{old, previous}:
      {==}

def children_lookup(+cs: List<&2, Nat>, +parent: Nat, +depth: Nat, +index: Index, +query: Nat) ->
  {lookup(children(cs, parent, depth, index), depth, query) ==
    combine(S.count(cs, query), S.member(cs, query), parent, lookup(index, depth, query)) : Placement}:
  match cs:
    case Nil{}:
      combine_empty(parent, lookup(index, depth, query))
    case +key <> +rest:
      +matches = Nat.is_eq(key, query)
      +previous = children(rest, parent, depth, index)
      +observed = lookup(children(key <> rest, parent, depth, index), depth, query)
      %combine_step(matches, S.count(rest, query), S.member(rest, query), parent, lookup(index, depth, query)) :
        {observed == _ : Placement}
      %children_lookup(rest, parent, depth, index, query) :
        {observed == update(matches, parent, _) : Placement}
      bump_lookup(previous, depth, key, query, parent)

def build(ns: List<&2, S.Node>, +depth: Nat) -> Index:
  match ns:
    case Nil{}:
      Unallocated{}
    case S.Node{id, payload, cs, refs} <> rest:
      children(cs, id, depth, build(rest, depth))

def spec_row(+ns: List<&2, S.Node>, +key: Nat) -> Placement:
  Placement{S.parents(ns, key), S.parent(ns, key)}

law build_equivalent:
  for +ns: List<&2, S.Node>
  for +depth: Nat
  for +key: Nat
  {lookup(build(ns, depth), depth, key) == spec_row(ns, key) : Placement}

def build_equivalent(ns, depth, key):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+id, payload, +cs, refs} <> +rest:
      %build_equivalent(rest, depth, key) :
        {lookup(children(cs, id, depth, build(rest, depth)), depth, key) ==
          combine(S.count(cs, key), S.member(cs, key), id, _) : Placement}
      children_lookup(cs, id, depth, build(rest, depth), key)

def count(row: Placement) -> Nat:
  match row:
    case Placement{n, parent}:
      n

def parent(row: Placement) -> Nat:
  match row:
    case Placement{n, id}:
      id

def count_equivalent(+ns: List<&2, S.Node>, +depth: Nat, +key: Nat) ->
  {count(lookup(build(ns, depth), depth, key)) == S.parents(ns, key) : Nat}:
  Equal.cong(Placement, Nat, count, lookup(build(ns, depth), depth, key), spec_row(ns, key),
    build_equivalent(ns, depth, key))

def parent_equivalent(+ns: List<&2, S.Node>, +depth: Nat, +key: Nat) ->
  {parent(lookup(build(ns, depth), depth, key)) == S.parent(ns, key) : Nat}:
  Equal.cong(Placement, Nat, parent, lookup(build(ns, depth), depth, key), spec_row(ns, key),
    build_equivalent(ns, depth, key))

# A root occurrence adds one placement and records a nonzero root marker.
def roots(rs: List<&2, Nat>, depth: Nat) -> Index:
  children(rs, 1n, depth, Unallocated{})

def root_flag(row: Placement) -> Bool:
  Nat.is_ne(parent(row), 0n)

def root_count_equivalent(+rs: List<&2, Nat>, +depth: Nat, +key: Nat) ->
  {count(lookup(roots(rs, depth), depth, key)) == S.count(rs, key) : Nat}:
  +observed = lookup(roots(rs, depth), depth, key)
  +expected = combine(S.count(rs, key), S.member(rs, key), 1n, Placement{0n, 0n})
  Equal.trans(Nat, count(observed), Nat.add(S.count(rs, key), 0n), S.count(rs, key),
    Equal.cong(Placement, Nat, count, observed, expected,
      children_lookup(rs, 1n, depth, Unallocated{}, key)),
    V.add_zero(S.count(rs, key)))

def root_marker(+found: Bool) -> {Nat.is_ne(S.pick_parent(1n, 0n, found), 0n) == found : Bool}:
  match found:
    case False{}:
      {==}
    case True{}:
      {==}

def root_membership_equivalent(+rs: List<&2, Nat>, +depth: Nat, +key: Nat) ->
  {root_flag(lookup(roots(rs, depth), depth, key)) == S.member(rs, key) : Bool}:
  +observed = lookup(roots(rs, depth), depth, key)
  +expected = combine(S.count(rs, key), S.member(rs, key), 1n, Placement{0n, 0n})
  Equal.trans(Bool, root_flag(observed), root_flag(expected), S.member(rs, key),
    Equal.cong(Placement, Bool, root_flag, observed, expected,
      children_lookup(rs, 1n, depth, Unallocated{}, key)),
    root_marker(S.member(rs, key)))

def rooted(fuel: Nat, +key: Nat, +depth: Nat, +edges: Index, +declared: Index) -> Bool:
  match fuel:
    case 0n:
      root_flag(lookup(declared, depth, key))
    case 1n+rest:
      R.or_delayed(root_flag(lookup(declared, depth, key)),
        unused => rooted(rest, parent(lookup(edges, depth, key)), depth, edges, declared))

law rooted_equivalent:
  for +fuel: Nat
  for +key: Nat
  for +depth: Nat
  for +ns: List<&2, S.Node>
  for +rs: List<&2, Nat>
  {rooted(fuel, key, depth, build(ns, depth), roots(rs, depth)) == S.rooted(fuel, key, ns, rs) : Bool}

def rooted_equivalent(fuel, key, depth, ns, rs):
  match fuel:
    case 0n:
      root_membership_equivalent(rs, depth, key)
    case 1n+rest:
      +edges = build(ns, depth)
      +declared = roots(rs, depth)
      +observed = rooted(1n+rest, key, depth, edges, declared)
      +up = S.parent(ns, key)
      +root = root_flag(lookup(declared, depth, key))
      %E.delayed_or_equivalent(S.member(rs, key), unused => S.rooted(rest, up, ns, rs)) :
        {observed == _ : Bool}
      %root_membership_equivalent(rs, depth, key) :
        {observed == R.or_delayed(_, unused => S.rooted(rest, up, ns, rs)) : Bool}
      %rooted_equivalent(rest, up, depth, ns, rs) :
        {observed == R.or_delayed(root, unused => _) : Bool}
      %parent_equivalent(ns, depth, key) :
        {observed == R.or_delayed(root, unused => rooted(rest, _, depth, edges, declared)) : Bool}
      {==}
