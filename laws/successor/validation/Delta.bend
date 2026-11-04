import Base
import ../../Spec.bend as S

# Proof-only root predicates allow adjoining a cut or eliminating one root
# without changing the frozen graph representation.
type Roots is Data:
  Set{ids: List<&2, Nat>}
  Union{left: Roots, right: Roots}
  Above{nodes: List<&2, S.Node>, target: Nat}

def has(roots: Roots, +x: Nat) -> Bool:
  match roots:
    case Set{ids}:
      S.member(ids, x)
    case Union{left, right}:
      has(left, x) || has(right, x)
    case Above{nodes, target}:
      Nat.is_eq(S.parent(nodes, x), target)

def bool_cases(+value: Bool, +claim: Bool,
  no: {value == False{} : Bool} -> {claim == True{} : Bool},
  yes: {value == True{} : Bool} -> {claim == True{} : Bool}) -> {claim == True{} : Bool}:
  match value:
    case False{}:
      %no({==}) : {claim == _ : Bool}
      {==}
    case True{}:
      %yes({==}) : {claim == _ : Bool}
      {==}

def true_of_equal(+a: Bool, +b: Bool, same: {a == b : Bool}, valid: {b == True{} : Bool}) -> {a == True{} : Bool}:
  Equal.trans(Bool, a, b, True{}, same, valid)

def or_left_true(+a: Bool, +b: Bool, valid: {a == True{} : Bool}) -> {a || b == True{} : Bool}:
  %valid : {a || b == _ || b : Bool}
  {==}

def or_right_true(+a: Bool, +b: Bool, valid: {b == True{} : Bool}) -> {a || b == True{} : Bool}:
  match a:
    case False{}:
      valid
    case True{}:
      {==}

def or_drop_left(+a: Bool, +b: Bool, empty: {a == False{} : Bool}, valid: {a || b == True{} : Bool}) -> {b == True{} : Bool}:
  Equal.trans(Bool, b, a || b, True{},
    Equal.sym(Bool, a || b, b, Equal.cong(Bool, Bool, h => h || b, a, False{}, empty)), valid)

def eq_cases(+value: Bool, -A: Type, -a: A, -b: A,
  no: {value == False{} : Bool} -> {a == b : A},
  yes: {value == True{} : Bool} -> {a == b : A}) -> {a == b : A}:
  match value:
    case False{}:
      %no({==}) : {a == _ : A}
      {==}
    case True{}:
      %yes({==}) : {a == _ : A}
      {==}

def duplicate(-A: Type, -a: A, -b: A, e: {a == b : A}) -> {a == b : A} & {a == b : A}:
  %e : {a == _ : A} & {a == _ : A}
  ({==}, {==})

def use_duplicates(-A: Type, -a: A, -b: A, -R: Type,
  pair: {a == b : A} & {a == b : A},
  next: {a == b : A} -> {a == b : A} -> R) -> R:
  match pair:
    case (first, second):
      next(first, second)

def duplicate_for(-A: Type, -a: A, -b: A, e: {a == b : A}, -R: Type,
  next: {a == b : A} -> {a == b : A} -> R) -> R:
  use_duplicates(A, a, b, R, duplicate(A, a, b, e), next)

def use_bool_duplicates(+value: Bool, +expected: Bool, +claim: Bool,
  pair: {value == expected : Bool} & {value == expected : Bool},
  next: {value == expected : Bool} -> {value == expected : Bool} -> {claim == True{} : Bool}) -> {claim == True{} : Bool}:
  match pair:
    case (first, second):
      next(first, second)

def duplicate_bool(+value: Bool, +expected: Bool, e: {value == expected : Bool}, +claim: Bool,
  next: {value == expected : Bool} -> {value == expected : Bool} -> {claim == True{} : Bool}) -> {claim == True{} : Bool}:
  use_bool_duplicates(value, expected, claim, duplicate(Bool, value, expected, e), next)

