import Base
import ../Spec.bend as S
import ../Core.bend as C

# Independent observers, not a second implementation of the transition.
# Structural operations may change children, but no identity/payload/reference.
def content_nodes(ns: List<&2, S.Node>) -> List<&2, S.Node>:
  match ns:
    case Nil{}:
      Nil{}
    case S.Node{i, p, c, r} <> t:
      S.Node{i, p, [], r} <> content_nodes(t)

# Rename may change opaque payload, but no identity/children/reference.
def hierarchy_nodes(ns: List<&2, S.Node>) -> List<&2, S.Node>:
  match ns:
    case Nil{}:
      Nil{}
    case S.Node{i, p, c, r} <> t:
      S.Node{i, 0n, c, r} <> hierarchy_nodes(t)

# Mask exactly one selected parent's sequence or one target payload.
def masked_children(ns: List<&2, S.Node>, +par: Nat) -> List<&2, S.Node>:
  match ns:
    case Nil{}:
      Nil{}
    case S.Node{+i, p, c, r} <> t:
      S.Node{i, p, Bool.pick(List<&2, Nat>, Nat.is_eq(i, par), [], c), r} <> masked_children(t, par)

def masked_payload(ns: List<&2, S.Node>, +x: Nat) -> List<&2, S.Node>:
  match ns:
    case Nil{}:
      Nil{}
    case S.Node{+i, p, c, r} <> t:
      S.Node{i, Bool.pick(Nat, Nat.is_eq(i, x), 0n, p), c, r} <> masked_payload(t, x)

def frame(g: S.Graph, req: S.Request) -> S.Graph:
  match g req:
    case S.Graph{ns, roots, assets} S.OpMove{x, dst}:
      S.Graph{content_nodes(ns), roots, assets}
    case S.Graph{ns, roots, assets} S.Reorder{x, after}:
      S.Graph{content_nodes(ns), roots, assets}
    case S.Graph{ns, roots, assets} S.Rename{x, payload}:
      S.Graph{masked_payload(ns, x), roots, assets}

# Rejection makes no assertion of success; frozen positive witnesses still apply.
def Preserved(+before: S.Graph, +req: S.Request, out: S.Outcome) -> Type:
  match out:
    case S.Reject{}:
      Unit
    case S.Accept{after}:
      {frame(after, req) == frame(before, req) : S.Graph}

law move_content_frame:
  for +ns: List<&2, S.Node>
  for +x: Nat
  for +dst: Nat
  {content_nodes(S.moved(ns, x, dst)) == content_nodes(ns) : List<&2, S.Node>}

law reorder_content_frame:
  for +ns: List<&2, S.Node>
  for +x: Nat
  for +after: Nat
  for +par: Nat
  {content_nodes(S.reordered(ns, x, after, par)) == content_nodes(ns) : List<&2, S.Node>}

law rename_hierarchy_frame:
  for +ns: List<&2, S.Node>
  for +x: Nat
  for +payload: Nat
  {hierarchy_nodes(S.renamed(ns, x, payload)) == hierarchy_nodes(ns) : List<&2, S.Node>}

law gate_frame:
  for +before: S.Graph
  for +req: S.Request
  for +after: S.Graph
  for ok: Bool
  for e: {frame(after, req) == frame(before, req) : S.Graph}
  Preserved(before, req, S.gate(ok, after))

law mask_children_pick:
  for +b: Bool
  for +old: List<&2, Nat>
  for changed: List<&2, Nat>
  {Bool.pick(List<&2, Nat>, b, [], old) == Bool.pick(List<&2, Nat>, b, [], Bool.pick(List<&2, Nat>, b, changed, old)) : List<&2, Nat>}

law mask_payload_pick:
  for +b: Bool
  for +old: Nat
  for changed: Nat
  {Bool.pick(Nat, b, 0n, old) == Bool.pick(Nat, b, 0n, Bool.pick(Nat, b, changed, old)) : Nat}

law reorder_one_sequence:
  for +ns: List<&2, S.Node>
  for +x: Nat
  for +after: Nat
  for +par: Nat
  {masked_children(S.reordered(ns, x, after, par), par) == masked_children(ns, par) : List<&2, S.Node>}

law rename_one_payload:
  for +ns: List<&2, S.Node>
  for +x: Nat
  for +payload: Nat
  {masked_payload(S.renamed(ns, x, payload), x) == masked_payload(ns, x) : List<&2, S.Node>}

def mask_children_pick(b, old, changed):
  match b:
    case False{}:
      {==}
    case True{}:
      {==}

