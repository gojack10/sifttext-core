import Base
import ./Spec.bend as S
import ./Core.bend as C

# Contract conformance for every modeled graph/request, not a separate Rust policy.
law conformance:
  for +g: S.Graph
  for +r: S.Request
  {C.execute(g, r) == S.transition(g, r) : S.Outcome}

law rejected_gate:
  for g: S.Graph
  {S.gate(False{}, g) == S.Reject{} : S.Outcome}

law accepted_gate:
  for g: S.Graph
  {S.gate(True{}, g) == S.Accept{g} : S.Outcome}

# Exact bounded witness: all nodes/payloads/references/assets, not just no dangling edges.
law before_valid:
  {S.valid(S.before()) == True{} : Bool}
law move_exact:
  {C.execute(S.before(), S.OpMove{3n, 2n}) == S.Accept{S.after_move()} : S.Outcome}
law reorder_exact:
  {C.execute(S.before(), S.Reorder{3n, 4n}) == S.Accept{S.after_reorder()} : S.Outcome}
law rename_exact:
  {C.execute(S.before(), S.Rename{3n, 18n}) == S.Accept{S.after_rename()} : S.Outcome}
law moved_valid:
  {S.valid(S.after_move()) == True{} : Bool}
law reordered_valid:
  {S.valid(S.after_reorder()) == True{} : Bool}
law renamed_valid:
  {S.valid(S.after_rename()) == True{} : Bool}
law cycle_rejected:
  {C.execute(S.before(), S.OpMove{3n, 5n}) == S.Reject{} : S.Outcome}
law root_rejected:
  {C.execute(S.before(), S.OpMove{1n, 2n}) == S.Reject{} : S.Outcome}
law absent_rejected:
  {C.execute(S.before(), S.OpMove{8n, 2n}) == S.Reject{} : S.Outcome}
law foreign_reorder_rejected:
  {C.execute(S.before(), S.Reorder{3n, 7n}) == S.Reject{} : S.Outcome}
law duplicate_id_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [], []}, S.Node{1n, 2n, [], []}], [1n], 1n}) == False{} : Bool}
law duplicate_parent_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [3n], []}, S.Node{2n, 2n, [3n], []}, S.Node{3n, 3n, [], []}], [1n, 2n], 1n}) == False{} : Bool}
law duplicate_order_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [2n, 2n], []}, S.Node{2n, 2n, [], []}], [1n], 1n}) == False{} : Bool}
law missing_child_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [2n], []}], [1n], 1n}) == False{} : Bool}
law orphan_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [], []}, S.Node{2n, 2n, [], []}], [1n], 1n}) == False{} : Bool}
law dangling_ref_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [], [S.Ref{2n, 1n}]}], [1n], 1n}) == False{} : Bool}
law blank_reason_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [], [S.Ref{1n, 0n}]}], [1n], 1n}) == False{} : Bool}
law graph_cycle_rejected:
  {S.valid(S.Graph{[S.Node{1n, 1n, [2n], []}, S.Node{2n, 2n, [1n], []}], [], 1n}) == False{} : Bool}