def false_disc(value: Bool) -> Type:
  match value:
    case False{}:
      Unit
    case True{}:
      Empty

def false_true(e: {False{} == True{} : Bool}) -> Empty:
  %e : false_disc(_)
  Unit{}

def and_true(+a: Bool, +b: Bool, ea: {a == True{} : Bool}, eb: {b == True{} : Bool}) -> {a && b == True{} : Bool}:
  Equal.trans(Bool, a && b, b, True{}, Equal.cong(Bool, Bool, v => v && b, a, True{}, ea), eb)

def and_left(+a: Bool, +b: Bool, valid: {a && b == True{} : Bool}) -> {a == True{} : Bool}:
  match a:
    case False{}:
      valid
    case True{}:
      {==}

def and_right(+a: Bool, +b: Bool, valid: {a && b == True{} : Bool}) -> {b == True{} : Bool}:
  match a:
    case False{}:
      Empty.absurd({b == True{} : Bool}, false_true(valid))
    case True{}:
      valid

def nat_equal(+a: Nat, +b: Nat, valid: {Nat.is_eq(a, b) == True{} : Bool}) -> {a == b : Nat}:
  match a b:
    case 0n 0n:
      {==}
    case 0n 1n+bp:
      Empty.absurd({0n == 1n+bp : Nat}, false_true(valid))
    case 1n+ap 0n:
      Empty.absurd({1n+ap == 0n : Nat}, false_true(valid))
    case 1n+ap 1n+bp:
      Equal.cong(Nat, Nat, v => 1n+v, ap, bp, nat_equal(ap, bp, valid))

def nat_equal_symmetric(+a: Nat, +b: Nat) -> {Nat.is_eq(a, b) == Nat.is_eq(b, a) : Bool}:
  match a b:
    case 0n 0n:
      {==}
    case 0n 1n+bp:
      {==}
    case 1n+ap 0n:
      {==}
    case 1n+ap 1n+bp:
      nat_equal_symmetric(ap, bp)

def remove_one(xs: List<&2, Nat>, +x: Nat) -> List<&2, Nat>:
  match xs:
    case Nil{}:
      []
    case +h <> +t:
      Bool.pick(List<&2, Nat>, Nat.is_eq(h, x), t, h <> remove_one(t, x))

def remove_step(found: Bool, h: Nat, +t: List<&2, Nat>, x: Nat) -> List<&2, Nat>:
  Bool.pick(List<&2, Nat>, found, t, h <> remove_one(t, x))

def remove_size(+xs: List<&2, Nat>, +x: Nat,
  present: {S.member(xs, x) == True{} : Bool}) ->
  {1n+List.length(&2, Nat, remove_one(xs, x)) == List.length(&2, Nat, xs) : Nat}:
  match xs:
    case Nil{}:
      Empty.absurd({1n+List.length(&2, Nat, remove_one([], x)) == 0n : Nat}, false_true(present))
    case +h <> +t:
      eq_cases(Nat.is_eq(h, x), Nat, 1n+List.length(&2, Nat, remove_one(h <> t, x)), List.length(&2, Nat, h <> t),
        no => duplicate_for(Bool, Nat.is_eq(h, x), False{}, no,
          {1n+List.length(&2, Nat, remove_one(h <> t, x)) == List.length(&2, Nat, h <> t) : Nat},
          no_shape => no_member => Equal.trans(Nat, 1n+List.length(&2, Nat, remove_one(h <> t, x)),
          1n+1n+List.length(&2, Nat, remove_one(t, x)), List.length(&2, Nat, h <> t),
          Equal.cong(Bool, Nat, b => 1n+List.length(&2, Nat, remove_step(b, h, t, x)), Nat.is_eq(h, x), False{}, no_shape),
          Equal.cong(Nat, Nat, n => 1n+n, 1n+List.length(&2, Nat, remove_one(t, x)), List.length(&2, Nat, t),
            remove_size(t, x, or_drop_left(Nat.is_eq(h, x), S.member(t, x), no_member, present))))),
        yes => Equal.cong(Bool, Nat, b => 1n+List.length(&2, Nat, remove_step(b, h, t, x)), Nat.is_eq(h, x), True{}, yes))

