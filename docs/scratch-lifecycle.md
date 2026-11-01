# Temporary operation storage

Core places newly allocated private drafts, internal publication views and large
binary intermediates in a private per-user directory beneath the platform temp
path (`TMPDIR` when set). Accepted Git history, draft branches and submitted-save
journals remain in the knowledge repository, outside disposable storage.

Keep task artifacts in the durable `artifact_home` returned by `sifttext worktree`,
not in its temporary `worktree` path. An expired draft's uncommitted files are not
recoverable from Git; retaining a chat transcript does not back them up.

## Normal completion

Successful submission removes its draft through the existing native completion
procedure. Internal binary operations remove their directories when they finish.
Live operation leases are inherited by controlled Git helpers so a caller's exit
does not authorize removing files still in use by its helper.

## Abandoned operations

Before ordinary knowledge and sync commands, Core attempts startup cleanup at
most once per hour. Help, account operations and application-update commands do
not run this cleanup; `update --local-status` remains read-only.

Startup cleanup considers Core's temporary namespace. Publication also expires
recorded legacy views beneath that repository's `.git/sifttext-publications`,
at most once per hour per repository, using the same expiry procedure. No
whole-home discovery scan or migration is needed.

A candidate must have no file or directory modifications within the preceding
48 hours. The scan does not follow symlinks. A future timestamp postpones removal.

- Binary operation directories require an available operation lease.
- Private drafts require the expected Core-created branch name, available
  lifecycle and draft locks, and inactive Git administrative files. An existing
  submitted-save journal, worktree lock other than an interrupted initialization,
  or Git index lock prevents expiry.
- Internal publication views require their exact recorded ownership and Git
  registration, an available lifecycle gate, and inactive Git administrative
  files. An index lock or an unexpected worktree lock prevents expiry.

Expiry can discard uncommitted draft files. It removes Git worktrees through Git
and does not delete draft branches, accepted commits or submitted-save journals.
Active, submitted or unrecognized work is retained. Cleanup errors are reported
without preventing the requested command.

Keep worktrees short-lived. Read SiftText, do the work, save through SiftText
Commit, and start the next chat from SiftText. Reads alone do not renew the
inactivity period.

Earlier private drafts beside the knowledge home are not swept by age. Legacy
publication views with matching recorded ownership are transient working copies,
including those marked `retained`; that label is not an indefinite retention
exemption. Their expiry uses the same lifecycle and Git-registration checks as
new views, without requiring a clean checkout or an ancestry audit. Existing
native recovery still handles recorded disposable views.
