import Base
import ../../Spec.bend as S
import ./Delta.bend as D
import ./Frontier.bend as F
import ./ParentTable.bend as P
import ./ParentClosure.bend as C

# First-order agreement is indexed by common semantic identities. Native code
# supplies this complete mask from exact base/candidate views, never ordinal
# equality across two independently numbered views.
def same_bool(a: Bool, b: Bool) -> Bool:
  match a b:
    case False{} False{}:
      True{}
    case False{} True{}:
      False{}
    case True{} False{}:
      False{}
    case True{} True{}:
      True{}

def bool_equal(+a: Bool, +b: Bool, valid: {same_bool(a, b) == True{} : Bool}) -> {a == b : Bool}:
  match a b:
    case False{} False{}:
      {==}
    case False{} True{}:
      valid
    case True{} False{}:
      Empty.absurd({True{} == False{} : Bool}, D.false_true(valid))
    case True{} True{}:
      {==}

def placement(+ns: List<&2, S.Node>, +roots: List<&2, Nat>, +x: Nat) -> Nat:
  Nat.add(S.count(roots, x), S.parents(ns, x))

def frame_row(+x: Nat, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>) -> Bool:
  S.member(frontier, x) || (S.member(S.ids(bn), x) &&
    (same_bool(S.member(br, x), S.member(cr, x)) &&
      (Nat.is_eq(S.parent(bn, x), S.parent(cn, x)) &&
        Nat.is_eq(placement(bn, br, x), placement(cn, cr, x)))))