def remove_member(+xs: List<&2, Nat>, +x: Nat, +y: Nat,
  different: {Nat.is_eq(y, x) == False{} : Bool},
  present: {S.member(xs, y) == True{} : Bool}) -> {S.member(remove_one(xs, x), y) == True{} : Bool}:
  match xs:
    case Nil{}:
      Empty.absurd({S.member(remove_one([], x), y) == True{} : Bool}, false_true(present))
    case +h <> +t:
      duplicate_bool(Nat.is_eq(y, x), False{}, different, S.member(remove_one(h <> t, x), y), different_no => different_yes =>
      duplicate_bool(S.member(h <> t, y), True{}, present, S.member(remove_one(h <> t, x), y), present_no => present_yes =>
      bool_cases(Nat.is_eq(h, x), S.member(remove_one(h <> t, x), y),
        no => true_of_equal(S.member(remove_one(h <> t, x), y), S.member(h <> remove_one(t, x), y),
          Equal.cong(Bool, Bool, b => S.member(remove_step(b, h, t, x), y), Nat.is_eq(h, x), False{}, no),
          bool_cases(Nat.is_eq(h, y), S.member(h <> remove_one(t, x), y),
            absent => or_right_true(Nat.is_eq(h, y), S.member(remove_one(t, x), y),
              remove_member(t, x, y, different_no, or_drop_left(Nat.is_eq(h, y), S.member(t, y), absent, present_no))),
            found => or_left_true(Nat.is_eq(h, y), S.member(remove_one(t, x), y), found))),
        yes => duplicate_bool(Nat.is_eq(h, x), True{}, yes, S.member(remove_one(h <> t, x), y), yes_shape => yes_target =>
          true_of_equal(S.member(remove_one(h <> t, x), y), S.member(t, y),
          Equal.cong(Bool, Bool, b => S.member(remove_step(b, h, t, x), y), Nat.is_eq(h, x), True{}, yes_shape),
          or_drop_left(Nat.is_eq(h, y), S.member(t, y),
            Equal.trans(Bool, Nat.is_eq(h, y), Nat.is_eq(x, y), False{},
              Equal.cong(Nat, Bool, v => Nat.is_eq(v, y), h, x, nat_equal(h, x, yes_target)),
              Equal.trans(Bool, Nat.is_eq(x, y), Nat.is_eq(y, x), False{}, nat_equal_symmetric(x, y), different_yes)), present_yes))))))

# The complete native frontier uses stable semantic identities. Off that
# frontier, root membership and the selected unique parent agree with the base.
# This first bridge is independent of how those identities are encoded.
def rooted(fuel: Nat, +x: Nat, +nodes: List<&2, S.Node>, +root: Roots) -> Bool:
  match fuel:
    case 0n:
      has(root, x)
    case 1n+f:
      has(root, x) || rooted(f, S.parent(nodes, x), nodes, root)

def cut(fuel: Nat, +x: Nat, +nodes: List<&2, S.Node>, +root: Roots, +frontier: Roots) -> Bool:
  match fuel:
    case 0n:
      has(root, x) || has(frontier, x)
    case 1n+f:
      has(root, x) || has(frontier, x) || cut(f, S.parent(nodes, x), nodes, root, frontier)

law rooted_spec:
  for +fuel: Nat
  for +x: Nat
  for +ns: List<&2, S.Node>
  for +roots: List<&2, Nat>
  {rooted(fuel, x, ns, Set{roots})
    == S.rooted(fuel, x, ns, roots) : Bool}
def rooted_spec(fuel, x, ns, roots):
  match fuel:
    case 0n:
      {==}
    case 1n+f:
      %rooted_spec(f, S.parent(ns, x), ns, roots) :
        {rooted(1n+f, x, ns, Set{roots})
          == S.member(roots, x) || _ : Bool}
      {==}

