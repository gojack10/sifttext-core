import Base
import ../../Spec.bend as S
import ./Delta.bend as D

def dense(ns: List<&2, S.Node>, +next: Nat) -> Bool:
  match ns:
    case Nil{}:
      True{}
    case S.Node{id, payload, children, refs} <> tail:
      Nat.is_eq(id, next) && dense(tail, 1n+next)

def in_range(+id: Nat, limit: Nat) -> Bool:
  Nat.is_ne(id, 0n) && Nat.is_le(id, limit)

def references(rs: List<&2, S.Ref>, +limit: Nat) -> Bool:
  match rs:
    case Nil{}:
      True{}
    case S.Ref{target, reason} <> tail:
      in_range(target, limit) && Nat.is_ne(reason, 0n) && references(tail, limit)

def interval(length: Nat, +start: Nat, +id: Nat) -> Bool:
  match length:
    case 0n:
      False{}
    case 1n+rest:
      Nat.is_eq(start, id) || interval(rest, 1n+start, id)

def interval_shift(length: Nat, +start: Nat, +id: Nat) ->
  {interval(length, 1n+start, 1n+id) == interval(length, start, id) : Bool}:
  match length:
    case 0n:
      {==}
    case 1n+rest:
      Equal.cong(Bool, Bool, part => Nat.is_eq(start, id) || part,
        interval(rest, 1n+1n+start, 1n+id), interval(rest, 1n+start, id),
        interval_shift(rest, 1n+start, id))

def interval_excludes_zero(length: Nat, +start: Nat) ->
  {interval(length, 1n+start, 0n) == False{} : Bool}:
  match length:
    case 0n:
      {==}
    case 1n+rest:
      interval_excludes_zero(rest, 1n+start)

def zero_le(length: Nat) -> {Nat.is_le(0n, length) == True{} : Bool}:
  match length:
    case 0n:
      {==}
    case 1n+rest:
      {==}

def interval_range(+length: Nat, +id: Nat) ->
  {interval(length, 1n, id) == in_range(id, length) : Bool}:
  match length id:
    case 0n 0n:
      {==}
    case 0n 1n+rest:
      {==}
    case 1n+rest 0n:
      interval_excludes_zero(rest, 1n)
    case 1n+rest 1n+0n:
      Equal.sym(Bool, Nat.is_le(0n, rest), True{}, zero_le(rest))
    case 1n+rest 1n+1n+index:
      Equal.trans(Bool, interval(1n+rest, 1n, 1n+1n+index),
        interval(rest, 1n, 1n+index), in_range(1n+1n+index, 1n+rest),
        interval_shift(rest, 1n, 1n+index), interval_range(rest, 1n+index))

def dense_interval(+ns: List<&2, S.Node>, +start: Nat, +id: Nat,
  proof: {dense(ns, start) == True{} : Bool}) ->
  {S.member(S.ids(ns), id) == interval(List.length(&2, S.Node, ns), start, id) : Bool}:
  match ns:
    case Nil{}:
      {==}
    case S.Node{+key, payload, children, refs} <> +rest:
      +keys = S.ids(rest)
      +length = List.length(&2, S.Node, rest)
      +same = Nat.is_eq(key, start)
      +tail = dense(rest, 1n+start)
      D.duplicate_for(Bool, same && tail, True{}, proof,
        {Nat.is_eq(key, id) || S.member(keys, id) ==
          Nat.is_eq(start, id) || interval(length, 1n+start, id) : Bool},
        head => remaining =>
          Equal.trans(Bool, Nat.is_eq(key, id) || S.member(keys, id),
            Nat.is_eq(start, id) || S.member(keys, id),
            Nat.is_eq(start, id) || interval(length, 1n+start, id),
            Equal.cong(Nat, Bool, n => Nat.is_eq(n, id) || S.member(keys, id), key, start,
              D.nat_equal(key, start, D.and_left(same, tail, head))),
            Equal.cong(Bool, Bool, part => Nat.is_eq(start, id) || part,
              S.member(keys, id), interval(length, 1n+start, id),
              dense_interval(rest, 1n+start, id, D.and_right(same, tail, remaining)))))

law dense_membership:
  for +ns: List<&2, S.Node>
  for +id: Nat
  for proof: {dense(ns, 1n) == True{} : Bool}
  {S.member(S.ids(ns), id) == in_range(id, List.length(&2, S.Node, ns)) : Bool}

def dense_membership(ns, id, proof):
  +length = List.length(&2, S.Node, ns)
  Equal.trans(Bool, S.member(S.ids(ns), id), interval(length, 1n, id),
    in_range(id, length), dense_interval(ns, 1n, id, proof), interval_range(length, id))

