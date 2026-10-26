//! Ranked lexical retrieval from the native current working-view index.
use crate::leviathan_search;
use crate::{Policy, Result, index, require};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

/// Explicit upstream ranking settings, not inferred status boosts. The native
/// adapter defaults to upstream title weight 2 and boost 1 for every node.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ranking {
    pub title_weight: f64,
    #[serde(default)]
    pub boosts: BTreeMap<String, f64>,
}
impl Default for Ranking {
    fn default() -> Self {
        Self {
            title_weight: 2.0,
            boosts: BTreeMap::new(),
        }
    }
}

pub struct Request<'a> {
    pub store: &'a Path,
    pub base_store: Option<&'a Path>,
    pub query: &'a str,
    pub under_node_id: Option<&'a str>,
    pub readable_ids: &'a BTreeSet<String>,
    pub limit: usize,
    pub offset: usize,
    pub ranking: &'a Ranking,
    pub pause_before_accept: Option<&'a index::PauseBeforeAccept>,
}

/// The actual Core hit surface; upstream card presentation is a separate
/// bounded comparison adapter, not the final hit guardrail contract.
#[derive(Debug, Serialize)]
pub struct Hit {
    pub id: String,
    pub name: String,
    pub micro_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matching_excerpt: Option<String>,
    pub path: PathBuf,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Outcome {
    #[serde(skip)]
    pub refusal: Option<index::Refusal>,
    pub status: &'static str,
    pub version: Option<String>,
    pub total_matches: i64,
    pub path_template: String,
    pub hits: Vec<Hit>,
    /// Ranked root-to-hit chains from the same capture, with unreadable positions redacted.
    pub rows: Vec<Value>,
    /// Upstream fields for differential comparison; contains no protected hits.
    pub cards: Vec<leviathan_search::Card>,
}

/// Local filesystem search resolves prefixes and discovers readable nodes only
/// inside the same locked capture used for retrieval. Filtered callers use search.
pub fn local_search(
    policy: &Policy,
    store: &Path,
    query: &str,
    under: Option<&str>,
) -> Result<Outcome> {
    search_with(
        policy,
        &Request {
            store,
            base_store: None,
            query,
            under_node_id: under,
            readable_ids: &BTreeSet::new(),
            limit: 20,
            offset: 0,
            ranking: &Ranking::default(),
            pause_before_accept: None,
        },
        true,
    )
}

pub fn search(policy: &Policy, request: &Request<'_>) -> Result<Outcome> {
    search_with(policy, request, false)
}

fn search_with(policy: &Policy, request: &Request<'_>, local: bool) -> Result<Outcome> {
    require(
        request.query.len() <= 65_536 && request.offset <= 1_000_000,
        "oversized search",
    )?;
    require(
        request.ranking.title_weight.is_finite() && request.ranking.title_weight >= 0.0,
        "invalid title weight",
    )?;
    require(
        request
            .ranking
            .boosts
            .values()
            .all(|b| b.is_finite() && *b > 0.0),
        "invalid boost",
    )?;
    require(
        request.readable_ids.iter().all(|id| crate::uuid(id)),
        "invalid readable identity",
    )?;
    if !local && let Some(id) = request.under_node_id {
        require(crate::uuid(id), "invalid scope identity")?;
    }
    require(
        request.base_store.is_none(),
        "unsupported old sparse adapter; use a complete worktree",
    )?;
    require(
        request.pause_before_accept.is_none(),
        "unsupported old retrieval pause adapter; use a complete worktree without a pause",
    )?;
    indexed_search(policy, request, local)
}

fn indexed_search(policy: &Policy, request: &Request<'_>, local: bool) -> Result<Outcome> {
    let (version, (total_matches, cards, hits, rows, path_template)) =
        index::with_current(policy, request.store, |tx, _version| {
            // Compact hierarchy only; bodies/record JSON are decoded for results
            // and requested ancestor context, never the complete corpus.
            let children: BTreeMap<String, Vec<String>> = tx
                .prepare("SELECT id,children FROM nodes ORDER BY id")?
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .map(|row| {
                    let (id, children) = row?;
                    Ok((id, serde_json::from_str(&children)?))
                })
                .collect::<Result<_>>()?;
            let mut scope = BTreeSet::new();
            if let Some(prefix) = request.under_node_id {
                let matches: Vec<_> = children
                    .keys()
                    .filter(|id| {
                        if local {
                            !prefix.is_empty() && id.starts_with(prefix)
                        } else {
                            id.as_str() == prefix
                        }
                    })
                    .cloned()
                    .collect();
                require(matches.len() == 1, "UUID prefix absent or ambiguous")?;
                let mut pending = matches;
                while let Some(id) = pending.pop() {
                    if !children.contains_key(&id) || !scope.insert(id.clone()) {
                        continue;
                    }
                    pending.extend(children[&id].iter().cloned());
                }
            } else {
                scope.extend(children.keys().cloned());
            }
            if !local {
                scope.retain(|id| request.readable_ids.contains(id));
            }
            let (total, cards) = leviathan_search::retrieve_indexed(
                tx,
                &scope,
                &request.ranking.boosts,
                request.query,
                request.limit,
                request.offset,
                request.ranking.title_weight,
            )?;
            let mut parents = BTreeMap::new();
            for (parent, children) in &children {
                for child in children {
                    parents.insert(child.as_str(), parent.as_str());
                }
            }
            let root = std::fs::canonicalize(request.store)?;
            let make_hit = |id: &str, matching_excerpt: Option<String>| -> Result<Hit> {
                let record: String =
                    tx.query_row("SELECT record FROM nodes WHERE id=?1", [id], |r| r.get(0))?;
                let row: Value = serde_json::from_str(&record)?;
                let status = row["status"].as_str().ok_or("invalid indexed status")?;
                let reason = match status {
                    "stuck" => "blocker",
                    "deferred" => "deferred_reason",
                    "discarded" => "discard_reason",
                    _ => "",
                };
                let text = |key| row[key].as_str().map(str::to_owned);
                Ok(Hit {
                    id: id.to_owned(),
                    name: text("name").ok_or("invalid indexed name")?,
                    micro_summary: text("micro_summary").ok_or("invalid indexed summary")?,
                    matching_excerpt,
                    path: root.join("nodes").join(id).join("node.md"),
                    status: status.to_owned(),
                    active_reason: text(reason),
                    warnings: text("warnings").filter(|s| !s.trim().is_empty()),
                })
            };
            let hits = cards
                .iter()
                .map(|c| make_hit(&c.text.id, c.text.snippet.clone()))
                .collect::<Result<Vec<_>>>()?;
            let mut rows = Vec::new();
            for (rank, hit) in hits.iter().enumerate() {
                let mut chain = vec![hit.id.as_str()];
                let mut seen = BTreeSet::from([hit.id.as_str()]);
                while let Some(parent) = parents.get(chain.last().unwrap()) {
                    if !children.contains_key(*parent) || !seen.insert(*parent) {
                        break;
                    }
                    chain.push(parent);
                }
                chain.reverse();
                for (depth, id) in chain.into_iter().enumerate() {
                    let mut row = if id == hit.id {
                        let mut row = serde_json::to_value(hit)?.as_object().unwrap().clone();
                        row.insert("match".into(), Value::Bool(true));
                        row
                    } else if local || request.readable_ids.contains(id) {
                        let mut row = serde_json::to_value(make_hit(id, None)?)?
                            .as_object()
                            .unwrap()
                            .clone();
                        row.remove("path");
                        row
                    } else {
                        serde_json::json!({"unavailable":true})
                            .as_object()
                            .unwrap()
                            .clone()
                    };
                    row.insert("hit".into(), (request.offset + rank + 1).into());
                    row.insert("depth".into(), depth.into());
                    rows.push(Value::Object(row));
                }
            }
            let path_template = root
                .join("nodes/{id}/node.md")
                .to_str()
                .ok_or("non-UTF-8 store path")?
                .to_owned();
            Ok((total, cards, hits, rows, path_template))
        })?;
    Ok(Outcome {
        refusal: None,
        status: "current",
        version: Some(version),
        total_matches,
        path_template,
        hits,
        rows,
        cards,
    })
}

impl Outcome {
    /// Discovery projection only. Rich same-capture chains remain available internally.
    pub fn cli_value(&self) -> Value {
        let path_template = &self.path_template;
        let mut output = serde_json::json!({"status":self.status,"version":self.version,
            "total_matches":self.total_matches,"path_template":path_template});
        let mut warnings = BTreeMap::new();
        let mut rows = Vec::new();
        let mut chain = Vec::new();
        for source in &self.rows {
            if source["depth"] == 0 {
                chain.clear();
            }
            chain.push(source);
            if let (Some(id), Some(warning)) = (source["id"].as_str(), source["warnings"].as_str())
            {
                warnings.entry(id).or_insert(warning);
            }
            if source["match"] != true {
                continue;
            }
            let mut row = source.as_object().unwrap().clone();
            for key in ["hit", "depth", "match", "path", "active_reason", "warnings"] {
                row.remove(key);
            }
            let context =
                |source: &Value| source["id"].as_str().unwrap_or("unavailable").to_owned();
            if chain.len() > 1 {
                row.insert("root".into(), context(chain[0]).into());
                row.insert("parent".into(), context(chain[chain.len() - 2]).into());
            }
            // Upstream snippets are query-specific; don't repeat text already shown intact.
            if let Some(excerpt) = row.get("matching_excerpt").and_then(Value::as_str) {
                let flat = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
                let excerpt = flat(excerpt.trim_matches('…'));
                if ["name", "micro_summary"]
                    .iter()
                    .any(|key| flat(row[*key].as_str().unwrap()).contains(&excerpt))
                {
                    row.remove("matching_excerpt");
                }
            }
            rows.push(Value::Object(row));
        }
        // Guardrails apply along every readable ancestor chain, once per source.
        // Oversized guidance becomes an explicit mandatory source read, never a clipped warning.
        for (id, warning) in warnings {
            if warning.len() > 512 && crate::token_count::ordinary(warning) > 512 {
                output[format!("must_read_warnings_{id}")] =
                    path_template.replace("{id}", id).into();
            } else {
                output[format!("warnings_{id}")] = warning.into();
            }
        }
        output["rows"] = rows.into();
        output
    }
}

/// Thin native differential surface mandated by the maintained run contract.
pub fn observation(out: &Outcome) -> Value {
    serde_json::json!({"status":out.status,"version":out.version,
        "total_matches":out.total_matches,"results":out.cards,"other_groups":[]})
}
