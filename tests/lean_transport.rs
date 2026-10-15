// Exercise the real observer envelope; render-only tests cannot establish it.
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

fn observe(input: &str) -> Output {
    let bin = std::env::var_os("CORE_LEAN_RENDER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_lean-render-observe")));
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("LEAN observer exceeded five seconds");
        }
        thread::sleep(Duration::from_millis(5));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn observer_refuses_malformed_json_and_invalid_keys_without_partial_output() {
    for input in [
        "not json",
        r#"{"bad.key":"unsupported"}"#,
        r#"{"rows":[{"id":"x","bad.key":"unsupported"}]}"#,
        "",
        "{} {}",
    ] {
        let out = observe(input);
        assert!(out.status.code().is_some(), "observer died by signal");
        assert!(!out.status.success(), "invalid request succeeded: {input}");
        assert!(out.stdout.is_empty(), "refusal emitted partial output");
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .strip_prefix("rejected: ")
                .is_some_and(|reason| !reason.trim().is_empty()),
            "refusal lost its reason"
        );
    }
}

#[test]
fn observer_preserves_ambiguous_strings_and_supplied_order_with_final_newline() {
    for (input, expected) in [
        ("{}", "{}\n"),
        (
            r#"{"rows":[{"name":"T"},{"name":"001"},{"name":"_"},{"name":" leading and trailing "}]}"#,
            "rows[4]:name\n  \"T\"\n  \"001\"\n  \"_\"\n  \" leading and trailing \"\n",
        ),
        (
            r#"{"coverage":"full","source_version":"v1","path_template":"nodes/{id}/node.md","token_encoding":"o200k_base","rows":[{"id":"a","status":"in_progress","warnings":" Keep this. ","ruled_out":" Do not trim this. "},{"id":"b","status":"stuck","blocker":"Waiting\nfor input."},{"id":"c","status":"deferred","deferred_reason":"Resume\tafter check."},{"id":"d","status":"discarded","discard_reason":"Rejected: example."},{"id":"e","status":"resolved","name":" leading and trailing ","micro_summary":"_"}]}"#,
            concat!(
                "coverage:full\npath_template:nodes/{id}/node.md\nsource_version:v1\ntoken_encoding:o200k_base\n",
                "rows[5]:id\tstatus\t~\n",
                "  a\tin_progress\truled_out:\" Do not trim this. \"\twarnings:\" Keep this. \"\n",
                "  b\tstuck\tblocker:\"Waiting\\nfor input.\"\n",
                "  c\tdeferred\tdeferred_reason:\"Resume\tafter check.\"\n",
                "  d\tdiscarded\tdiscard_reason:Rejected: example.\n",
                "  e\tresolved\tmicro_summary:\"_\"\tname:\" leading and trailing \"\n",
            ),
        ),
    ] {
        let out = observe(input);
        assert!(out.status.success(), "{:?}", out);
        assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
        assert!(out.stderr.is_empty());
    }
}