def frame(xs: List<&2, Nat>, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case h <> t:
      frame_row(h, bn, br, cn, cr, frontier) && frame(t, bn, br, cn, cr, frontier)

def frame_at(+xs: List<&2, Nat>, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>, +x: Nat,
  present: {S.member(xs, x) == True{} : Bool},
  valid: {frame(xs, bn, br, cn, cr, frontier) == True{} : Bool}) ->
  {frame_row(x, bn, br, cn, cr, frontier) == True{} : Bool}:
  match xs:
    case Nil{}:
      Empty.absurd({frame_row(x, bn, br, cn, cr, frontier) == True{} : Bool}, D.false_true(present))
    case +h <> +t:
      D.duplicate_bool(frame(h <> t, bn, br, cn, cr, frontier), True{}, valid,
        frame_row(x, bn, br, cn, cr, frontier), valid_no => valid_yes =>
        D.bool_cases(Nat.is_eq(h, x), frame_row(x, bn, br, cn, cr, frontier),
          no => frame_at(t, bn, br, cn, cr, frontier, x,
            D.or_drop_left(Nat.is_eq(h, x), S.member(t, x), no, present),
            D.and_right(frame_row(h, bn, br, cn, cr, frontier), frame(t, bn, br, cn, cr, frontier), valid_no)),
          yes => D.true_of_equal(frame_row(x, bn, br, cn, cr, frontier), frame_row(h, bn, br, cn, cr, frontier),
            Equal.sym(Bool, frame_row(h, bn, br, cn, cr, frontier), frame_row(x, bn, br, cn, cr, frontier),
              Equal.cong(Nat, Bool, y => frame_row(y, bn, br, cn, cr, frontier), h, x, D.nat_equal(h, x, yes))),
            D.and_left(frame_row(h, bn, br, cn, cr, frontier), frame(t, bn, br, cn, cr, frontier), valid_yes))))

def with_frame_off(+x: Nat, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  absent: {S.member(frontier, x) == False{} : Bool},
  valid: {frame_row(x, bn, br, cn, cr, frontier) == True{} : Bool}, -R: Type,
  next: {S.member(S.ids(bn), x) == True{} : Bool} ->
    {S.member(br, x) == S.member(cr, x) : Bool} ->
    {S.parent(bn, x) == S.parent(cn, x) : Nat} ->
    {placement(bn, br, x) == placement(cn, cr, x) : Nat} -> R) -> R:
  D.split_and(S.member(S.ids(bn), x),
    same_bool(S.member(br, x), S.member(cr, x)) &&
      (Nat.is_eq(S.parent(bn, x), S.parent(cn, x)) && Nat.is_eq(placement(bn, br, x), placement(cn, cr, x))),
    D.or_drop_left(S.member(frontier, x), S.member(S.ids(bn), x) &&
      (same_bool(S.member(br, x), S.member(cr, x)) &&
        (Nat.is_eq(S.parent(bn, x), S.parent(cn, x)) && Nat.is_eq(placement(bn, br, x), placement(cn, cr, x)))), absent, valid),
    R, present => rest =>
    D.split_and(same_bool(S.member(br, x), S.member(cr, x)),
      Nat.is_eq(S.parent(bn, x), S.parent(cn, x)) && Nat.is_eq(placement(bn, br, x), placement(cn, cr, x)), rest,
      R, roots => parents_counts =>
      D.split_and(Nat.is_eq(S.parent(bn, x), S.parent(cn, x)), Nat.is_eq(placement(bn, br, x), placement(cn, cr, x)),
        parents_counts, R, parents => counts =>
        next(present, bool_equal(S.member(br, x), S.member(cr, x), roots),
          D.nat_equal(S.parent(bn, x), S.parent(cn, x), parents),
          D.nat_equal(placement(bn, br, x), placement(cn, cr, x), counts)))))

# Follow a successful base walk in the candidate until either a candidate root
# or a changed-placement frontier key is reached. The reusable assumptions are
# finite data and equality evidence, rather than affine pointwise callbacks.
def cut_transport(+fuel: Nat, +x: Nat, +keys: List<&2, Nat>,
  +bn: List<&2, S.Node>, +br: List<&2, Nat>, +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  present: {S.member(keys, x) == True{} : Bool},
  endpoints: {D.closed(keys, keys, cn, D.Set{cr}) == True{} : Bool},
  agreement: {frame(keys, bn, br, cn, cr, frontier) == True{} : Bool},
  valid: {D.rooted(fuel, x, bn, D.Set{br}) == True{} : Bool}) ->
  {D.rooted(fuel, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}) == True{} : Bool}:
  match fuel:
    case 0n:
      D.bool_cases(S.member(frontier, x), D.rooted(0n, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}),
        no => with_frame_off(x, bn, br, cn, cr, frontier, no,
          frame_at(keys, bn, br, cn, cr, frontier, x, present, agreement),
          {D.rooted(0n, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}) == True{} : Bool},
          old_present => roots => parents => counts =>
          D.or_left_true(S.member(cr, x), S.member(frontier, x),
            D.true_of_equal(S.member(cr, x), S.member(br, x), Equal.sym(Bool, S.member(br, x), S.member(cr, x), roots), valid))),
        yes => D.or_right_true(S.member(cr, x), S.member(frontier, x), yes))
    case 1n+f:
      D.duplicate_bool(S.member(keys, x), True{}, present,
        D.rooted(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}), present_frame => present_closed =>
      D.duplicate_bool(D.closed(keys, keys, cn, D.Set{cr}), True{}, endpoints,
        D.rooted(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}), closed_at => closed_rec =>
      D.duplicate_bool(frame(keys, bn, br, cn, cr, frontier), True{}, agreement,
        D.rooted(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}), frame_now => frame_rec =>
      D.bool_cases(S.member(frontier, x), D.rooted(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}),
        no => with_frame_off(x, bn, br, cn, cr, frontier, no,
          frame_at(keys, bn, br, cn, cr, frontier, x, present_frame, frame_now),
          {D.rooted(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}) == True{} : Bool},
          old_present => roots => parents => counts =>
          D.bool_cases(S.member(cr, x), D.rooted(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}),
            root_no => D.duplicate_bool(S.member(cr, x), False{}, root_no,
              D.rooted(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}}), no_closed => no_base =>
              D.or_right_true(D.has(D.Union{D.Set{cr}, D.Set{frontier}}, x),
                D.rooted(f, S.parent(cn, x), cn, D.Union{D.Set{cr}, D.Set{frontier}}),
                cut_transport(f, S.parent(cn, x), keys, bn, br, cn, cr, frontier,
                  D.or_drop_left(S.member(cr, x), S.member(keys, S.parent(cn, x)), no_closed,
                    D.closed_at(keys, keys, cn, D.Set{cr}, x, present_closed, closed_at)), closed_rec, frame_rec,
                  D.true_of_equal(D.rooted(f, S.parent(cn, x), bn, D.Set{br}), D.rooted(f, S.parent(bn, x), bn, D.Set{br}),
                    Equal.sym(Bool, D.rooted(f, S.parent(bn, x), bn, D.Set{br}), D.rooted(f, S.parent(cn, x), bn, D.Set{br}),
                      Equal.cong(Nat, Bool, y => D.rooted(f, y, bn, D.Set{br}), S.parent(bn, x), S.parent(cn, x), parents)),
                    D.or_drop_left(S.member(br, x), D.rooted(f, S.parent(bn, x), bn, D.Set{br}),
                      Equal.trans(Bool, S.member(br, x), S.member(cr, x), False{}, roots, no_base), valid))))),
            root_yes => D.root_at_any(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}},
              D.or_left_true(S.member(cr, x), S.member(frontier, x), root_yes)))),
        yes => D.root_at_any(1n+f, x, cn, D.Union{D.Set{cr}, D.Set{frontier}},
          D.or_right_true(S.member(cr, x), S.member(frontier, x), yes))))))

def frontier_row(+x: Nat, +ns: List<&2, S.Node>, +roots: List<&2, Nat>, +keys: List<&2, Nat>, +fuel: Nat) -> Bool:
  S.member(keys, x) && Nat.is_eq(placement(ns, roots, x), 1n) && P.rooted(fuel, x, P.build(ns), roots)

def frontier_at(+xs: List<&2, Nat>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>,
  +keys: List<&2, Nat>, +fuel: Nat, +x: Nat,
  present: {S.member(xs, x) == True{} : Bool},
  valid: {F.placements(xs, ns, roots, keys, fuel, P.build(ns)) == True{} : Bool}) ->
  {frontier_row(x, ns, roots, keys, fuel) == True{} : Bool}:
  match xs:
    case Nil{}:
      Empty.absurd({frontier_row(x, ns, roots, keys, fuel) == True{} : Bool}, D.false_true(present))
    case +h <> +t:
      D.duplicate_bool(F.placements(h <> t, ns, roots, keys, fuel, P.build(ns)), True{}, valid,
        frontier_row(x, ns, roots, keys, fuel), valid_no => valid_yes =>
        D.bool_cases(Nat.is_eq(h, x), frontier_row(x, ns, roots, keys, fuel),
          no => frontier_at(t, ns, roots, keys, fuel, x,
            D.or_drop_left(Nat.is_eq(h, x), S.member(t, x), no, present),
            D.and_right(frontier_row(h, ns, roots, keys, fuel), F.placements(t, ns, roots, keys, fuel, P.build(ns)), valid_no)),
          yes => D.true_of_equal(frontier_row(x, ns, roots, keys, fuel), frontier_row(h, ns, roots, keys, fuel),
            Equal.sym(Bool, frontier_row(h, ns, roots, keys, fuel), frontier_row(x, ns, roots, keys, fuel),
              Equal.cong(Nat, Bool, y => frontier_row(y, ns, roots, keys, fuel), h, x, D.nat_equal(h, x, yes))),
            D.and_left(frontier_row(h, ns, roots, keys, fuel), F.placements(t, ns, roots, keys, fuel, P.build(ns)), valid_yes))))

