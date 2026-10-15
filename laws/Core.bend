import ./Spec.bend as S

# Actual pure runtime entry; its universal conformance law is checked.
def execute(g: S.Graph, r: S.Request) -> S.Outcome:
  S.transition(g, r)
