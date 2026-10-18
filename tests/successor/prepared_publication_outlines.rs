use super::*;

// Fixed simple-text fixture rows, independent of the product assembler/renderer.
fn expected(version: &str, rows: &[(usize, usize, usize, usize, &str)]) -> String {
    let mut text = format!(
        "coverage:full\npath_template:nodes/{{id}}/node.md\nrows[{}]:children\tdepth\tdescendants\tid\tmicro_summary\tname\tstatus\n",
        rows.len()
    );
    for &(n, depth, children, descendants, name) in rows {
        text.push_str(&format!(
            "  {children}\t{depth}\t{descendants}\t{}\tSummary\t{name}\tin_progress\n",
            id(n)
        ));
    }
    text + &format!(
        "source_version:{version}\nsource_version_scope:outline_view\ntoken_encoding:o200k_base\n"
    )
}

fn matches(text: &str, rows: &[(usize, usize, usize, usize, &str)]) -> bool {
    let Some(version) = text
        .lines()
        .find_map(|line| line.strip_prefix("source_version:"))
    else {
        return false;
    };
    version.len() == 64
        && version.bytes().all(|b| b.is_ascii_hexdigit())
        && text == expected(version, rows)
}

pub(super) fn disjoint(view: &Path) {
    let text = fs::read_to_string(view.join(format!("nodes/{}/outline.full.lean", id(1)))).unwrap();
    assert!(
        matches(
            &text,
            &[
                (1, 0, 2, 2, "Node 1"),
                (2, 1, 0, 0, "Prepared X"),
                (3, 1, 0, 0, "Published Y"),
            ]
        ),
        "wrong complete root outline: {text}"
    );
}

#[test]
fn root_outline_expectations_reject_wrong_counts_order_and_membership() {
    let rows = [
        (1, 0, 2, 2, "Node 1"),
        (2, 1, 0, 0, "Prepared X"),
        (3, 1, 0, 0, "Published Y"),
    ];
    let text = expected(&"a".repeat(64), &rows);
    assert!(matches(&text, &rows));
    for bad in [
        text.replace("  2\t0\t2\t", "  2\t0\t1\t"),
        text.replace("rows[3]:", "rows[2]:"),
        expected(&"a".repeat(64), &[rows[0], rows[2], rows[1]]),
        text.replace(&id(3), &id(4)),
        text.replace("Published Y", "stale name"),
    ] {
        assert!(
            !matches(&bad, &rows),
            "accepted corrupted root outline: {bad}"
        );
    }
}

#[test]
fn native_branch_inserts_publish_complete_root_outline() {
    let f = Fixture::new();
    for (draft, parent, child) in [(&f.a, 2, 4), (&f.b, 3, 5)] {
        add(draft, child);
        std::os::unix::fs::symlink(
            format!("../../{}", id(child)),
            draft.join(format!("nodes/{}/children/{}", id(parent), id(child))),
        )
        .unwrap();
        fs::write(
            draft.join(format!("nodes/{}/children.order", id(parent))),
            id(child) + "\n",
        )
        .unwrap();
        commit(draft);
    }
    let a_before = precas::fingerprint(&f.a);
    let b_before = precas::fingerprint(&f.b);
    let b = main_donors::native("git-submit-observe", f.request(&f.b));
    assert_eq!(b["status"], "current");
    let a = main_donors::native("git-submit-observe", f.request(&f.a));
    assert_eq!(a["status"], "current");
    let view = Path::new(a["view"].as_str().unwrap());
    assert_eq!(
        git(view, &["rev-parse", "HEAD"]),
        a["candidate"].as_str().unwrap()
    );
    assert_eq!(
        git(view, &["rev-parse", "HEAD^{tree}"]),
        a["tree"].as_str().unwrap()
    );
    let indexed = main_donors::native(
        "core-observe",
        json!({"op":"index","store":view,"mode":"incremental"}),
    );
    assert_eq!(indexed["status"], "current");
    assert_eq!(indexed["version"], a["version"]);
    assert_eq!(indexed["nodes"].as_array().unwrap().len(), 5);
    let inspected = main_donors::native(
        "core-observe",
        json!({"op":"outline_inspect","store":view}),
    );
    assert_eq!(inspected["status"], "current");
    assert_eq!(inspected["version"], a["version"]);
    let text = fs::read_to_string(view.join(format!("nodes/{}/outline.full.lean", id(1)))).unwrap();
    assert!(
        matches(
            &text,
            &[
                (1, 0, 2, 4, "Node 1"),
                (2, 1, 1, 1, "Node 2"),
                (4, 2, 0, 0, "Node 4"),
                (3, 1, 1, 1, "Node 3"),
                (5, 2, 0, 0, "Node 5"),
            ]
        ),
        "wrong merged root outline: {text}"
    );
    assert_eq!(precas::fingerprint(&f.a), a_before);
    assert_eq!(precas::fingerprint(&f.b), b_before);
}
