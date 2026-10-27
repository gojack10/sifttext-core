# Core update repairs

Updates now retain the configured Git name and email without enabling unrelated global Git behavior. Missing identity is reported before package preparation. Official knowledge imports use a private Git index rather than a registered preparation worktree, and downloads are staged before entering the reusable application cache. Interrupted preparation can restart without deleting older attempts; an untouched journaled draft can resume its merge.

Filesystem validation, bundled-home capture and neighborhood inspection now share the existing generated-file ownership cache and cleanup implementation. They no longer reconstruct deleted nodes using a directory embedded by the release builder. Unknown or modified files remain protected. Missing ownership records still require assisted recovery.

Published knowledge must be resolved. An installed contributor hook checks staged KB changes, GitHub CI checks KB changes, and packaging checks every node in the release snapshot. Customer-edited node statuses are not rewritten by these checks.

For an interrupted older update, use the attached `update-recovery.md` with your agent. In particular, v0.3.5 can fail before activating the downloaded executable; running its old updater again does not apply the repair. The prompt explains how to run the repaired executable against the retained installation journal without replacing customer knowledge or manually switching the launcher.

Build/start evidence is not a customer-migration trial. Old caches without exact generated-byte ownership, conflicts, and incomplete journals may still need maintainer assistance.