def frontier_rooted(+frontier: List<&2, Nat>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>,
  +keys: List<&2, Nat>, +fuel: Nat, +x: Nat,
  present: {S.member(frontier, x) == True{} : Bool},
  valid: {F.placements(frontier, ns, roots, keys, fuel, P.build(ns)) == True{} : Bool}) ->
  {D.rooted(fuel, x, ns, D.Set{roots}) == True{} : Bool}:
  D.true_of_equal(D.rooted(fuel, x, ns, D.Set{roots}), S.rooted(fuel, x, ns, roots), D.rooted_spec(fuel, x, ns, roots),
    D.true_of_equal(S.rooted(fuel, x, ns, roots), P.rooted(fuel, x, P.build(ns), roots),
      Equal.sym(Bool, P.rooted(fuel, x, P.build(ns), roots), S.rooted(fuel, x, ns, roots), P.rooted_equivalent(fuel, x, ns, roots)),
      D.and_right(S.member(keys, x) && Nat.is_eq(placement(ns, roots, x), 1n), P.rooted(fuel, x, P.build(ns), roots),
        frontier_at(frontier, ns, roots, keys, fuel, x, present, valid))))

def frontier_placed(+frontier: List<&2, Nat>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>,
  +keys: List<&2, Nat>, +fuel: Nat, +x: Nat,
  present: {S.member(frontier, x) == True{} : Bool},
  valid: {F.placements(frontier, ns, roots, keys, fuel, P.build(ns)) == True{} : Bool}) ->
  {Nat.is_eq(placement(ns, roots, x), 1n) == True{} : Bool}:
  D.and_right(S.member(keys, x), Nat.is_eq(placement(ns, roots, x), 1n),
    D.and_left(S.member(keys, x) && Nat.is_eq(placement(ns, roots, x), 1n), P.rooted(fuel, x, P.build(ns), roots),
      frontier_at(frontier, ns, roots, keys, fuel, x, present, valid)))

law add_zero:
  for +a: Nat
  {Nat.add(a, 0n) == a : Nat}
def add_zero(a):
  match a:
    case 0n:
      {==}
    case 1n+p:
      %add_zero(p) : {Nat.add(1n+p, 0n) == 1n+_ : Nat}
      {==}

law add_successor:
  for +a: Nat
  for +b: Nat
  {Nat.add(a, 1n+b) == 1n+Nat.add(a, b) : Nat}
def add_successor(a, b):
  match a:
    case 0n:
      {==}
    case 1n+p:
      %add_successor(p, b) : {Nat.add(1n+p, 1n+b) == 1n+_ : Nat}
      {==}

law add_commute:
  for +a: Nat
  for +b: Nat
  {Nat.add(a, b) == Nat.add(b, a) : Nat}
def add_commute(a, b):
  match a:
    case 0n:
      Equal.sym(Nat, Nat.add(b, 0n), b, add_zero(b))
    case 1n+p:
      Equal.trans(Nat, Nat.add(1n+p, b), 1n+Nat.add(b, p), Nat.add(b, 1n+p),
        Equal.cong(Nat, Nat, n => 1n+n, Nat.add(p, b), Nat.add(b, p), add_commute(p, b)),
        Equal.sym(Nat, Nat.add(b, 1n+p), 1n+Nat.add(b, p), add_successor(b, p)))

def cut_root_path(+budget: Nat, +x: Nat, +ns: List<&2, S.Node>, +roots: List<&2, Nat>, +keys: List<&2, Nat>, +frontier: List<&2, Nat>,
  checked: {F.placements(frontier, ns, roots, keys, budget, P.build(ns)) == True{} : Bool},
  valid: {D.has(D.Union{D.Set{roots}, D.Set{frontier}}, x) == True{} : Bool}) ->
  {D.rooted(budget, x, ns, D.Set{roots}) == True{} : Bool}:
  D.bool_cases(S.member(roots, x), D.rooted(budget, x, ns, D.Set{roots}),
    no => frontier_rooted(frontier, ns, roots, keys, budget, x,
      D.or_drop_left(S.member(roots, x), S.member(frontier, x), no, valid), checked),
    yes => D.root_at_any(budget, x, ns, D.Set{roots}, yes))