def root_extends(+fuel: Nat, +extra: Nat, +x: Nat, +nodes: List<&2, S.Node>, +root: Roots,
  valid: {rooted(fuel, x, nodes, root) == True{} : Bool}) ->
  {rooted(Nat.add(fuel, extra), x, nodes, root) == True{} : Bool}:
  match fuel:
    case 0n:
      match extra:
        case 0n:
          valid
        case 1n+p:
          or_left_true(has(root, x), rooted(p, S.parent(nodes, x), nodes, root), valid)
    case 1n+f:
      bool_cases(has(root, x), rooted(Nat.add(1n+f, extra), x, nodes, root),
        no => or_right_true(has(root, x), rooted(Nat.add(f, extra), S.parent(nodes, x), nodes, root),
          root_extends(f, extra, S.parent(nodes, x), nodes, root,
            or_drop_left(has(root, x), rooted(f, S.parent(nodes, x), nodes, root), no, valid))),
        yes => or_left_true(has(root, x), rooted(Nat.add(f, extra), S.parent(nodes, x), nodes, root), yes))

def closed(xs: List<&2, Nat>, +keys: List<&2, Nat>, +nodes: List<&2, S.Node>, +root: Roots) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case +h <> t:
      (has(root, h) || S.member(keys, S.parent(nodes, h))) && closed(t, keys, nodes, root)

def closed_at(+xs: List<&2, Nat>, +keys: List<&2, Nat>, +nodes: List<&2, S.Node>, +root: Roots, +x: Nat,
  present: {S.member(xs, x) == True{} : Bool}, valid: {closed(xs, keys, nodes, root) == True{} : Bool}) ->
  {has(root, x) || S.member(keys, S.parent(nodes, x)) == True{} : Bool}:
  match xs:
    case Nil{}:
      Empty.absurd({has(root, x) || S.member(keys, S.parent(nodes, x)) == True{} : Bool}, false_true(present))
    case +h <> +t:
      duplicate_bool(closed(h <> t, keys, nodes, root), True{}, valid,
        has(root, x) || S.member(keys, S.parent(nodes, x)), valid_no => valid_yes =>
        bool_cases(Nat.is_eq(h, x), has(root, x) || S.member(keys, S.parent(nodes, x)),
          no => closed_at(t, keys, nodes, root, x, or_drop_left(Nat.is_eq(h, x), S.member(t, x), no, present),
            and_right(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(t, keys, nodes, root), valid_no)),
          yes => true_of_equal(has(root, x) || S.member(keys, S.parent(nodes, x)),
            has(root, h) || S.member(keys, S.parent(nodes, h)),
            Equal.sym(Bool, has(root, h) || S.member(keys, S.parent(nodes, h)), has(root, x) || S.member(keys, S.parent(nodes, x)),
              Equal.cong(Nat, Bool, y => has(root, y) || S.member(keys, S.parent(nodes, y)), h, x, nat_equal(h, x, yes))),
            and_left(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(t, keys, nodes, root), valid_yes))))

def choose_vertex(done: Bool, x: Nat, next: Nat) -> Nat:
  match done:
    case True{}:
      x
    case False{}:
      next

def last_root(fuel: Nat, +x: Nat, +nodes: List<&2, S.Node>, +root: Roots) -> Nat:
  match fuel:
    case 0n:
      x
    case 1n+f:
      choose_vertex(has(root, x), x, last_root(f, S.parent(nodes, x), nodes, root))

