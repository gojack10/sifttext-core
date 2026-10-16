If you are running this command yourself, ask your agent to run it and follow these instructions.

# Install SiftText

Set up SiftText for the user's project. Ask which project if it is unclear.

Check that the machine runs macOS 15 or newer, or Linux with glibc 2.35 or newer, on arm64 or x86_64. The installer needs Git, curl, tar and shasum or sha256sum. Reuse the configured Git identity. Ask the user to choose an identity if none is available, and obtain permission for any missing tools or privileged changes.

Download the installer to a temporary file.

```sh
installer=$(mktemp)
curl -fsSL https://github.com/gojack10/sifttext-core/releases/latest/download/install.sh -o "$installer"
```

Read the downloaded file, then run `sh "$installer"`. It verifies and installs the precompiled package for this machine. The default application directory is `~/.local/share/sifttext` and the knowledge home is `~/sifttext`. To choose other locations, pass the application directory and knowledge-home path as the two arguments. Preserve existing knowledge and keep software outside its home.

Use the printed local command to run `runtime-check` and `instructions` for the knowledge home. Read the returned entry and its required files before project work. If installation or verification fails, preserve the files and report the failing command and error.

Managed installations check official releases opportunistically after four hours and apply compatible updates. Network failures leave local work available. When an update reports a conflict, use its retained private worktree to reconcile the knowledge and run `sifttext update --resume` through the installed launcher. Ask the user about any unresolved consequential choice.

For knowledge from hosted SiftText, read the migration guide in the knowledge home. Continue the user's project through the local instructions and verify a useful save by retrieving it. Report the application path, knowledge home and accepted commit.