def cut_lift(+fuel: Nat, +budget: Nat, +x: Nat, +ns: List<&2, S.Node>, +roots: List<&2, Nat>, +keys: List<&2, Nat>, +frontier: List<&2, Nat>,
  checked: {F.placements(frontier, ns, roots, keys, budget, P.build(ns)) == True{} : Bool},
  valid: {D.rooted(fuel, x, ns, D.Union{D.Set{roots}, D.Set{frontier}}) == True{} : Bool}) ->
  {D.rooted(Nat.add(fuel, budget), x, ns, D.Set{roots}) == True{} : Bool}:
  match fuel:
    case 0n:
      cut_root_path(budget, x, ns, roots, keys, frontier, checked, valid)
    case 1n+f:
      D.duplicate_bool(F.placements(frontier, ns, roots, keys, budget, P.build(ns)), True{}, checked,
        D.rooted(Nat.add(1n+f, budget), x, ns, D.Set{roots}), checked_no => checked_yes =>
        D.bool_cases(D.has(D.Union{D.Set{roots}, D.Set{frontier}}, x), D.rooted(Nat.add(1n+f, budget), x, ns, D.Set{roots}),
          no => D.or_right_true(S.member(roots, x), D.rooted(Nat.add(f, budget), S.parent(ns, x), ns, D.Set{roots}),
            cut_lift(f, budget, S.parent(ns, x), ns, roots, keys, frontier, checked_no,
              D.or_drop_left(D.has(D.Union{D.Set{roots}, D.Set{frontier}}, x),
                D.rooted(f, S.parent(ns, x), ns, D.Union{D.Set{roots}, D.Set{frontier}}), no, valid))),
          yes => D.true_of_equal(D.rooted(Nat.add(1n+f, budget), x, ns, D.Set{roots}),
            D.rooted(Nat.add(budget, 1n+f), x, ns, D.Set{roots}),
            Equal.cong(Nat, Bool, n => D.rooted(n, x, ns, D.Set{roots}), Nat.add(1n+f, budget), Nat.add(budget, 1n+f), add_commute(1n+f, budget)),
            D.root_extends(budget, 1n+f, x, ns, D.Set{roots}, cut_root_path(budget, x, ns, roots, keys, frontier, checked_yes, yes)))))

def node_shape(+i: Nat, +children: List<&2, Nat>, +keys: List<&2, Nat>) -> Bool:
  Nat.is_ne(i, 0n) && S.unique(children) && S.members(children, keys)

def node_local(+i: Nat, +children: List<&2, Nat>, +refs: List<&2, S.Ref>, +keys: List<&2, Nat>) -> Bool:
  node_shape(i, children, keys) && S.refs_valid(refs, keys)

def base_facts(+x: Nat, +ns: List<&2, S.Node>, +roots: List<&2, Nat>, +fuel: Nat) -> Bool:
  Nat.is_eq(placement(ns, roots, x), 1n) && S.rooted(fuel, x, ns, roots)

def drop_conjunct(+a: Bool, +b: Bool, +c: Bool, valid: {a && b && c == True{} : Bool}) -> {b && c == True{} : Bool}:
  D.split_and(a && b, c, valid, {b && c == True{} : Bool}, first => last =>
    D.and_true(b, c, D.and_right(a, b, first), last))

def base_at(+xs: List<&2, S.Node>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>,
  +keys: List<&2, Nat>, +fuel: Nat, +x: Nat,
  present: {S.member(S.ids(xs), x) == True{} : Bool},
  valid: {S.nodes_valid(xs, ns, roots, keys, fuel) == True{} : Bool}) ->
  {base_facts(x, ns, roots, fuel) == True{} : Bool}:
  match xs:
    case Nil{}:
      Empty.absurd({base_facts(x, ns, roots, fuel) == True{} : Bool}, D.false_true(present))
    case S.Node{+i, p, +children, +refs} <> +t:
      D.duplicate_bool(S.nodes_valid(S.Node{i, p, children, refs} <> t, ns, roots, keys, fuel), True{}, valid,
        base_facts(x, ns, roots, fuel), valid_no => valid_yes =>
        D.bool_cases(Nat.is_eq(i, x), base_facts(x, ns, roots, fuel),
          no => base_at(t, ns, roots, keys, fuel, x,
            D.or_drop_left(Nat.is_eq(i, x), S.member(S.ids(t), x), no, present),
            D.and_right(node_local(i, children, refs, keys) && Nat.is_eq(placement(ns, roots, i), 1n) && S.rooted(fuel, i, ns, roots),
              S.nodes_valid(t, ns, roots, keys, fuel), valid_no)),
          yes => D.true_of_equal(base_facts(x, ns, roots, fuel), base_facts(i, ns, roots, fuel),
            Equal.sym(Bool, base_facts(i, ns, roots, fuel), base_facts(x, ns, roots, fuel),
              Equal.cong(Nat, Bool, y => base_facts(y, ns, roots, fuel), i, x, D.nat_equal(i, x, yes))),
            drop_conjunct(node_local(i, children, refs, keys), Nat.is_eq(placement(ns, roots, i), 1n), S.rooted(fuel, i, ns, roots),
              D.and_left(node_local(i, children, refs, keys) && Nat.is_eq(placement(ns, roots, i), 1n) && S.rooted(fuel, i, ns, roots),
                S.nodes_valid(t, ns, roots, keys, fuel), valid_yes)))))

def base_nodes(+ns: List<&2, S.Node>, +roots: List<&2, Nat>,
  valid: {S.valid(S.Graph{ns, roots, 0n}) == True{} : Bool}) ->
  {S.nodes_valid(ns, ns, roots, S.ids(ns), List.length(&2, S.Node, ns)) == True{} : Bool}:
  D.and_right(S.unique(S.ids(ns)) && S.unique(roots) && S.members(roots, S.ids(ns)),
    S.nodes_valid(ns, ns, roots, S.ids(ns), List.length(&2, S.Node, ns)), valid)