def root_witness(+fuel: Nat, +x: Nat, +keys: List<&2, Nat>, +nodes: List<&2, S.Node>, +root: Roots,
  present: {S.member(keys, x) == True{} : Bool},
  endpoints: {closed(keys, keys, nodes, root) == True{} : Bool},
  valid: {rooted(fuel, x, nodes, root) == True{} : Bool}) ->
  {S.member(keys, last_root(fuel, x, nodes, root)) && has(root, last_root(fuel, x, nodes, root)) == True{} : Bool}:
  match fuel:
    case 0n:
      and_true(S.member(keys, x), has(root, x), present, valid)
    case 1n+f:
      duplicate_bool(S.member(keys, x), True{}, present,
        S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)), present_no => present_yes =>
        bool_cases(has(root, x), S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)),
          no => duplicate_bool(has(root, x), False{}, no,
            S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)), shape_no => rest_no =>
            duplicate_bool(has(root, x), False{}, rest_no,
              S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)), closed_no => path_no =>
              duplicate_bool(closed(keys, keys, nodes, root), True{}, endpoints,
                S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)), endpoints_at => endpoints_rec =>
                true_of_equal(S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)),
                  S.member(keys, last_root(f, S.parent(nodes, x), nodes, root)) && has(root, last_root(f, S.parent(nodes, x), nodes, root)),
                  Equal.cong(Bool, Bool, b => (
                    S.member(keys, choose_vertex(b, x, last_root(f, S.parent(nodes, x), nodes, root)))
                    && has(root, choose_vertex(b, x, last_root(f, S.parent(nodes, x), nodes, root)))), has(root, x), False{}, shape_no),
                  root_witness(f, S.parent(nodes, x), keys, nodes, root,
                    or_drop_left(has(root, x), S.member(keys, S.parent(nodes, x)), closed_no,
                      closed_at(keys, keys, nodes, root, x, present_no, endpoints_at)), endpoints_rec,
                    or_drop_left(has(root, x), rooted(f, S.parent(nodes, x), nodes, root), path_no, valid)))))),
          yes => duplicate_bool(has(root, x), True{}, yes,
            S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)), shape_yes => root_yes =>
            true_of_equal(S.member(keys, last_root(1n+f, x, nodes, root)) && has(root, last_root(1n+f, x, nodes, root)),
              S.member(keys, x) && has(root, x),
              Equal.cong(Bool, Bool, b => (
                S.member(keys, choose_vertex(b, x, last_root(f, S.parent(nodes, x), nodes, root)))
                && has(root, choose_vertex(b, x, last_root(f, S.parent(nodes, x), nodes, root)))), has(root, x), True{}, shape_yes),
              and_true(S.member(keys, x), has(root, x), present_yes, root_yes)))))

def predecessor(n: Nat) -> Nat:
  match n:
    case 0n:
      0n
    case 1n+p:
      p

def successor_injective(+a: Nat, +b: Nat, same: {1n+a == 1n+b : Nat}) -> {a == b : Nat}:
  Equal.cong(Nat, Nat, n => predecessor(n), 1n+a, 1n+b, same)

def root_at_any(+fuel: Nat, +x: Nat, +nodes: List<&2, S.Node>, +root: Roots,
  valid: {has(root, x) == True{} : Bool}) -> {rooted(fuel, x, nodes, root) == True{} : Bool}:
  match fuel:
    case 0n:
      valid
    case 1n+f:
      or_left_true(has(root, x), rooted(f, S.parent(nodes, x), nodes, root), valid)

def enlarge_root(+fuel: Nat, +x: Nat, +nodes: List<&2, S.Node>, +root: Roots, +r: Nat,
  valid: {rooted(fuel, x, nodes, root) == True{} : Bool}) ->
  {rooted(fuel, x, nodes, Union{root, Above{nodes, r}}) == True{} : Bool}:
  match fuel:
    case 0n:
      or_left_true(has(root, x), Nat.is_eq(S.parent(nodes, x), r), valid)
    case 1n+f:
      bool_cases(has(root, x), rooted(1n+f, x, nodes, Union{root, Above{nodes, r}}),
        no => or_right_true(has(Union{root, Above{nodes, r}}, x),
          rooted(f, S.parent(nodes, x), nodes, Union{root, Above{nodes, r}}),
          enlarge_root(f, S.parent(nodes, x), nodes, root, r,
            or_drop_left(has(root, x), rooted(f, S.parent(nodes, x), nodes, root), no, valid))),
        yes => root_at_any(1n+f, x, nodes, Union{root, Above{nodes, r}},
          or_left_true(has(root, x), Nat.is_eq(S.parent(nodes, x), r), yes)))

