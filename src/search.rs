//! Ranked lexical retrieval of one complete checked logical working view.
use crate::leviathan_search::{self, Document};
use crate::{Policy, Result, index, require, search_document};
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
    pub hits: Vec<Hit>,
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
    let (view, found) = index::with_query(
        policy,
        request.store,
        request.base_store,
        index::Mode::Incremental,
        request.pause_before_accept,
        |tx, store, view| {
            let mut scope = BTreeSet::new();
            if let Some(id) = request.under_node_id {
                let id = if local {
                    store
                        .resolve(id)
                        .map_err(|e| index::Refusal::from_error("scope", e.as_ref()))?
                } else {
                    require(store.nodes.contains_key(id), "unknown scope")?;
                    id.to_owned()
                };
                let mut pending = vec![id];
                while let Some(id) = pending.pop() {
                    pending.extend(store.children[&id].iter().cloned());
                    scope.insert(id);
                }
            } else {
                scope.extend(store.nodes.keys().cloned());
            }
            if !local {
                scope.retain(|id| request.readable_ids.contains(id));
            }
            let projected = view
                .nodes
                .iter()
                .map(|r| {
                    search_document::project(search_document::Fields {
                        name: r["name"].as_str().unwrap(),
                        micro_summary: r["micro_summary"].as_str().unwrap(),
                        scope: r["scope"].as_str().unwrap_or(""),
                        crystallization: r["content"].as_str().unwrap(),
                    })
                    .map_err(Into::into)
                })
                .collect::<Result<Vec<_>>>()?;
            let documents = view
                .nodes
                .iter()
                .zip(&projected)
                .map(|(r, d)| {
                    let id = r["id"].as_str().unwrap();
                    Document {
                        id,
                        title: d.title,
                        body: &d.body,
                        permitted: scope.contains(id),
                        boost: *request.ranking.boosts.get(id).unwrap_or(&1.0),
                    }
                })
                .collect::<Vec<_>>();
            let (total, cards) = leviathan_search::retrieve(
                tx,
                &documents,
                request.query,
                request.limit,
                request.offset,
                request.ranking.title_weight,
            )?;
            let hits = cards
                .iter()
                .map(|card| {
                    let r = view.nodes.iter().find(|r| r["id"] == card.text.id).unwrap();
                    let status = r["status"].as_str().unwrap();
                    let key = match status {
                        "stuck" => "blocker",
                        "deferred" => "deferred_reason",
                        "discarded" => "discard_reason",
                        _ => "",
                    };
                    let text = |key| r[key].as_str().map(str::to_owned);
                    Hit {
                        id: card.text.id.clone(),
                        name: text("name").unwrap(),
                        micro_summary: text("micro_summary").unwrap(),
                        matching_excerpt: card.text.snippet.clone(),
                        path: store.path.join("nodes").join(&card.text.id).join("node.md"),
                        status: status.to_owned(),
                        active_reason: text(key),
                        warnings: text("warnings").filter(|s| !s.trim().is_empty()),
                    }
                })
                .collect();
            Ok((total, cards, hits))
        },
    )?;
    let (total_matches, cards, hits) = found.unwrap_or((0, vec![], vec![]));
    Ok(Outcome {
        refusal: view.refusal,
        status: view.status,
        version: view.version,
        total_matches,
        cards,
        hits,
    })
}

/// Thin native differential surface mandated by the maintained run contract.
pub fn observation(out: &Outcome) -> Value {
    serde_json::json!({"status":out.status,"version":out.version,
        "total_matches":out.total_matches,"results":out.cards,"other_groups":[]})
}