def base_rooted(+ns: List<&2, S.Node>, +roots: List<&2, Nat>, +x: Nat,
  present: {S.member(S.ids(ns), x) == True{} : Bool},
  valid: {S.valid(S.Graph{ns, roots, 0n}) == True{} : Bool}) ->
  {D.rooted(List.length(&2, S.Node, ns), x, ns, D.Set{roots}) == True{} : Bool}:
  D.true_of_equal(D.rooted(List.length(&2, S.Node, ns), x, ns, D.Set{roots}), S.rooted(List.length(&2, S.Node, ns), x, ns, roots),
    D.rooted_spec(List.length(&2, S.Node, ns), x, ns, roots),
    D.and_right(Nat.is_eq(placement(ns, roots, x), 1n), S.rooted(List.length(&2, S.Node, ns), x, ns, roots),
      base_at(ns, ns, roots, S.ids(ns), List.length(&2, S.Node, ns), x, present, base_nodes(ns, roots, valid))))

def base_placed(+ns: List<&2, S.Node>, +roots: List<&2, Nat>, +x: Nat,
  present: {S.member(S.ids(ns), x) == True{} : Bool},
  valid: {S.valid(S.Graph{ns, roots, 0n}) == True{} : Bool}) ->
  {Nat.is_eq(placement(ns, roots, x), 1n) == True{} : Bool}:
  D.and_left(Nat.is_eq(placement(ns, roots, x), 1n), S.rooted(List.length(&2, S.Node, ns), x, ns, roots),
    base_at(ns, ns, roots, S.ids(ns), List.length(&2, S.Node, ns), x, present, base_nodes(ns, roots, valid)))

def placed_at(+x: Nat, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  present: {S.member(S.ids(cn), x) == True{} : Bool},
  base: {S.valid(S.Graph{bn, br, 0n}) == True{} : Bool},
  agreement: {frame(S.ids(cn), bn, br, cn, cr, frontier) == True{} : Bool},
  checked: {F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)) == True{} : Bool}) ->
  {Nat.is_eq(placement(cn, cr, x), 1n) == True{} : Bool}:
  D.bool_cases(S.member(frontier, x), Nat.is_eq(placement(cn, cr, x), 1n),
    no => with_frame_off(x, bn, br, cn, cr, frontier, no,
      frame_at(S.ids(cn), bn, br, cn, cr, frontier, x, present, agreement),
      {Nat.is_eq(placement(cn, cr, x), 1n) == True{} : Bool}, old_present => roots => parents => counts =>
      D.true_of_equal(Nat.is_eq(placement(cn, cr, x), 1n), Nat.is_eq(placement(bn, br, x), 1n),
        Equal.sym(Bool, Nat.is_eq(placement(bn, br, x), 1n), Nat.is_eq(placement(cn, cr, x), 1n),
          Equal.cong(Nat, Bool, n => Nat.is_eq(n, 1n), placement(bn, br, x), placement(cn, cr, x), counts)),
        base_placed(bn, br, x, old_present, base))),
    yes => frontier_placed(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), x, yes, checked))

def members_cons(+xs: List<&2, Nat>, +keys: List<&2, Nat>, +h: Nat,
  valid: {S.members(xs, keys) == True{} : Bool}) -> {S.members(xs, h <> keys) == True{} : Bool}:
  match xs:
    case Nil{}:
      {==}
    case +x <> +t:
      D.split_and(S.member(keys, x), S.members(t, keys), valid, {S.members(x <> t, h <> keys) == True{} : Bool}, head => tail =>
        D.and_true(S.member(h <> keys, x), S.members(t, h <> keys),
          D.or_right_true(Nat.is_eq(h, x), S.member(keys, x), head), members_cons(t, keys, h, tail)))

def members_self(+keys: List<&2, Nat>) -> {S.members(keys, keys) == True{} : Bool}:
  match keys:
    case Nil{}:
      {==}
    case +h <> +t:
      D.and_true(S.member(h <> t, h), S.members(t, h <> t),
        D.or_left_true(Nat.is_eq(h, h), S.member(t, h), C.nat_equal_refl(h)), members_cons(t, t, h, members_self(t)))

def all_placements(xs: List<&2, Nat>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case h <> t:
      Nat.is_eq(placement(ns, roots, h), 1n) && all_placements(t, ns, roots)

def placements_preserved(+xs: List<&2, Nat>, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  included: {S.members(xs, S.ids(cn)) == True{} : Bool},
  base: {S.valid(S.Graph{bn, br, 0n}) == True{} : Bool},
  agreement: {frame(S.ids(cn), bn, br, cn, cr, frontier) == True{} : Bool},
  checked: {F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)) == True{} : Bool}) ->
  {all_placements(xs, cn, cr) == True{} : Bool}:
  match xs:
    case Nil{}:
      {==}
    case +h <> +t:
      D.duplicate_bool(S.valid(S.Graph{bn, br, 0n}), True{}, base, all_placements(h <> t, cn, cr), base_head => base_tail =>
      D.duplicate_bool(frame(S.ids(cn), bn, br, cn, cr, frontier), True{}, agreement, all_placements(h <> t, cn, cr), frame_head => frame_tail =>
      D.duplicate_bool(F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)), True{}, checked,
        all_placements(h <> t, cn, cr), checked_head => checked_tail =>
      D.split_and(S.member(S.ids(cn), h), S.members(t, S.ids(cn)), included,
        {all_placements(h <> t, cn, cr) == True{} : Bool}, head => tail =>
        D.and_true(Nat.is_eq(placement(cn, cr, h), 1n), all_placements(t, cn, cr),
          placed_at(h, bn, br, cn, cr, frontier, head, base_head, frame_head, checked_head),
          placements_preserved(t, bn, br, cn, cr, frontier, tail, base_tail, frame_tail, checked_tail))))))