def mask_payload_pick(b, old, changed):
  match b:
    case False{}:
      {==}
    case True{}:
      {==}

def reorder_one_sequence(ns, x, after, par):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, +p, +c, +r} <> +t:
      %mask_children_pick(Nat.is_eq(i, par), c, S.insert(S.remove(c, x), x, after)) : {S.Node{i, p, _, r} <> masked_children(S.reordered(t, x, after, par), par) == S.Node{i, p, Bool.pick(List<&2, Nat>, Nat.is_eq(i, par), [], c), r} <> masked_children(t, par) : List<&2, S.Node>}
      %reorder_one_sequence(t, x, after, par) : {S.Node{i, p, Bool.pick(List<&2, Nat>, Nat.is_eq(i, par), [], c), r} <> masked_children(S.reordered(t, x, after, par), par) == S.Node{i, p, Bool.pick(List<&2, Nat>, Nat.is_eq(i, par), [], c), r} <> _ : List<&2, S.Node>}
      {==}

def rename_one_payload(ns, x, payload):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, +p, +c, +r} <> +t:
      %mask_payload_pick(Nat.is_eq(i, x), p, payload) : {S.Node{i, _, c, r} <> masked_payload(S.renamed(t, x, payload), x) == S.Node{i, Bool.pick(Nat, Nat.is_eq(i, x), 0n, p), c, r} <> masked_payload(t, x) : List<&2, S.Node>}
      %rename_one_payload(t, x, payload) : {S.Node{i, Bool.pick(Nat, Nat.is_eq(i, x), 0n, p), c, r} <> masked_payload(S.renamed(t, x, payload), x) == S.Node{i, Bool.pick(Nat, Nat.is_eq(i, x), 0n, p), c, r} <> _ : List<&2, S.Node>}
      {==}

# Every actual accepted transition preserves its independent frame, for arbitrary
# modeled graphs/requests (including invalid inputs, which may reject).
law transition_frame:
  for +g: S.Graph
  for +req: S.Request
  Preserved(g, req, C.execute(g, req))

def move_content_frame(ns, x, dst):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, +p, c, +r} <> +t:
      %move_content_frame(t, x, dst) : {S.Node{i, p, [], r} <> content_nodes(S.moved(t, x, dst)) == S.Node{i, p, [], r} <> _ : List<&2, S.Node>}
      {==}

def reorder_content_frame(ns, x, after, par):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, +p, c, +r} <> +t:
      %reorder_content_frame(t, x, after, par) : {S.Node{i, p, [], r} <> content_nodes(S.reordered(t, x, after, par)) == S.Node{i, p, [], r} <> _ : List<&2, S.Node>}
      {==}

def rename_hierarchy_frame(ns, x, payload):
  match ns:
    case Nil{}:
      {==}
    case S.Node{+i, p, +c, +r} <> +t:
      %rename_hierarchy_frame(t, x, payload) : {S.Node{i, 0n, c, r} <> hierarchy_nodes(S.renamed(t, x, payload)) == S.Node{i, 0n, c, r} <> _ : List<&2, S.Node>}
      {==}

def gate_frame(before, req, after, ok, e):
  match ok:
    case False{}:
      Unit{}
    case True{}:
      e

def move_graph_frame(+ns: List<&2, S.Node>, +roots: List<&2, Nat>, +assets: Nat, +x: Nat, +dst: Nat) -> {frame(S.Graph{S.moved(ns, x, dst), roots, assets}, S.OpMove{x, dst}) == frame(S.Graph{ns, roots, assets}, S.OpMove{x, dst}) : S.Graph}:
  Equal.cong(List<&2, S.Node>, S.Graph, zs => S.Graph{zs, roots, assets}, content_nodes(S.moved(ns, x, dst)), content_nodes(ns), move_content_frame(ns, x, dst))

def reorder_graph_frame(+ns: List<&2, S.Node>, +roots: List<&2, Nat>, +assets: Nat, +x: Nat, +after: Nat, +par: Nat) -> {frame(S.Graph{S.reordered(ns, x, after, par), roots, assets}, S.Reorder{x, after}) == frame(S.Graph{ns, roots, assets}, S.Reorder{x, after}) : S.Graph}:
  Equal.cong(List<&2, S.Node>, S.Graph, zs => S.Graph{zs, roots, assets}, content_nodes(S.reordered(ns, x, after, par)), content_nodes(ns), reorder_content_frame(ns, x, after, par))

