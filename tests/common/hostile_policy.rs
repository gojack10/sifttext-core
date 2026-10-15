//! Standalone test-only verdict transformer, compiled once by the Markdown boundary harness.
use std::{
    env, fs,
    io::{self, Read},
    process,
};

fn transform(input: &str, mode: &str, pos: usize) -> String {
    input
        .lines()
        .map(|line| {
            let mut words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
            if let Some(word) = pos.checked_sub(1).and_then(|i| words.get_mut(i)) {
                match mode {
                    "plus1" => *word = (word.parse::<u64>().unwrap_or(0) + 1).to_string(),
                    "zero" => *word = "0".into(),
                    "huge" => *word = "99999".into(),
                    "drop" => word.clear(),
                    _ => {}
                }
            }
            let mut line = words
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            match mode {
                "extra" => line.push_str(" 0"),
                "empty" | "stderr" => line.clear(),
                _ => {}
            }
            line + "\n"
        })
        .collect()
}

#[allow(dead_code)] // Also imported by the Cargo harness for the pure mutation check.
fn main() {
    let args: Vec<String> = env::args().collect();
    assert_eq!(
        args.len(),
        4,
        "usage: hostile-policy MODE POSITION DUMP-or-empty"
    );
    let (mode, pos) = (args[1].as_str(), args[2].parse::<usize>().unwrap());
    assert!(matches!(
        mode,
        "passthrough" | "plus1" | "zero" | "huge" | "drop" | "extra" | "empty" | "stderr" | "fail"
    ));
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    if !args[3].is_empty() && input.starts_with("STG1 OK ") {
        fs::write(&args[3], &input).unwrap();
    }
    if mode == "fail" {
        process::exit(3);
    }
    if mode == "stderr" {
        eprintln!("boom");
    }
    print!("{}", transform(&input, mode, pos));
}

#[test]
fn prescribed_mutations_have_independent_expected_bytes() {
    let input = "STG1 OK 2 10\n";
    for (mode, pos, expected) in [
        ("passthrough", 0, input),
        ("plus1", 1, "1 OK 2 10\n"),
        ("plus1", 4, "STG1 OK 2 11\n"),
        ("zero", 3, "STG1 OK 0 10\n"),
        ("huge", 3, "STG1 OK 99999 10\n"),
        ("drop", 1, "OK 2 10\n"),
        ("extra", 0, "STG1 OK 2 10 0\n"),
        ("empty", 0, "\n"),
        ("stderr", 0, "\n"),
    ] {
        assert_eq!(transform(input, mode, pos), expected, "{mode}@{pos}");
    }
    assert_eq!(transform("STG1 VALID\n", "passthrough", 0), "STG1 VALID\n");
    assert_eq!(transform(" a\t b\n c \n", "passthrough", 0), "a b\nc\n");
}