def placements_closed(+xs: List<&2, Nat>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>,
  placed: {all_placements(xs, ns, roots) == True{} : Bool}) ->
  {D.closed(xs, S.ids(ns), ns, D.Set{roots}) == True{} : Bool}:
  match xs:
    case Nil{}:
      {==}
    case +h <> +t:
      D.split_and(Nat.is_eq(placement(ns, roots, h), 1n), all_placements(t, ns, roots), placed,
        {D.closed(h <> t, S.ids(ns), ns, D.Set{roots}) == True{} : Bool}, head => tail =>
        D.and_true(S.member(roots, h) || S.member(S.ids(ns), S.parent(ns, h)), D.closed(t, S.ids(ns), ns, D.Set{roots}),
          C.placement_closed(ns, roots, h, head), placements_closed(t, ns, roots, tail)))

law ids_length:
  for +ns: List<&2, S.Node>
  {List.length(&2, Nat, S.ids(ns)) == List.length(&2, S.Node, ns) : Nat}
def ids_length(ns):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %ids_length(t) : {List.length(&2, Nat, S.ids(S.Node{i, p, c, r} <> t)) == 1n+_ : Nat}
      {==}

def rooted_from_base(+fuel: Nat, +budget: Nat, +x: Nat, +keys: List<&2, Nat>,
  +bn: List<&2, S.Node>, +br: List<&2, Nat>, +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  present: {S.member(keys, x) == True{} : Bool},
  endpoints: {D.closed(keys, keys, cn, D.Set{cr}) == True{} : Bool},
  agreement: {frame(keys, bn, br, cn, cr, frontier) == True{} : Bool},
  checked: {F.placements(frontier, cn, cr, keys, budget, P.build(cn)) == True{} : Bool},
  valid: {D.rooted(fuel, x, bn, D.Set{br}) == True{} : Bool}) ->
  {D.rooted(List.length(&2, Nat, keys), x, cn, D.Set{cr}) == True{} : Bool}:
  D.duplicate_bool(S.member(keys, x), True{}, present, D.rooted(List.length(&2, Nat, keys), x, cn, D.Set{cr}), present_finite => present_cut =>
  D.duplicate_bool(D.closed(keys, keys, cn, D.Set{cr}), True{}, endpoints, D.rooted(List.length(&2, Nat, keys), x, cn, D.Set{cr}), closed_finite => closed_cut =>
    D.finite_rooted(keys, Nat.add(fuel, budget), x, cn, D.Set{cr}, present_finite, closed_finite,
      cut_lift(fuel, budget, x, cn, cr, keys, frontier, checked,
        cut_transport(fuel, x, keys, bn, br, cn, cr, frontier, present_cut, closed_cut, agreement, valid)))))

def rooted_at(+x: Nat, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  present: {S.member(S.ids(cn), x) == True{} : Bool},
  base: {S.valid(S.Graph{bn, br, 0n}) == True{} : Bool},
  agreement: {frame(S.ids(cn), bn, br, cn, cr, frontier) == True{} : Bool},
  endpoints: {D.closed(S.ids(cn), S.ids(cn), cn, D.Set{cr}) == True{} : Bool},
  checked: {F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)) == True{} : Bool}) ->
  {S.rooted(List.length(&2, S.Node, cn), x, cn, cr) == True{} : Bool}:
  D.duplicate_bool(F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)), True{}, checked,
    S.rooted(List.length(&2, S.Node, cn), x, cn, cr), checked_no => checked_yes =>
  D.bool_cases(S.member(frontier, x), S.rooted(List.length(&2, S.Node, cn), x, cn, cr),
    no => D.duplicate_bool(S.member(S.ids(cn), x), True{}, present,
      S.rooted(List.length(&2, S.Node, cn), x, cn, cr), present_frame => present_path =>
      D.duplicate_bool(frame(S.ids(cn), bn, br, cn, cr, frontier), True{}, agreement,
        S.rooted(List.length(&2, S.Node, cn), x, cn, cr), frame_now => frame_path =>
      with_frame_off(x, bn, br, cn, cr, frontier, no,
        frame_at(S.ids(cn), bn, br, cn, cr, frontier, x, present_frame, frame_now),
        {S.rooted(List.length(&2, S.Node, cn), x, cn, cr) == True{} : Bool}, old_present => roots => parents => counts =>
        D.true_of_equal(S.rooted(List.length(&2, S.Node, cn), x, cn, cr), D.rooted(List.length(&2, S.Node, cn), x, cn, D.Set{cr}),
          Equal.sym(Bool, D.rooted(List.length(&2, S.Node, cn), x, cn, D.Set{cr}), S.rooted(List.length(&2, S.Node, cn), x, cn, cr),
            D.rooted_spec(List.length(&2, S.Node, cn), x, cn, cr)),
          D.true_of_equal(D.rooted(List.length(&2, S.Node, cn), x, cn, D.Set{cr}), D.rooted(List.length(&2, Nat, S.ids(cn)), x, cn, D.Set{cr}),
            Equal.sym(Bool, D.rooted(List.length(&2, Nat, S.ids(cn)), x, cn, D.Set{cr}), D.rooted(List.length(&2, S.Node, cn), x, cn, D.Set{cr}),
              Equal.cong(Nat, Bool, n => D.rooted(n, x, cn, D.Set{cr}), List.length(&2, Nat, S.ids(cn)), List.length(&2, S.Node, cn), ids_length(cn))),
            rooted_from_base(List.length(&2, S.Node, bn), List.length(&2, S.Node, cn), x, S.ids(cn), bn, br, cn, cr, frontier,
              present_path, endpoints, frame_path, checked_no, base_rooted(bn, br, x, old_present, base))))))),
    yes => D.true_of_equal(S.rooted(List.length(&2, S.Node, cn), x, cn, cr), D.rooted(List.length(&2, S.Node, cn), x, cn, D.Set{cr}),
      Equal.sym(Bool, D.rooted(List.length(&2, S.Node, cn), x, cn, D.Set{cr}), S.rooted(List.length(&2, S.Node, cn), x, cn, cr),
        D.rooted_spec(List.length(&2, S.Node, cn), x, cn, cr)),
      frontier_rooted(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), x, yes, checked_yes))))

