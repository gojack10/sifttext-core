#!/bin/sh
# Reviewed agent-facing bootstrap. Downloads official packages and never compiles.
set -eu
umask 077
PREFIX=${1:-"$HOME/.local/share/sifttext"}
KNOWLEDGE=${2:-"$HOME/sifttext"}
for tool in curl tar git; do
  command -v "$tool" >/dev/null || { echo "Missing $tool. Ask the user to install it within their permissions." >&2; exit 1; }
done
git --version >/dev/null 2>&1 || { echo 'Git is unavailable. Ask the user to install Git before continuing.' >&2; exit 1; }
if command -v shasum >/dev/null; then
  sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
elif command -v sha256sum >/dev/null; then
  sha256() { sha256sum "$1" | cut -d ' ' -f 1; }
else
  echo 'A SHA-256 tool is required. Install shasum or sha256sum with permission.' >&2; exit 1
fi
case "$(uname -s)" in
  Darwin)
    OS=darwin
    MAJOR=$(sw_vers -productVersion | cut -d . -f 1)
    [ "$MAJOR" -ge 15 ] || { echo 'This release requires macOS 15 or newer.' >&2; exit 1; } ;;
  Linux)
    OS=linux
    LIBC=$(getconf GNU_LIBC_VERSION 2>/dev/null) || { echo 'This release requires Linux with glibc 2.35 or newer.' >&2; exit 1; }
    LIBC=${LIBC#glibc }
    MAJOR=${LIBC%%.*}; MINOR=${LIBC#*.}; MINOR=${MINOR%%.*}
    [ "$MAJOR" -gt 2 ] || { [ "$MAJOR" -eq 2 ] && [ "$MINOR" -ge 35 ]; } || { echo 'This release requires glibc 2.35 or newer.' >&2; exit 1; } ;;
  *) echo 'Supported systems are macOS and Linux.' >&2; exit 1 ;;
esac
case "$(uname -m)" in arm64|aarch64) CPU=arm64 ;; x86_64|amd64) CPU=x64 ;; *) echo 'Supported CPUs are arm64 and x86_64.' >&2; exit 1 ;; esac
case "$PREFIX:$KNOWLEDGE" in *'
'*) echo 'Installation paths cannot contain newlines.' >&2; exit 1 ;; esac
[ ! -L "$PREFIX" ] || { echo 'The application directory must not be a symlink.' >&2; exit 1; }
mkdir -p "$(dirname "$PREFIX")"
PARENT=$(CDPATH= cd -P -- "$(dirname "$PREFIX")" && pwd)
PREFIX=$PARENT/$(basename "$PREFIX")
KNOWLEDGE_PARENT=$(CDPATH= cd -P -- "$(dirname "$KNOWLEDGE")" && pwd)
KNOWLEDGE=$KNOWLEDGE_PARENT/$(basename "$KNOWLEDGE")
case "$PREFIX/" in "$KNOWLEDGE/"*) echo 'Keep application software outside the knowledge home.' >&2; exit 1 ;; esac
case "$KNOWLEDGE/" in "$PREFIX/"*) echo 'Keep knowledge outside the application directory.' >&2; exit 1 ;; esac
# A user-owned command, never an overwrite of another program or a system directory.
BIN=$HOME/.local/bin
case "$BIN" in *'
'*) echo 'Installation paths cannot contain newlines.' >&2; exit 1 ;; esac
QUOTED_PREFIX=$(printf '%s' "$PREFIX" | sed "s/'/'\\\\''/g")
COMMAND=$(printf "#!/bin/sh\n# SiftText managed command\nexec '%s/sifttext' \"\$@\"\n" "$QUOTED_PREFIX")
check_command() {
  if [ -e "$BIN/sifttext" ] || [ -L "$BIN/sifttext" ]; then
    [ ! -L "$BIN/sifttext" ] && [ -f "$BIN/sifttext" ] && [ "$(cat "$BIN/sifttext")" = "$COMMAND" ] || {
      echo 'Refusing to replace an unrelated ~/.local/bin/sifttext command.' >&2; exit 1;
    }
  fi
}
check_command
FOUND=$(command -v sifttext || :)
case "$FOUND" in ''|"$BIN/sifttext"|"$PREFIX/sifttext") ;; *)
  echo 'An unrelated sifttext command already exists on PATH. Nothing was installed.' >&2; exit 1 ;;
esac
expose_command() {
  check_command
  mkdir -p "$BIN"
  if [ ! -e "$BIN/sifttext" ] && [ ! -L "$BIN/sifttext" ]; then
    (set -C; printf '%s\n' "$COMMAND" > "$BIN/sifttext")
  fi
  chmod +x "$BIN/sifttext"
  case ":$PATH:" in *":$BIN:"*) return ;; esac
  # A piped installer cannot change its parent shell. Persist PATH for the next shell.
  case "${SHELL:-/bin/sh}" in
    */zsh) PROFILES="${ZDOTDIR:-$HOME}/.zshrc" ;;
    */bash)
      if [ -e "$HOME/.bash_profile" ]; then LOGIN=$HOME/.bash_profile
      elif [ -e "$HOME/.bash_login" ]; then LOGIN=$HOME/.bash_login
      else LOGIN=$HOME/.profile; fi
      PROFILES=$LOGIN ;;
    */sh|*/dash|*/ksh) PROFILES=$HOME/.profile ;;
    *) echo 'Cannot configure PATH for this shell. No success banner was printed.' >&2; return 1 ;;
  esac
  PATH_LINE='export PATH="$HOME/.local/bin:$PATH"'
  grep -Fqx "$PATH_LINE" "$PROFILES" 2>/dev/null || printf '\n%s\n' "$PATH_LINE" >> "$PROFILES"
  case "${SHELL:-/bin/sh}" in */bash)
    grep -Fqx "$PATH_LINE" "$HOME/.bashrc" 2>/dev/null || printf '\n%s\n' "$PATH_LINE" >> "$HOME/.bashrc" ;;
  esac
}
success() {
  cat <<'LOGO'
░▒▓█████████████████

    ░▒▓█████████████

        ░▒▓█████████

            ░▒▓█████

                ░▒▓█
LOGO
  CYAN= RESET=
  if [ -t 1 ] && [ "${TERM:-dumb}" != dumb ] && [ "${NO_COLOR+x}" != x ]; then
    CYAN=$(printf '\033[1;36m'); RESET=$(printf '\033[0m')
  fi
  printf 'Login with %ssifttext login%s.\nTell your agent to run %ssifttext instructions%s.\n' "$CYAN" "$RESET" "$CYAN" "$RESET"
}
if [ -e "$PREFIX" ]; then
  [ -f "$PREFIX/state.json" ] && [ -x "$PREFIX/sifttext" ] || { echo 'Application path exists. Choose a new empty application path.' >&2; exit 1; }
  "$PREFIX/current/sifttext" install-home "$PREFIX" "$KNOWLEDGE" >/dev/null
  expose_command
  success
  exit 0