def original_root(+x: Nat, +nodes: List<&2, S.Node>, +root: Roots, +r: Nat,
  root_r: {has(root, r) == True{} : Bool},
  valid: {has(Union{root, Above{nodes, r}}, x) == True{} : Bool}) ->
  {rooted(1n, x, nodes, root) == True{} : Bool}:
  bool_cases(has(root, x), rooted(1n, x, nodes, root),
    no => or_right_true(has(root, x), has(root, S.parent(nodes, x)),
      true_of_equal(has(root, S.parent(nodes, x)), has(root, r),
        Equal.cong(Nat, Bool, y => has(root, y), S.parent(nodes, x), r,
          nat_equal(S.parent(nodes, x), r,
            or_drop_left(has(root, x), Nat.is_eq(S.parent(nodes, x), r), no, valid))), root_r)),
    yes => root_at_any(1n, x, nodes, root, yes))

def unexpand_root(+fuel: Nat, +x: Nat, +nodes: List<&2, S.Node>, +root: Roots, +r: Nat,
  root_r: {has(root, r) == True{} : Bool},
  valid: {rooted(fuel, x, nodes, Union{root, Above{nodes, r}}) == True{} : Bool}) ->
  {rooted(1n+fuel, x, nodes, root) == True{} : Bool}:
  match fuel:
    case 0n:
      original_root(x, nodes, root, r, root_r, valid)
    case 1n+f:
      duplicate_bool(has(root, r), True{}, root_r, rooted(1n+1n+f, x, nodes, root), root_no => root_yes =>
      duplicate_bool(rooted(1n+f, x, nodes, Union{root, Above{nodes, r}}), True{}, valid,
        rooted(1n+1n+f, x, nodes, root), valid_no => valid_yes =>
        bool_cases(has(Union{root, Above{nodes, r}}, x), rooted(1n+1n+f, x, nodes, root),
          no => or_right_true(has(root, x), rooted(1n+f, S.parent(nodes, x), nodes, root),
            unexpand_root(f, S.parent(nodes, x), nodes, root, r, root_no,
              or_drop_left(has(Union{root, Above{nodes, r}}, x),
                rooted(f, S.parent(nodes, x), nodes, Union{root, Above{nodes, r}}), no, valid_no))),
          yes => root_extends(1n, 1n+f, x, nodes, root,
            original_root(x, nodes, root, r, root_yes, yes)))))

def endpoint_after_remove(+keys: List<&2, Nat>, +nodes: List<&2, S.Node>, +root: Roots, +r: Nat, +x: Nat,
  valid: {has(root, x) || S.member(keys, S.parent(nodes, x)) == True{} : Bool}) ->
  {has(Union{root, Above{nodes, r}}, x) || S.member(remove_one(keys, r), S.parent(nodes, x)) == True{} : Bool}:
  bool_cases(has(root, x), has(Union{root, Above{nodes, r}}, x) || S.member(remove_one(keys, r), S.parent(nodes, x)),
    no => bool_cases(Nat.is_eq(S.parent(nodes, x), r),
      has(Union{root, Above{nodes, r}}, x) || S.member(remove_one(keys, r), S.parent(nodes, x)),
      different => or_right_true(has(Union{root, Above{nodes, r}}, x), S.member(remove_one(keys, r), S.parent(nodes, x)),
        remove_member(keys, r, S.parent(nodes, x), different,
          or_drop_left(has(root, x), S.member(keys, S.parent(nodes, x)), no, valid))),
      same => or_left_true(has(Union{root, Above{nodes, r}}, x), S.member(remove_one(keys, r), S.parent(nodes, x)),
        or_right_true(has(root, x), Nat.is_eq(S.parent(nodes, x), r), same))),
    yes => or_left_true(has(Union{root, Above{nodes, r}}, x), S.member(remove_one(keys, r), S.parent(nodes, x)),
      or_left_true(has(root, x), Nat.is_eq(S.parent(nodes, x), r), yes)))