def all_rooted(xs: List<&2, Nat>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>, +fuel: Nat) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case h <> t:
      S.rooted(fuel, h, ns, roots) && all_rooted(t, ns, roots, fuel)

def paths_preserved(+xs: List<&2, Nat>, +bn: List<&2, S.Node>, +br: List<&2, Nat>,
  +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  included: {S.members(xs, S.ids(cn)) == True{} : Bool},
  base: {S.valid(S.Graph{bn, br, 0n}) == True{} : Bool},
  agreement: {frame(S.ids(cn), bn, br, cn, cr, frontier) == True{} : Bool},
  endpoints: {D.closed(S.ids(cn), S.ids(cn), cn, D.Set{cr}) == True{} : Bool},
  checked: {F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)) == True{} : Bool}) ->
  {all_rooted(xs, cn, cr, List.length(&2, S.Node, cn)) == True{} : Bool}:
  match xs:
    case Nil{}:
      {==}
    case +h <> +t:
      D.duplicate_bool(S.valid(S.Graph{bn, br, 0n}), True{}, base, all_rooted(h <> t, cn, cr, List.length(&2, S.Node, cn)), base_head => base_tail =>
      D.duplicate_bool(frame(S.ids(cn), bn, br, cn, cr, frontier), True{}, agreement, all_rooted(h <> t, cn, cr, List.length(&2, S.Node, cn)), frame_head => frame_tail =>
      D.duplicate_bool(D.closed(S.ids(cn), S.ids(cn), cn, D.Set{cr}), True{}, endpoints, all_rooted(h <> t, cn, cr, List.length(&2, S.Node, cn)), closed_head => closed_tail =>
      D.duplicate_bool(F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)), True{}, checked,
        all_rooted(h <> t, cn, cr, List.length(&2, S.Node, cn)), checked_head => checked_tail =>
      D.split_and(S.member(S.ids(cn), h), S.members(t, S.ids(cn)), included,
        {all_rooted(h <> t, cn, cr, List.length(&2, S.Node, cn)) == True{} : Bool}, head => tail =>
        D.and_true(S.rooted(List.length(&2, S.Node, cn), h, cn, cr), all_rooted(t, cn, cr, List.length(&2, S.Node, cn)),
          rooted_at(h, bn, br, cn, cr, frontier, head, base_head, frame_head, closed_head, checked_head),
          paths_preserved(t, bn, br, cn, cr, frontier, tail, base_tail, frame_tail, closed_tail, checked_tail)))))))

# These are native representation facts, separate from both ancestry and
# placement preservation. Reference validity is evaluated by Frontier itself.
def shape_nodes(xs: List<&2, S.Node>, +keys: List<&2, Nat>) -> Bool:
  match xs:
    case Nil{}:
      True{}
    case S.Node{i, p, children, refs} <> t:
      node_shape(i, children, keys) && shape_nodes(t, keys)

def representation(+ns: List<&2, S.Node>, +roots: List<&2, Nat>) -> Bool:
  S.unique(S.ids(ns)) && S.unique(roots) && S.members(roots, S.ids(ns)) && shape_nodes(ns, S.ids(ns))