fi
STAGE=$(mktemp -d "$PARENT/.sifttext-install.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
BASE=https://github.com/gojack10/sifttext-core/releases
RELEASE=${SIFTTEXT_RELEASE:-latest}
case "$RELEASE" in ''|*[!a-zA-Z0-9._-]*|.|..) echo 'Invalid requested release.' >&2; exit 1 ;; esac
if [ "$RELEASE" = latest ]; then CHECKSUMS=$BASE/latest/download/SHA256SUMS; else CHECKSUMS=$BASE/download/$RELEASE/SHA256SUMS; fi
curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --connect-timeout 5 --max-time 60 \
  "$CHECKSUMS" -o "$STAGE/SHA256SUMS"
VERSION=$(awk 'NR==1 && $1=="#" && $2=="SiftText" && $3=="release" {print $4}' "$STAGE/SHA256SUMS")
SOURCE=$(awk 'NR==1 && $1=="#" && $2=="SiftText" && $3=="release" && $5=="source" {print $6}' "$STAGE/SHA256SUMS")
case "$SOURCE" in ''|*[!a-f0-9]*) echo 'Invalid release source revision.' >&2; exit 1 ;; esac
[ "${#SOURCE}" -eq 40 ] || { echo 'Invalid release source revision.' >&2; exit 1; }
case "$VERSION" in ''|*[!a-zA-Z0-9._-]*|.|..) echo 'Invalid release version.' >&2; exit 1 ;; esac
[ "$RELEASE" = latest ] || [ "$VERSION" = "$RELEASE" ] || { echo 'Checksum release differs from the requested version.' >&2; exit 1; }
ASSET=sifttext-$OS-$CPU.tar.gz
DIGEST=$(awk -v name="$ASSET" '$2==name {n++; value=$1} END {if(n==1)print value;else exit 1}' "$STAGE/SHA256SUMS")
[ "${#DIGEST}" -eq 64 ] || { echo 'Missing release checksum.' >&2; exit 1; }
case "$DIGEST" in *[!a-f0-9]*) echo 'Invalid release checksum.' >&2; exit 1 ;; esac
curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --connect-timeout 5 --max-time 60 --max-filesize 268435456 \
  "$BASE/download/$VERSION/$ASSET" -o "$STAGE/package.tar.gz"
[ "$(sha256 "$STAGE/package.tar.gz")" = "$DIGEST" ] || { echo 'Release checksum mismatch. Nothing was installed.' >&2; exit 1; }
# What this proves: the archive is the exact bytes the publisher's official GitHub release
# serves over HTTPS, bound to the digest published beside it. What it does not prove: which
# build produced them. That would need a signed-provenance verifier with its own established
# trust root, which this product neither requires of a customer nor ships.

tar -tzf "$STAGE/package.tar.gz" > "$STAGE/members"
while IFS= read -r member; do
  case "$member" in package/*) ;; *) echo 'Unsafe archive entry.' >&2; exit 1 ;; esac
  case "/$member/" in */../*|*/./*) echo 'Unsafe archive traversal.' >&2; exit 1 ;; esac
done < "$STAGE/members"
mkdir -p "$STAGE/application/releases/$VERSION"
tar -xzf "$STAGE/package.tar.gz" --strip-components=1 -C "$STAGE/application/releases/$VERSION"
ln -s "releases/$VERSION" "$STAGE/application/current"
printf '%s\n' "$DIGEST" > "$STAGE/application/package.sha256"
cat > "$STAGE/application/sifttext" <<'LAUNCHER'
#!/bin/sh
set -eu
SIFTTEXT_INSTALL_ROOT=$(CDPATH= cd -P -- "$(dirname -- "$0")" && pwd)
export SIFTTEXT_INSTALL_ROOT
exec "$SIFTTEXT_INSTALL_ROOT/current/sifttext" "$@"
LAUNCHER
chmod +x "$STAGE/application/sifttext"
"$STAGE/application/current/sifttext" runtime-check >/dev/null
"$STAGE/application/current/sifttext" install-home "$STAGE/application" "$KNOWLEDGE" >/dev/null
# This invocation owns only the new application directory. A failed home setup
# leaves any created knowledge available for inspection and never overwrites it.
[ ! -e "$PREFIX" ] && [ ! -L "$PREFIX" ] || { echo 'Application path appeared during installation. Preserving it.' >&2; exit 1; }
mv "$STAGE/application" "$PREFIX"
expose_command
success