def rename_graph_frame(+ns: List<&2, S.Node>, +roots: List<&2, Nat>, +assets: Nat, +x: Nat, +payload: Nat) -> {frame(S.Graph{S.renamed(ns, x, payload), roots, assets}, S.Rename{x, payload}) == frame(S.Graph{ns, roots, assets}, S.Rename{x, payload}) : S.Graph}:
  Equal.cong(List<&2, S.Node>, S.Graph, zs => S.Graph{zs, roots, assets}, masked_payload(S.renamed(ns, x, payload), x), masked_payload(ns, x), rename_one_payload(ns, x, payload))

def transition_frame(g, req):
  match g req:
    case S.Graph{+ns, +roots, +assets} S.OpMove{+x, +dst}:
      +next = {S.Graph{S.moved(ns, x, dst), roots, assets} : S.Graph}
      ok = S.valid(S.Graph{ns, roots, assets}) && S.member(S.ids(ns), x) && S.member(S.ids(ns), dst) && Bool.not(S.member(roots, x))
        && Bool.not(S.ancestor(List.length(&2, S.Node, ns), dst, x, ns))
      gate_frame(S.Graph{ns, roots, assets}, S.OpMove{x, dst}, next, ok && S.valid(next), move_graph_frame(ns, roots, assets, x, dst))
    case S.Graph{+ns, +roots, +assets} S.Reorder{+x, +after}:
      +par = S.parent(ns, x)
      +next = {S.Graph{S.reordered(ns, x, after, par), roots, assets} : S.Graph}
      ok = S.valid(S.Graph{ns, roots, assets}) && Nat.is_ne(par, 0n) && Nat.is_ne(x, after) && Nat.is_eq(par, S.parent(ns, after))
      gate_frame(S.Graph{ns, roots, assets}, S.Reorder{x, after}, next, ok && S.valid(next), reorder_graph_frame(ns, roots, assets, x, after, par))
    case S.Graph{+ns, +roots, +assets} S.Rename{+x, +payload}:
      +next = {S.Graph{S.renamed(ns, x, payload), roots, assets} : S.Graph}
      ok = S.valid(S.Graph{ns, roots, assets}) && S.member(S.ids(ns), x) && Nat.is_ne(payload, 0n)
      gate_frame(S.Graph{ns, roots, assets}, S.Rename{x, payload}, next, ok && S.valid(next), rename_graph_frame(ns, roots, assets, x, payload))

# Observe reorder using the original parent's identity, not a parent inferred
# again from the result. This strengthens the actual transition's locality.
def reorder_frame(g: S.Graph, par: Nat) -> S.Graph:
  match g:
    case S.Graph{ns, roots, assets}:
      S.Graph{masked_children(ns, par), roots, assets}

def ReorderPreserved(+before: S.Graph, +par: Nat, out: S.Outcome) -> Type:
  match out:
    case S.Reject{}:
      Unit
    case S.Accept{after}:
      {reorder_frame(after, par) == reorder_frame(before, par) : S.Graph}

law reorder_gate_frame:
  for +before: S.Graph
  for +par: Nat
  for +after: S.Graph
  for ok: Bool
  for e: {reorder_frame(after, par) == reorder_frame(before, par) : S.Graph}
  ReorderPreserved(before, par, S.gate(ok, after))

def reorder_gate_frame(before, par, after, ok, e):
  match ok:
    case False{}:
      Unit{}
    case True{}:
      e

def original_parent(g: S.Graph, x: Nat) -> Nat:
  match g:
    case S.Graph{ns, roots, assets}:
      S.parent(ns, x)

law transition_reorder_locality:
  for +g: S.Graph
  for +x: Nat
  for +after: Nat
  ReorderPreserved(g, original_parent(g, x), C.execute(g, S.Reorder{x, after}))

def transition_reorder_locality(g, x, after):
  match g:
    case S.Graph{+ns, +roots, +assets}:
      +par = S.parent(ns, x)
      +next = {S.Graph{S.reordered(ns, x, after, par), roots, assets} : S.Graph}
      ok = S.valid(S.Graph{ns, roots, assets}) && Nat.is_ne(par, 0n) && Nat.is_ne(x, after) && Nat.is_eq(par, S.parent(ns, after))
      e = Equal.cong(List<&2, S.Node>, S.Graph, zs => S.Graph{zs, roots, assets}, masked_children(S.reordered(ns, x, after, par), par), masked_children(ns, par), reorder_one_sequence(ns, x, after, par))
      reorder_gate_frame(S.Graph{ns, roots, assets}, par, next, ok && S.valid(next), e)
