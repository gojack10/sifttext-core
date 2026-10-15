// Finite synthetic numeric-model integration, NOT universal proof or Markdown fidelity.
// Runs the actual compiled Bend definition against an independent parent-map oracle.
use sifttext_file_graph::{Policy, invoke};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Node {
    id: usize,
    payload: usize,
    children: Vec<usize>,
    refs: Vec<(usize, usize)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Graph {
    assets: usize,
    roots: Vec<usize>,
    nodes: Vec<Node>,
}
impl Graph {
    fn words(&self) -> String {
        let mut words = vec![self.assets, self.roots.len()];
        words.extend(&self.roots);
        words.push(self.nodes.len());
        for n in &self.nodes {
            words.extend([n.id, n.payload, n.children.len()]);
            words.extend(&n.children);
            words.push(n.refs.len());
            for &(target, reason) in &n.refs {
                words.extend([target, reason]);
            }
        }
        words
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn parent(&self, id: usize) -> Option<usize> {
        self.nodes
            .iter()
            .find(|n| n.children.contains(&id))
            .map(|n| n.id)
    }
    fn expected(&self, kind: usize, x: usize, y: usize) -> Option<Self> {
        if x == 0 || x > self.nodes.len() {
            return None;
        }
        let mut next = self.clone();
        match kind {
            1 => {
                let old = self.parent(x)?;
                if y == 0 || y > self.nodes.len() {
                    return None;
                }
                // Independent ancestor walk, not Bend's fuel-bounded helper.
                let mut ancestor = Some(y);
                while let Some(id) = ancestor {
                    if id == x {
                        return None;
                    }
                    ancestor = self.parent(id);
                }
                next.nodes[old - 1].children.retain(|&id| id != x);
                next.nodes[y - 1].children.push(x);
            }
            2 => {
                let parent = self.parent(x)?;
                if x == y || self.parent(y) != Some(parent) {
                    return None;
                }
                let children = &mut next.nodes[parent - 1].children;
                children.retain(|&id| id != x);
                let position = children.iter().position(|&id| id == y)?;
                children.insert(position + 1, x);
            }
            3 => {
                if y == 0 {
                    return None;
                }
                next.nodes[x - 1].payload = y;
            }
            _ => panic!("unexpected oracle operation"),
        }
        Some(next)
    }
}
fn is_forest(parents: &[usize]) -> bool {
    for id in 1..=parents.len() {
        let mut current = id;
        let mut seen = vec![false; parents.len() + 1];
        while current != 0 {
            if seen[current] {
                return false;
            }
            seen[current] = true;
            current = parents[current - 1];
        }
    }
    true
}
fn graph(parents: &[usize], reverse: bool) -> Graph {
    let n = parents.len();
    let mut g = Graph {
        assets: 91,
        roots: (1..=n).filter(|&id| parents[id - 1] == 0).collect(),
        nodes: (1..=n)
            .map(|id| Node {
                id,
                payload: 100 + id,
                children: (1..=n).filter(|&child| parents[child - 1] == id).collect(),
                refs: vec![(id % n + 1, 200 + id), (id, 300 + id)],
            })
            .collect(),
    };
    if reverse {
        g.roots.reverse();
        for node in &mut g.nodes {
            node.children.reverse();
        }
    }
    g
}
fn verdict(expected: Option<Graph>) -> String {
    expected.map_or_else(
        || "STG1 REJECT\n".into(),
        |g| format!("STG1 OK {}\n", g.words()),
    )
}

fn check_forests(
    project: &Path,
    worker: usize,
    workers: usize,
) -> (usize, usize, [usize; 3], [usize; 3]) {
    let executable = project.join("build/graph-policy");
    let mut forests = 0;
    let mut operations = 0;
    let mut accepted = [0; 3];
    let mut rejected = [0; 3];
    for n in 1_usize..=4 {
        for encoded in 0..(n + 1).pow(n as u32) {
            if encoded % workers != worker {
                continue;
            }
            let mut value = encoded;
            let parents = (0..n)
                .map(|_| {
                    let p = value % (n + 1);
                    value /= n + 1;
                    p
                })
                .collect::<Vec<_>>();
            if !is_forest(&parents) {
                continue;
            }
            forests += 1;
            for reverse in [false, true] {
                let g = graph(&parents, reverse);
                assert_eq!(
                    invoke(&executable, format!("1 0 0 0 {}", g.words()).as_bytes()).unwrap(),
                    "STG1 VALID\n"
                );
                for kind in 1..=3 {
                    for x in 0..=n + 1 {
                        let ys = if kind == 3 {
                            vec![0, 777]
                        } else {
                            (0..=n + 1).collect()
                        };
                        for y in ys {
                            let expected = g.expected(kind, x, y);
                            if expected.is_some() {
                                accepted[kind - 1] += 1;
                            } else {
                                rejected[kind - 1] += 1;
                            }
                            let wire = format!("1 {kind} {x} {y} {}", g.words());
                            assert_eq!(
                                invoke(&executable, wire.as_bytes()).unwrap(),
                                verdict(expected),
                                "parents={parents:?}, reverse={reverse}, kind={kind}, x={x}, y={y}"
                            );
                            operations += 1;
                        }
                    }
                }
            }
        }
    }
    (forests, operations, accepted, rejected)
}

#[test]
fn compiled_bend_matches_all_small_parent_forests_and_requests() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    Policy::checked(project).unwrap();
    // ponytail: at most four isolated policy processes; tune only with measured contention.
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
    let (mut forests, mut operations, mut accepted, mut rejected) = (0, 0, [0; 3], [0; 3]);
    std::thread::scope(|scope| {
        let handles = (0..workers)
            .map(|worker| scope.spawn(move || check_forests(project, worker, workers)))
            .collect::<Vec<_>>();
        for handle in handles {
            let (f, o, a, r) = handle.join().unwrap();
            forests += f;
            operations += o;
            for kind in 0..3 {
                accepted[kind] += a[kind];
                rejected[kind] += r[kind];
            }
        }
    });
    assert_eq!(forests, 145);
    assert_eq!(operations, 23_208);
    assert!(accepted.iter().all(|&n| n > 0)); // reject-all cannot satisfy the oracle.
    assert!(rejected.iter().all(|&n| n > 0));
    Policy::checked(project).unwrap();
    println!(
        "FINITE NUMERIC INTEGRATION: {forests} forests, 2 root/sibling-order variants, {operations} requests; accepted={accepted:?}, rejected={rejected:?}. No universal or Markdown-native claim."
    );
}

#[test]
fn independent_oracle_detects_delete_all_and_unrelated_changes() {
    let g = graph(&[0, 1, 1, 3], false);
    let expected = g.expected(1, 3, 2).unwrap();
    assert_eq!(expected.nodes[0].children, vec![2]);
    assert_eq!(expected.nodes[1].children, vec![3]);
    assert_eq!(expected.nodes[2].children, vec![4]);
    assert_ne!(verdict(Some(expected.clone())), "STG1 OK 0 0 0\n");
    let mut wrong = expected.clone();
    wrong.nodes[3].payload = 999;
    assert_ne!(verdict(Some(expected.clone())), verdict(Some(wrong)));
    let mut wrong = expected.clone();
    wrong.nodes[2].children.clear();
    assert_ne!(verdict(Some(expected)), verdict(Some(wrong)));
    assert_ne!(verdict(g.expected(2, 2, 3)), "STG1 REJECT\n");
}
