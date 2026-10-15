---
id: "dcdd97b3-9557-41e3-b7cb-1f6f9744267d"
name: "Git Commit Skill"
micro_summary: "Tree-native Git commit procedure: inspect the worktree, split unrelated changes, use Scoped Commit messages, commit only intended files, and report the resulting hash."
status: "resolved"
see_also: []
scope: "Turn a working-tree diff into clean, reviewable Scoped Commits: inspect and separate unrelated changes, write repository-appropriate commit messages, commit only the intended files, and report the landed commit."
warnings: ""
---
## Scoped Commit convention

Use this format, with a title no longer than 100 characters:

```text
<scope>: <description>

[optional body]

[optional trailers]
```

Choose the narrowest stable subsystem or behavior that covers the commit. Package and repository names are fallback scopes, not defaults. Avoid umbrella scopes such as `ai`, `agent`, `core`, or `app` when a functional scope such as `openai-responses`, `context-estimation`, `context-overflow`, or `model-routing` describes the change accurately. Derive scope from the behavior changed, not merely the containing directory.

Do not use Conventional Commit type prefixes such as `feat`, `fix`, or `chore`. For SiftText knowledge commits, apply the selected attribution below. For multiple scopes, prefer a useful general scope, comma-separated scopes, or `treewide`, `all`, or `global`. Put ticket numbers in the scope or a trailer when applicable. Use the body only for context contributors need to understand the change.

Inspect repository-local commit instructions before proposing messages. If they conflict with this convention, surface the conflict and request an explicit choice; never silently substitute the repository format.

Examples:

```text
i2c: virtio: mark device ready before registering the adapter
linuxulator: Return EINVAL for invalid inotify flags
gitlab-ci: update macOS image
net/http/cookiejar: add godoc links
xwayland: 24.1.11 -> 24.1.12
```

## SiftText knowledge attribution

Use the workspace owner's selected name/email, following CLI Operations' selection and confirmation procedure when missing. Apply identity repository-locally or per command; preserve global configuration and other workers' settings.

- Human author with ordinary agent contribution: selected human name/email as author; `Co-authored-by: SiftText Agent <agent@sifttext.com>`.
- Agent-only contribution: `SiftText Agent <agent@sifttext.com>` as author; no duplicate co-author.
- Human author with a SiftText Swarm worker contribution: selected human as author; `Co-authored-by: SiftText Swarm Agent [actual ID] <agent@sifttext.com>`.
- Agent-only SiftText Swarm worker: `SiftText Swarm Agent [actual ID] <agent@sifttext.com>` as author; no duplicate co-author.

Use an actual worker ID only when supplied by an authorized Swarm integration. Ordinary local Core does not require a worker identity. Set worker-specific attribution per commit, preserving other workers' Git configuration. Honor the owner's requests to change or omit agent credit. Preserve earlier commit attribution and apply the selected policy to any necessary integration commit; verify the accepted commit author and trailers after submission.

## Branch names

Prefer a pragmatic scope and short slug, optionally with a ticket:

```text
<scope>/<ticket>-<short-slug>
<scope>/<short-slug>
```

Examples: `auth/123-add-login-form`, `parser/handle-nested-blocks`, `llm/split-prompt-sections`.

## Workflow

1. Inspect `git status`, `git diff --stat`, and the relevant file diffs.
2. Separate unrelated changes into logical commits. Never sweep unrelated pre-existing work into the requested commit.
3. Stage only the intended files and verify the staged diff, including `git diff --cached --check`.
4. Commit using the Scoped Commit convention.
5. Push only when requested, then report the hash, destination, summary, tests, and any intentionally untouched worktree changes.

For a large diff, segment it into independently reviewable commits rather than hiding several concerns under one message. Extra human guidance supplied with the skill overrides default scoping choices but not repository safety.