def assemble_nodes(+xs: List<&2, S.Node>, +ns: List<&2, S.Node>, +roots: List<&2, Nat>, +keys: List<&2, Nat>, +fuel: Nat,
  shapes: {shape_nodes(xs, keys) == True{} : Bool},
  refs: {F.references(xs, keys) == True{} : Bool},
  placed: {all_placements(S.ids(xs), ns, roots) == True{} : Bool},
  paths: {all_rooted(S.ids(xs), ns, roots, fuel) == True{} : Bool}) ->
  {S.nodes_valid(xs, ns, roots, keys, fuel) == True{} : Bool}:
  match xs:
    case Nil{}:
      {==}
    case S.Node{+i, p, +children, +references} <> +t:
      D.split_and(node_shape(i, children, keys), shape_nodes(t, keys), shapes,
        {S.nodes_valid(S.Node{i, p, children, references} <> t, ns, roots, keys, fuel) == True{} : Bool}, shape_head => shape_tail =>
      D.split_and(S.refs_valid(references, keys), F.references(t, keys), refs,
        {S.nodes_valid(S.Node{i, p, children, references} <> t, ns, roots, keys, fuel) == True{} : Bool}, refs_head => refs_tail =>
      D.split_and(Nat.is_eq(placement(ns, roots, i), 1n), all_placements(S.ids(t), ns, roots), placed,
        {S.nodes_valid(S.Node{i, p, children, references} <> t, ns, roots, keys, fuel) == True{} : Bool}, place_head => place_tail =>
      D.split_and(S.rooted(fuel, i, ns, roots), all_rooted(S.ids(t), ns, roots, fuel), paths,
        {S.nodes_valid(S.Node{i, p, children, references} <> t, ns, roots, keys, fuel) == True{} : Bool}, path_head => path_tail =>
        D.and_true(node_local(i, children, references, keys) && Nat.is_eq(placement(ns, roots, i), 1n) && S.rooted(fuel, i, ns, roots),
          S.nodes_valid(t, ns, roots, keys, fuel),
          D.and_true(node_local(i, children, references, keys) && Nat.is_eq(placement(ns, roots, i), 1n), S.rooted(fuel, i, ns, roots),
            D.and_true(node_local(i, children, references, keys), Nat.is_eq(placement(ns, roots, i), 1n),
              D.and_true(node_shape(i, children, keys), S.refs_valid(references, keys), shape_head, refs_head), place_head), path_head),
          assemble_nodes(t, ns, roots, keys, fuel, shape_tail, refs_tail, place_tail, path_tail))))))

def finish_valid(+bn: List<&2, S.Node>, +br: List<&2, Nat>, +cn: List<&2, S.Node>, +cr: List<&2, Nat>, +frontier: List<&2, Nat>,
  prefix: {S.unique(S.ids(cn)) && S.unique(cr) && S.members(cr, S.ids(cn)) == True{} : Bool},
  shapes: {shape_nodes(cn, S.ids(cn)) == True{} : Bool},
  refs: {F.references(cn, S.ids(cn)) == True{} : Bool},
  base: {S.valid(S.Graph{bn, br, 0n}) == True{} : Bool},
  agreement: {frame(S.ids(cn), bn, br, cn, cr, frontier) == True{} : Bool},
  checked: {F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)) == True{} : Bool},
  placed: {all_placements(S.ids(cn), cn, cr) == True{} : Bool}) ->
  {S.valid(S.Graph{cn, cr, 0n}) == True{} : Bool}:
  D.duplicate_bool(all_placements(S.ids(cn), cn, cr), True{}, placed, S.valid(S.Graph{cn, cr, 0n}), placed_nodes => placed_closed =>
    D.and_true(S.unique(S.ids(cn)) && S.unique(cr) && S.members(cr, S.ids(cn)),
      S.nodes_valid(cn, cn, cr, S.ids(cn), List.length(&2, S.Node, cn)), prefix,
      assemble_nodes(cn, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), shapes, refs, placed_nodes,
        paths_preserved(S.ids(cn), bn, br, cn, cr, frontier, members_self(S.ids(cn)), base, agreement,
          placements_closed(S.ids(cn), cn, cr, placed_closed), checked))))

# Native exact-view extraction discharges representation and the complete
# first-order agreement mask. The runtime evaluates only Frontier.preserved:
# all candidate references, and placement/ancestry of the supplied frontier.
law valid_preserved:
  for +bn: List<&2, S.Node>
  for +br: List<&2, Nat>
  for +cn: List<&2, S.Node>
  for +cr: List<&2, Nat>
  for +frontier: List<&2, Nat>
  for base: {S.valid(S.Graph{bn, br, 0n}) == True{} : Bool}
  for native_shape: {representation(cn, cr) == True{} : Bool}
  for agreement: {frame(S.ids(cn), bn, br, cn, cr, frontier) == True{} : Bool}
  for checked: {F.preserved(S.Graph{cn, cr, 0n}, frontier) == True{} : Bool}
  {S.valid(S.Graph{cn, cr, 0n}) == True{} : Bool}
def valid_preserved(bn, br, cn, cr, frontier, base, native_shape, agreement, checked):
  D.split_and(S.unique(S.ids(cn)) && S.unique(cr) && S.members(cr, S.ids(cn)), shape_nodes(cn, S.ids(cn)), native_shape,
    {S.valid(S.Graph{cn, cr, 0n}) == True{} : Bool}, prefix => shapes =>
  D.split_and(F.references(cn, S.ids(cn)), F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)), checked,
    {S.valid(S.Graph{cn, cr, 0n}) == True{} : Bool}, refs => placements =>
  D.duplicate_bool(S.valid(S.Graph{bn, br, 0n}), True{}, base, S.valid(S.Graph{cn, cr, 0n}), base_place => base_path =>
  D.duplicate_bool(frame(S.ids(cn), bn, br, cn, cr, frontier), True{}, agreement, S.valid(S.Graph{cn, cr, 0n}), frame_place => frame_path =>
  D.duplicate_bool(F.placements(frontier, cn, cr, S.ids(cn), List.length(&2, S.Node, cn), P.build(cn)), True{}, placements,
    S.valid(S.Graph{cn, cr, 0n}), checked_place => checked_path =>
    finish_valid(bn, br, cn, cr, frontier, prefix, shapes, refs, base_path, frame_path, checked_path,
      placements_preserved(S.ids(cn), bn, br, cn, cr, frontier, members_self(S.ids(cn)), base_place, frame_place, checked_place)))))))
