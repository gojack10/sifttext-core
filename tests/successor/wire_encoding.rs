use super::*;

// Retained pre-change linear encoder, independent of the new lookup tables.
#[derive(Default)]
struct LinearTokens(Vec<Vec<u8>>);
impl LinearTokens {
    fn intern(&mut self, bytes: &[u8]) -> usize {
        if let Some(i) = self.0.iter().position(|b| b == bytes) {
            return i + 1;
        }
        self.0.push(bytes.to_vec());
        self.0.len()
    }
}
fn baseline(s: &Store, ids: &[String], tokens: &mut LinearTokens) -> Graph {
    let id = |s: &str| ids.iter().position(|x| x == s).map_or(0, |i| i + 1);
    Graph {
        assets: tokens.intern(&asset_bytes(s)),
        roots: s.roots.iter().map(|s| id(s)).collect(),
        nodes: s
            .nodes
            .values()
            .map(|n| WireNode {
                id: id(&n.id),
                payload: tokens.intern(
                    match &s.inventory[&PathBuf::from(format!("nodes/{}/node.md", n.id))] {
                        Entry::File(b) => b,
                        _ => unreachable!(),
                    },
                ),
                children: s.children[&n.id].iter().map(|s| id(s)).collect(),
                refs: n
                    .see_also
                    .iter()
                    .map(|r| (id(&r.target), tokens.intern(r.reason.as_bytes())))
                    .collect(),
            })
            .collect(),
    }
}
fn id(n: usize) -> String {
    format!("60000000-0000-4000-8000-{n:012x}")
}
fn fixture(count: usize, deep: bool) -> Store {
    let mut s = Store {
        path: PathBuf::new(),
        nodes: BTreeMap::new(),
        children: BTreeMap::new(),
        roots: if count == 0 { vec![] } else { vec![id(1)] },
        inventory: BTreeMap::new(),
    };
    for n in 1..=count {
        let refs = if count > 1 {
            vec![
                Reference {
                    target: id(n % count + 1),
                    reason: "shared reason\0雪".into(),
                },
                Reference {
                    target: id(n % count + 1),
                    reason: format!("distinct reason {n}"),
                },
            ]
        } else {
            vec![]
        };
        s.nodes.insert(
            id(n),
            Authored {
                id: id(n),
                name: format!("Node {n}"),
                micro_summary: "Synthetic summary".into(),
                content: format!("Synthetic body {n}\r\n雪"),
                see_also: refs,
            },
        );
        s.children.insert(
            id(n),
            if deep && n < count {
                vec![id(n + 1)]
            } else if !deep && n == 1 {
                (2..=count).rev().map(id).collect()
            } else {
                vec![]
            },
        );
        s.inventory.insert(
            PathBuf::from(format!("nodes/{}/node.md", id(n))),
            Entry::File(format!("complete source {n}\r\n雪\0").into_bytes()),
        );
    }
    s.inventory.insert(
        PathBuf::from("attachment.bin"),
        Entry::File(vec![0, 255, 1]),
    );
    s
}
fn differential(s: &Store, ids: &[String]) {
    let mut old = LinearTokens::default();
    let mut new = Tokens::default();
    for seed in [
        b"".as_slice(),
        b"shared reason\0\xe9\x9b\xaa",
        b"",
        b"rename bytes",
    ] {
        assert_eq!(old.intern(seed), new.intern(seed));
    }
    // Reuse tables across calls, as well as appending a rename payload.
    for _ in 0..2 {
        let expected = baseline(s, ids, &mut old);
        let actual = encode(s, ids, &mut new);
        assert_eq!(actual, expected);
        assert_eq!(
            graph_words(&actual).unwrap(),
            graph_words(&expected).unwrap()
        );
        for bytes in old.0.clone() {
            assert_eq!(old.intern(&bytes), new.intern(&bytes));
        }
        assert_eq!(
            old.intern(b"new rename payload"),
            new.intern(b"new rename payload")
        );
    }
}
#[test]
fn exact_linear_equivalence_including_invalid_symbols_and_order() {
    for count in 0..=16 {
        for deep in [false, true] {
            let mut s = fixture(count, deep);
            let mut ids: Vec<_> = s.nodes.keys().cloned().collect();
            differential(&s, &ids);
            ids.reverse();
            differential(&s, &ids);
            ids.insert(0, id(999));
            ids.extend(ids.clone()); // First duplicate position, not last, must win.
            differential(&s, &ids);
            ids.retain(|i| i != &id(1)); // Absent UUID becomes zero.
            s.roots.extend([id(999), id(999)]);
            if count > 0 {
                s.children
                    .get_mut(&id(1))
                    .unwrap()
                    .extend([id(999), id(2), id(2)]);
                s.nodes.get_mut(&id(1)).unwrap().see_also.push(Reference {
                    target: id(999),
                    reason: String::new(),
                });
            }
            differential(&s, &ids);
            differential(&s, &[]);
        }
    }
    // Generic byte interning includes identical payloads and hash-equal content in distinct allocations.
    let mut old = LinearTokens::default();
    let mut new = Tokens::default();
    for i in 0..1000 {
        let bytes = vec![(i % 257) as u8; i % 19];
        assert_eq!(old.intern(&bytes), new.intern(&bytes));
    }
}
#[test]
fn compiled_policy_preserves_valid_and_invalid_outcomes() {
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    for deep in [false, true] {
        let s = fixture(8, deep);
        assert!(policy.graph_valid(&s).unwrap());
        for mutation in 0..4 {
            let mut invalid = s.clone();
            match mutation {
                0 => invalid.roots.push(id(1)),
                1 => invalid.children.get_mut(&id(8)).unwrap().push(id(1)),
                2 => invalid.nodes.get_mut(&id(1)).unwrap().see_also[0].target = id(999),
                _ => invalid.children.get_mut(&id(1)).unwrap().push(id(999)),
            }
            differential(&invalid, &invalid.nodes.keys().cloned().collect::<Vec<_>>());
            assert!(
                !policy.graph_valid(&invalid).unwrap(),
                "mutation {mutation}"
            );
        }
    }
}
#[test]
fn large_encoding_equivalence() {
    for (count, deep) in [(100, true), (1000, true), (1698, false), (4096, false)] {
        let s = fixture(count, deep);
        let ids: Vec<_> = s.nodes.keys().cloned().collect();
        let expected = baseline(&s, &ids, &mut LinearTokens::default());
        let actual = encode(&s, &ids, &mut Tokens::default());
        assert_eq!(expected, actual);
        assert_eq!(
            graph_words(&expected).unwrap(),
            graph_words(&actual).unwrap()
        );
    }
}
#[test]
#[ignore = "opt-in seven-round encoding benchmark; equality is checked separately"]
fn encoding_measurements() {
    for (count, deep) in [(100, true), (1000, true), (1698, false), (4096, false)] {
        let s = fixture(count, deep);
        let ids: Vec<_> = s.nodes.keys().cloned().collect();
        let expected = baseline(&s, &ids, &mut LinearTokens::default());
        assert_eq!(expected, encode(&s, &ids, &mut Tokens::default()));
        let mut old_times = Vec::new();
        let mut new_times = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            std::hint::black_box(baseline(&s, &ids, &mut LinearTokens::default()));
            old_times.push(start.elapsed());
            let start = Instant::now();
            std::hint::black_box(encode(&s, &ids, &mut Tokens::default()));
            new_times.push(start.elapsed());
        }
        old_times.sort();
        new_times.sort();
        eprintln!(
            "ENCODING count={count} deep={deep} words={} baseline_median_us={} lookup_median_us={} rounds=7",
            graph_words(&expected).unwrap().len(),
            old_times[3].as_micros(),
            new_times[3].as_micros()
        );
    }
}
#[test]
fn native_capacity_observations() {
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    for (count, deep) in [(100, true), (1000, true), (1698, false), (4096, false)] {
        let s = fixture(count, deep);
        let start = Instant::now();
        match policy.validate(&s) {
            Ok(()) => eprintln!(
                "NATIVE count={count} deep={deep} valid=true elapsed_ms={}",
                start.elapsed().as_millis()
            ),
            Err(e) => {
                // Observation only: a frozen helper deadline is NOT a valid-store pass.
                assert_eq!(e.to_string(), "policy process timed out");
                eprintln!(
                    "NATIVE count={count} deep={deep} VALIDATION_NOT_ESTABLISHED error={e:?} elapsed_ms={}",
                    start.elapsed().as_millis()
                );
            }
        }
    }
}
