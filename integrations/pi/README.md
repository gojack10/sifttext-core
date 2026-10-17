# Optional Pi session retention

This package loads an opt-in Pi extension from the SiftText Core checkout. Review the extension source before enabling it: extensions run inside Pi with the same operating-system permissions as Pi.

## Install or remove

From Pi, install the local Core checkout:

```sh
pi install /absolute/path/to/sifttext-core
```

This explicitly adds the package to the selected Pi settings file. To remove it later, use `pi remove /absolute/path/to/sifttext-core`. Do not run either command if you do not want to change Pi settings.

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

The extension currently retains and checkpoints the session; it does not choose a knowledge node or attach the session to one automatically. Node association and real Pi lifecycle qualification remain separate work. A local Git checkpoint is not an off-machine backup.
