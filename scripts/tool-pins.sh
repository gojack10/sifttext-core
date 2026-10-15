# Source-only platform and verified release pins. No executable is run here.
tool_sha256() { shasum -a 256 "$1" | cut -d ' ' -f 1; }
platform_pins() {
  case "$(uname -s)" in
    Darwin) OS=darwin; RUST_OS=apple-darwin ;;
    Linux) OS=linux; RUST_OS=unknown-linux-gnu ;;
    *) echo 'Unsupported OS; only macOS and Linux are supported' >&2; return 1 ;;
  esac
  case "$(uname -m)" in
    arm64|aarch64) CPU=arm64; RUST_CPU=aarch64 ;;
    x86_64|amd64) CPU=x64; RUST_CPU=x86_64 ;;
    *) echo 'Unsupported CPU; only arm64 and x86_64 are supported' >&2; return 1 ;;
  esac
  PLATFORM=$OS-$CPU
  RUST_TARGET=$RUST_CPU-$RUST_OS
  case "$PLATFORM" in
    darwin-arm64)
      ARCHIVE_SHA256=de1f0a8b8db18c336edfb9385234b34f7da60fa4c79a4261e3be4a9674a2ffd7
      BEND_SHA256=aaa9e73e31419d939825ceb24e746d96426b68b5901db799b4afd78619e319f8 ;;
    darwin-x64)
      ARCHIVE_SHA256=fcde1f2a17939b1cbef1369a32c13ffdb7cef53a8355e9cf3887b119c79fa17a
      BEND_SHA256=24dcddacecdd2aae7e9223f424d3af2f7e9a735edc58cee159ef8b59b8f5d523 ;;
    linux-arm64)
      ARCHIVE_SHA256=2535c11bf554639cd90e634248915c597a06f40753ce28dea5fc71f7a6cf9703
      BEND_SHA256=9a0d4fd95cc7c6845fc30601e610dfb499a816501c33310cb5ef44ed17ebc79c ;;
    linux-x64)
      ARCHIVE_SHA256=58adc86af6605ed0c48f7d84e4c23028f78893ce4a867a20a4f004b11582687b
      BEND_SHA256=38330ad07e228ba7a317836f484648cba93a1a3927357edccc65e3f2a0de253a ;;
  esac
  [ "$BEND_SHA256" != UNQUALIFIED ] || { echo 'Platform binary digest acquisition unfinished' >&2; return 1; }
}