def closed_smaller_domain(+xs: List<&2, Nat>, +keys: List<&2, Nat>, +nodes: List<&2, S.Node>, +root: Roots, +r: Nat,
  valid: {closed(xs, keys, nodes, root) == True{} : Bool}) ->
  {closed(xs, remove_one(keys, r), nodes, Union{root, Above{nodes, r}}) == True{} : Bool}:
  match xs:
    case Nil{}:
      {==}
    case +h <> +t:
      duplicate_bool(closed(h <> t, keys, nodes, root), True{}, valid,
        closed(h <> t, remove_one(keys, r), nodes, Union{root, Above{nodes, r}}), head_valid => tail_valid =>
        and_true(has(Union{root, Above{nodes, r}}, h) || S.member(remove_one(keys, r), S.parent(nodes, h)),
          closed(t, remove_one(keys, r), nodes, Union{root, Above{nodes, r}}),
          endpoint_after_remove(keys, nodes, root, r, h,
            and_left(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(t, keys, nodes, root), head_valid)),
          closed_smaller_domain(t, keys, nodes, root, r,
            and_right(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(t, keys, nodes, root), tail_valid))))

def closed_remove(+xs: List<&2, Nat>, +keys: List<&2, Nat>, +nodes: List<&2, S.Node>, +root: Roots, +r: Nat,
  valid: {closed(xs, keys, nodes, root) == True{} : Bool}) ->
  {closed(remove_one(xs, r), keys, nodes, root) == True{} : Bool}:
  match xs:
    case Nil{}:
      {==}
    case +h <> +t:
      duplicate_bool(closed(h <> t, keys, nodes, root), True{}, valid,
        closed(remove_one(h <> t, r), keys, nodes, root), valid_no => valid_yes =>
        bool_cases(Nat.is_eq(h, r), closed(remove_one(h <> t, r), keys, nodes, root),
          no => true_of_equal(closed(remove_one(h <> t, r), keys, nodes, root),
            closed(h <> remove_one(t, r), keys, nodes, root),
            Equal.cong(Bool, Bool, b => closed(remove_step(b, h, t, r), keys, nodes, root), Nat.is_eq(h, r), False{}, no),
            duplicate_bool(closed(h <> t, keys, nodes, root), True{}, valid_no,
              closed(h <> remove_one(t, r), keys, nodes, root), head_valid => tail_valid =>
              and_true(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(remove_one(t, r), keys, nodes, root),
                and_left(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(t, keys, nodes, root), head_valid),
                closed_remove(t, keys, nodes, root, r,
                  and_right(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(t, keys, nodes, root), tail_valid))))),
          yes => true_of_equal(closed(remove_one(h <> t, r), keys, nodes, root), closed(t, keys, nodes, root),
            Equal.cong(Bool, Bool, b => closed(remove_step(b, h, t, r), keys, nodes, root), Nat.is_eq(h, r), True{}, yes),
            and_right(has(root, h) || S.member(keys, S.parent(nodes, h)), closed(t, keys, nodes, root), valid_yes))))

def split_and(+a: Bool, +b: Bool, valid: {a && b == True{} : Bool}, -R: Type,
  next: {a == True{} : Bool} -> {b == True{} : Bool} -> R) -> R:
  duplicate_for(Bool, a && b, True{}, valid, R, first => second =>
    next(and_left(a, b, first), and_right(a, b, second)))

def zero_disc(value: Nat) -> Type:
  match value:
    case 0n:
      Empty
    case 1n+p:
      Unit

def successor_zero(+a: Nat, same: {1n+a == 0n : Nat}) -> Empty:
  %same : zero_disc(_)
  Unit{}