def references_equivalent(+rs: List<&2, S.Ref>, +ns: List<&2, S.Node>,
  proof: {dense(ns, 1n) == True{} : Bool}) ->
  {references(rs, List.length(&2, S.Node, ns)) == S.refs_valid(rs, S.ids(ns)) : Bool}:
  match rs:
    case Nil{}:
      {==}
    case S.Ref{+target, +reason} <> +tail:
      +limit = List.length(&2, S.Node, ns)
      +keys = S.ids(ns)
      +bounds = in_range(target, limit)
      +present = S.member(keys, target)
      +reasoned = Nat.is_ne(reason, 0n)
      +fast_tail = references(tail, limit)
      +spec_tail = S.refs_valid(tail, keys)
      D.duplicate_for(Bool, dense(ns, 1n), True{}, proof,
        {bounds && reasoned && fast_tail == present && reasoned && spec_tail : Bool},
        head => rest =>
          Equal.trans(Bool, bounds && reasoned && fast_tail,
            present && reasoned && fast_tail, present && reasoned && spec_tail,
            Equal.cong(Bool, Bool, value => value && reasoned && fast_tail,
              bounds, present, Equal.sym(Bool, present, bounds, dense_membership(ns, target, head))),
            Equal.cong(Bool, Bool, value => present && reasoned && value,
              fast_tail, spec_tail, references_equivalent(tail, ns, rest))))

def members(ids: List<&2, Nat>, +limit: Nat) -> Bool:
  match ids:
    case Nil{}:
      True{}
    case id <> rest:
      in_range(id, limit) && members(rest, limit)

def members_equivalent(+ids: List<&2, Nat>, +ns: List<&2, S.Node>,
  proof: {dense(ns, 1n) == True{} : Bool}) ->
  {members(ids, List.length(&2, S.Node, ns)) == S.members(ids, S.ids(ns)) : Bool}:
  match ids:
    case Nil{}:
      {==}
    case +id <> +rest:
      +limit = List.length(&2, S.Node, ns)
      +keys = S.ids(ns)
      +observed = members(id <> rest, limit)
      D.duplicate_for(Bool, dense(ns, 1n), True{}, proof,
        {observed == S.members(id <> rest, keys) : Bool},
        head => tail =>
          (%Equal.sym(Bool, S.member(keys, id), in_range(id, limit), dense_membership(ns, id, head)) :
             {observed == _ && S.members(rest, keys) : Bool}
           %members_equivalent(rest, ns, tail) : {observed == in_range(id, limit) && _ : Bool}
           {==}))

def next_plus_one(+n: Nat) -> {Nat.add(n, 1n) == 1n+n : Nat}:
  match n:
    case 0n:
      {==}
    case 1n+rest:
      Equal.cong(Nat, Nat, v => 1n+v, Nat.add(rest, 1n), 1n+rest, next_plus_one(rest))

def absent_below(+start: Nat, +length: Nat) ->
  {interval(length, Nat.add(start, 1n), start) == False{} : Bool}:
  match start:
    case 0n:
      interval_excludes_zero(length, 0n)
    case 1n+rest:
      Equal.trans(Bool, interval(length, Nat.add(1n+rest, 1n), 1n+rest),
        interval(length, Nat.add(rest, 1n), rest), False{},
        interval_shift(length, Nat.add(rest, 1n), rest), absent_below(rest, length))

def dense_tail_absent(+ns: List<&2, S.Node>, +start: Nat,
  proof: {dense(ns, 1n+start) == True{} : Bool}) ->
  {S.member(S.ids(ns), start) == False{} : Bool}:
  +length = List.length(&2, S.Node, ns)
  Equal.trans(Bool, S.member(S.ids(ns), start), interval(length, 1n+start, start), False{},
    dense_interval(ns, 1n+start, start, proof),
    (%next_plus_one(start) : {interval(length, _, start) == False{} : Bool}
     absent_below(start, length)))

def dense_unique(+ns: List<&2, S.Node>, +start: Nat,
  proof: {dense(ns, start) == True{} : Bool}) ->
  {S.unique(S.ids(ns)) == True{} : Bool}:
  match ns:
    case Nil{}:
      {==}
    case S.Node{+key, payload, children, refs} <> +rest:
      +keys = S.ids(rest)
      +same = Nat.is_eq(key, start)
      +tail = dense(rest, 1n+start)
      +claim = S.unique(key <> keys)
      D.duplicate_for(Bool, same && tail, True{}, proof, {claim == True{} : Bool},
        head => remaining =>
          D.duplicate_for(Bool, tail, True{}, D.and_right(same, tail, remaining), {claim == True{} : Bool},
            tail_absent => tail_unique =>
              D.and_true(Bool.not(S.member(keys, key)), S.unique(keys),
                Equal.cong(Bool, Bool, Bool.not, S.member(keys, key), False{},
                  Equal.trans(Bool, S.member(keys, key), S.member(keys, start), False{},
                    Equal.cong(Nat, Bool, n => S.member(keys, n), key, start,
                      D.nat_equal(key, start, D.and_left(same, tail, head))),
                    dense_tail_absent(rest, start, tail_absent))),
                dense_unique(rest, 1n+start, tail_unique))))
