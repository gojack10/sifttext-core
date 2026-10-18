# Optional Pi session retention

This package includes an opt-in Pi extension in the installed Core release, a directory created by `install-local.sh`, or a local Core checkout. Review the extension source before enabling it: extensions run inside Pi with the same operating-system permissions as Pi.

## Install or remove

From Pi, install the SiftText application directory that contains this package. For a release install that is the `current` directory; for an `install-local.sh` prefix or a local checkout it is that directory itself:

```sh
pi install /absolute/path/to/sifttext-install/current
```

This explicitly adds the package to the selected Pi settings file. To remove it later, use `pi remove` with the same path. Do not run either command if you do not want to change Pi settings.

Set the SiftText home and executable in the environment used to start Pi:

```sh
export SIFTTEXT_HOME="$HOME/sifttext"
export SIFTTEXT_BIN="/absolute/path/to/sifttext"
```

`SIFTTEXT_BIN` may be omitted when `sifttext` is on `PATH`. The extension reads `$SIFTTEXT_HOME/.sifttext/config.json`; it does not change Pi or SiftText settings.

## Enable retention

Retention is off unless the local Core config explicitly enables it. Add this JSON object to the existing config without replacing unrelated settings:

```json
{
  "session_retention": {
    "enabled": true,
    "archive_root": "/absolute/path/to/private/session-archive"
  }
}
```

The archive root must be absolute and should be a private Git repository. When enabled, the extension captures the current Pi session at `agent_end` and checkpoints the archived session file. When disabled or when the setting is absent, the hook does nothing. Capture failures are reported without preventing ordinary knowledge work.

The extension retains and checkpoints the session but never chooses a knowledge node automatically. When a session materially informed a particular node, the agent can call the `sifttext_attach_session` tool with that node's exact UUID, a short kebab-case topic, and an authorized private knowledge worktree. The tool refuses unless retention is enabled, captures/checkpoints the current Pi session, and attaches it only to the named node. For multiple affected nodes, make a separate deliberate call for each; do not attach every chat to every node. Its result contains the node and friendly attachment name, not transcript content or the archive's session ID. Review and publish the changed knowledge through the normal checked worktree workflow.

## Optional off-machine archive backup

A local Git checkpoint is not an off-machine backup. Configure a private Git destination and explicit opt-in in the same local config only when you intend to send the entire retained archive there:

```json
{
  "session_retention": {
    "enabled": true,
    "archive_root": "/absolute/path/to/private/session-archive",
    "backup_enabled": true,
    "backup_remote": "ssh://your-private-git-host/your-account/session-archive.git"
  }
}
```

Configure Git authentication separately; do not put credentials in the URL. The archive contains private transcripts, so use only a destination you control. `sifttext session-backup "$SIFTTEXT_HOME"` pushes the archive's `main` branch. On another machine, set an absolute local `archive_root` and the same authorized destination, then run `sifttext session-restore "$SIFTTEXT_HOME" "$archive_root"`; it clones the archive if absent, otherwise fetches and fast-forwards only, then rebuilds local links. A divergent local archive is refused rather than overwritten. The command's explicit invocation and `backup_enabled: true` are both required before transfer; disabling that setting prevents remote backup/fetch.