def empty_length_member(+keys: List<&2, Nat>, +x: Nat,
  size: {List.length(&2, Nat, keys) == 0n : Nat},
  present: {S.member(keys, x) == True{} : Bool}) -> Empty:
  match keys:
    case Nil{}:
      false_true(present)
    case h <> +t:
      successor_zero(List.length(&2, Nat, t), size)

# Induct on an explicit bound so the recursive call decreases structurally even
# though the root removed from keys need not be the list's first element.
def finite_bound(+bound: Nat, +keys: List<&2, Nat>, +fuel: Nat, +x: Nat,
  +nodes: List<&2, S.Node>, +root: Roots,
  size: {List.length(&2, Nat, keys) == bound : Nat},
  present: {S.member(keys, x) == True{} : Bool},
  endpoints: {closed(keys, keys, nodes, root) == True{} : Bool},
  valid: {rooted(fuel, x, nodes, root) == True{} : Bool}) ->
  {rooted(bound, x, nodes, root) == True{} : Bool}:
  match bound:
    case 0n:
      Empty.absurd({rooted(0n, x, nodes, root) == True{} : Bool}, empty_length_member(keys, x, size, present))
    case 1n+n:
      +r = last_root(fuel, x, nodes, root)
      duplicate_bool(S.member(keys, x), True{}, present, rooted(1n+n, x, nodes, root), present_witness => present_remove =>
      duplicate_bool(closed(keys, keys, nodes, root), True{}, endpoints, rooted(1n+n, x, nodes, root), endpoints_witness => endpoints_remove =>
      duplicate_bool(rooted(fuel, x, nodes, root), True{}, valid, rooted(1n+n, x, nodes, root), valid_witness => valid_enlarge =>
        split_and(S.member(keys, r), has(root, r),
          root_witness(fuel, x, keys, nodes, root, present_witness, endpoints_witness, valid_witness),
          {rooted(1n+n, x, nodes, root) == True{} : Bool}, root_member => root_r =>
          duplicate_bool(has(root, r), True{}, root_r, rooted(1n+n, x, nodes, root), root_no => root_yes =>
          bool_cases(Nat.is_eq(x, r), rooted(1n+n, x, nodes, root),
            different => unexpand_root(n, x, nodes, root, r, root_no,
              finite_bound(n, remove_one(keys, r), fuel, x, nodes, Union{root, Above{nodes, r}},
                successor_injective(List.length(&2, Nat, remove_one(keys, r)), n,
                  Equal.trans(Nat, 1n+List.length(&2, Nat, remove_one(keys, r)), List.length(&2, Nat, keys), 1n+n,
                    remove_size(keys, r, root_member), size)),
                remove_member(keys, r, x, different, present_remove),
                closed_remove(keys, remove_one(keys, r), nodes, Union{root, Above{nodes, r}}, r,
                  closed_smaller_domain(keys, keys, nodes, root, r, endpoints_remove)),
                enlarge_root(fuel, x, nodes, root, r, valid_enlarge))),
            same => root_at_any(1n+n, x, nodes, root,
              true_of_equal(has(root, x), has(root, r),
                Equal.cong(Nat, Bool, y => has(root, y), x, r, nat_equal(x, r, same)), root_yes))))))))

# A finite closed domain bounds every successful ancestor walk. The list may
# contain duplicate keys: its length still bounds the number of distinct keys.
law finite_rooted:
  for +keys: List<&2, Nat>
  for +fuel: Nat
  for +x: Nat
  for +nodes: List<&2, S.Node>
  for +root: Roots
  for present: {S.member(keys, x) == True{} : Bool}
  for endpoints: {closed(keys, keys, nodes, root) == True{} : Bool}
  for valid: {rooted(fuel, x, nodes, root) == True{} : Bool}
  {rooted(List.length(&2, Nat, keys), x, nodes, root) == True{} : Bool}
def finite_rooted(keys, fuel, x, nodes, root, present, endpoints, valid):
  finite_bound(List.length(&2, Nat, keys), keys, fuel, x, nodes, root, {==}, present, endpoints, valid)
