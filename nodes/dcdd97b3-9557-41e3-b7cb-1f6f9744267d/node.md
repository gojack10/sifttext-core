---
id: "dcdd97b3-9557-41e3-b7cb-1f6f9744267d"
name: "Git Commit Skill"
micro_summary: "Tree-native Git commit procedure: inspect the worktree, split unrelated changes, use Scoped Commit messages, commit only intended files, and report the resulting hash."
status: "resolved"
see_also: []
scope: "Turn a working-tree diff into clean, reviewable Scoped Commits: inspect and separate unrelated changes, write repository-appropriate commit messages, commit only the intended files, and report the landed commit."
---
## Scoped Commit convention



Choose the narrowest stable scope that describes the changed behavior, falling back to the package name when necessary.

Omit Conventional Commit type prefixes.

For a change spanning subsystems, use a common scope or comma-separated scopes.

For a codebase-wide change, use `treewide`, `all` or `global` as the scope.

Put an applicable ticket number in the scope or a trailer.

Use the body for context contributors need.

Inspect repository-local commit instructions before proposing messages.

If they conflict with this convention, explain the conflict and ask which format to use before committing.

```text
i2c: virtio: mark device ready before registering the adapter

linuxulator: Return EINVAL for invalid flags

gitlab-ci: update macOS image

net/http/cookiejar: add godoc links

xwayland: 24.1.11 -> 24.1.12

```

## Knowledge-tree commit messages

For a knowledge-tree commit, inspect the before-and-after authored node contents and structure.

Write the subject so a reader can tell what the tree learned, decided or corrected.

Do not make “added node,” “resolved node,” or a version/count the whole explanation.

State the evidence or consequence that makes the change meaningful.

Use the commit body when the title cannot carry that context within its length limit.

Do not invent a finding.

For a status-only, ordering-only or provenance-only capture, describe that change without inventing a knowledge finding.

## SiftText attribution

| Contribution | Author | Co-author trailer |
| --- | --- | --- |
| Human with ordinary agent assistance | Selected human name and email | `Co-authored-by: SiftText Agent <agent@sifttext.com>` |
| Ordinary agent only | `SiftText Agent <agent@sifttext.com>` | Omit duplicate credit |
| Human with Swarm worker assistance | Selected human name and email | `Co-authored-by: SiftText Swarm Agent [actual ID] <agent@sifttext.com>` |
| Swarm worker only | `SiftText Swarm Agent [actual ID] <agent@sifttext.com>` | Omit duplicate credit |

Apply the selected attribution to integration commits.

Preserve attribution already recorded on earlier commits.

Set worker identity per commit to preserve other workers' Git configuration.

Honor the owner's request to change or omit agent credit.

Core carries this attribution through submission.

## Branch names

Prefer a pragmatic scope and short slug, optionally with a ticket:

```text
<scope>/<ticket>-<short-slug>

<scope>/<short-slug>

```

Examples: `auth/123-add-login-form`, `parser/handle-nested-blocks`, `llm/split-prompt-sections`.

## Workflow

Inspect git status and the actual diff before selecting changes.

Separate unrelated changes into independently reviewable commits.

Stage only the intended files and inspect the staged diff.

Run git diff --cached --check before committing.

Use the Scoped Commit convention and the selected attribution policy.

Push only when requested.

Report the resulting commit hash and actual destination.

Leave unrelated pre-existing changes untouched.
