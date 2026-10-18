//! Native inspection adapter. One request, one real library operation.
use serde::Deserialize;
use sifttext_file_graph::{
    Policy, Result,
    neighborhoods::{PauseBeforeAccept, neighborhood},
};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "op")]
enum Request {
    #[serde(rename = "validate_candidate")]
    ValidateCandidate {
        #[serde(flatten)]
        request: sifttext_file_graph::candidate_validation::Request,
    },
    #[serde(rename = "search")]
    Search {
        store: PathBuf,
        query: String,
        under_node_id: Option<String>,
        limit: usize,
        offset: usize,
        readable_ids: BTreeSet<String>,
        #[serde(default)]
        base_store: Option<PathBuf>,
        #[serde(default)]
        ranking: sifttext_file_graph::search::Ranking,
        #[serde(default)]
        pause_before_accept: Option<sifttext_file_graph::index::PauseBeforeAccept>,
    },
    #[serde(rename = "outline_inspect")]
    OutlineInspect {
        store: PathBuf,
        #[serde(default)]
        base_store: Option<PathBuf>,
    },
    #[serde(rename = "index")]
    Index {
        store: PathBuf,
        mode: sifttext_file_graph::index::Mode,
        #[serde(default)]
        base_store: Option<PathBuf>,
        #[serde(default)]
        pause_before_accept: Option<sifttext_file_graph::index::PauseBeforeAccept>,
    },
    #[serde(rename = "index_prepared")]
    IndexPrepared {
        store: PathBuf,
        accepted_main: PathBuf,
        #[serde(default)]
        prepared_worktree: Option<PathBuf>,
        #[serde(default)]
        base_store: Option<PathBuf>,
        #[serde(default)]
        prepared_base: Option<PathBuf>,
        #[serde(default)]
        pause_before_accept: Option<sifttext_file_graph::index::PauseBeforeAccept>,
    },
    #[serde(rename = "token_refresh")]
    TokenRefresh {
        path: PathBuf,
        #[serde(default)]
        pause_before_accept: Option<sifttext_file_graph::token_metadata::PauseBeforeAccept>,
    },
    #[serde(rename = "neighborhood")]
    Neighborhood {
        store: PathBuf,
        id: String,
        readable_ids: BTreeSet<String>,
        #[serde(default)]
        pause_before_accept: Option<PauseBeforeAccept>,
    },
    #[serde(rename = "status")]
    Status {
        before: PathBuf,
        after: PathBuf,
        explicit: BTreeSet<String>,
        #[serde(default)]
        pause_before_accept: Option<sifttext_file_graph::status::PauseBeforeAccept>,
    },
}

// Library consumers may legitimately inspect unavailable views. Only the process
// boundary treats a non-current result as an unsuccessful command.
type CommandStatus = (&'static str, Option<sifttext_file_graph::index::Refusal>);
fn emit(output: &impl serde::Serialize, status: &'static str) -> Result<CommandStatus> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, output)?;
    writeln!(stdout)?;
    Ok((status, None))
}

fn emit_retrieval(
    output: &impl serde::Serialize,
    status: &'static str,
    refusal: Option<sifttext_file_graph::index::Refusal>,
    diagnostics: bool,
) -> Result<CommandStatus> {
    emit(output, status)?;
    Ok((status, diagnostics.then_some(refusal).flatten()))
}

fn error_kind(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if error.is::<serde_json::Error>() {
        return "invalid_request";
    }
    if let Some(error) = error.downcast_ref::<io::Error>() {
        return match error.kind() {
            io::ErrorKind::NotFound => "not_found",
            io::ErrorKind::PermissionDenied => "permission_denied",
            io::ErrorKind::StorageFull
            | io::ErrorKind::FileTooLarge
            | io::ErrorKind::OutOfMemory => "resource_exhausted",
            io::ErrorKind::TimedOut => "timeout",
            io::ErrorKind::WouldBlock => "busy",
            _ => "io",
        };
    }
    if let Some(rusqlite::Error::SqliteFailure(error, _)) = error.downcast_ref() {
        return match error.code {
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => "busy",
            rusqlite::ErrorCode::DiskFull
            | rusqlite::ErrorCode::OutOfMemory
            | rusqlite::ErrorCode::TooBig => "resource_exhausted",
            rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly => {
                "permission_denied"
            }
            _ => "sqlite",
        };
    }
    // Older library guards use string errors. Match only fixed identities, never
    // interpolate a parser/filesystem/policy message that could contain secrets.
    match error.to_string().as_str() {
        "policy process timed out"
        | "barrier expired"
        | "inspection barrier expired"
        | "refresh lock expired" => "timeout",
        "oversized request"
        | "oversized search"
        | "oversized node file"
        | "policy process output exceeds limit" => "resource_exhausted",
        _ => "rejected",
    }
}

#[derive(Debug)]
struct CandidateRefusal {
    status: &'static str,
    failure: sifttext_file_graph::candidate_validation::Failure,
}
impl std::fmt::Display for CandidateRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("candidate refused")
    }
}
impl std::error::Error for CandidateRefusal {}

fn run(diagnostics: &mut bool) -> Result<CommandStatus> {
    let mut input = Vec::new();
    io::stdin().take(1_048_577).read_to_end(&mut input)?;
    if input.len() > 1_048_576 {
        return Err("oversized request".into());
    }
    let mut input: serde_json::Value = serde_json::from_slice(&input)?;
    if let Some(value) = input.as_object_mut().and_then(|o| o.remove("diagnostics")) {
        *diagnostics = value.as_bool().ok_or("diagnostics must be boolean")?;
    }
    let request: Request = serde_json::from_value(input)?;
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    match request {
        Request::ValidateCandidate { request } => {
            let output = sifttext_file_graph::candidate_validation::validate(&policy, &request);
            emit(&output, output.status)?;
            if output.status != "current" {
                return Err(Box::new(CandidateRefusal {
                    status: output.status,
                    failure: output.failure.unwrap_or(
                        sifttext_file_graph::candidate_validation::Failure {
                            stage: "candidate",
                            kind: "unknown",
                        },
                    ),
                }));
            }
            Ok((output.status, None))
        }
        Request::Search {
            store,
            query,
            under_node_id,
            limit,
            offset,
            readable_ids,
            base_store,
            ranking,
            pause_before_accept,
        } => {
            let output = sifttext_file_graph::search::search(
                &policy,
                &sifttext_file_graph::search::Request {
                    store: &store,
                    base_store: base_store.as_deref(),
                    query: &query,
                    under_node_id: under_node_id.as_deref(),
                    readable_ids: &readable_ids,
                    limit,
                    offset,
                    ranking: &ranking,
                    pause_before_accept: pause_before_accept.as_ref(),
                },
            )?;
            emit_retrieval(
                &sifttext_file_graph::search::observation(&output),
                output.status,
                output.refusal,
                *diagnostics,
            )
        }
        Request::OutlineInspect { store, base_store } => {
            let output =
                sifttext_file_graph::outlines::inspect(&policy, &store, base_store.as_deref())?;
            emit(&output, output.status)
        }
        Request::Index {
            store,
            mode,
            base_store,
            pause_before_accept,
        } => {
            let output = sifttext_file_graph::index::refresh(
                &policy,
                &store,
                base_store.as_deref(),
                mode,
                pause_before_accept.as_ref(),
            )?;
            emit_retrieval(&output, output.status, output.refusal.clone(), *diagnostics)
        }
        Request::IndexPrepared {
            store,
            accepted_main,
            prepared_worktree,
            base_store,
            prepared_base,
            pause_before_accept,
        } => {
            use sifttext_file_graph::index;
            let main = index::prepare_reuse(&policy, &accepted_main, None)?;
            let prepared = prepared_worktree
                .as_deref()
                .map(|p| index::prepare_reuse(&policy, p, prepared_base.as_deref()))
                .transpose()?
                .flatten();
            let (view, reuse) = if let Some(main) = &main {
                index::refresh_prepared(
                    &policy,
                    &store,
                    base_store.as_deref(),
                    main,
                    prepared.as_ref(),
                    pause_before_accept.as_ref(),
                )?
            } else {
                (
                    index::refresh(
                        &policy,
                        &store,
                        base_store.as_deref(),
                        index::Mode::Incremental,
                        pause_before_accept.as_ref(),
                    )?,
                    index::Reuse::default(),
                )
            };
            let status = view.status;
            emit_retrieval(
                &serde_json::json!({"view":view,"reuse":reuse,"accepted_main_available":main.is_some(),"prepared_worktree_available":prepared.is_some()}),
                status,
                view.refusal,
                *diagnostics,
            )
        }
        Request::TokenRefresh {
            path,
            pause_before_accept,
        } => {
            let output = sifttext_file_graph::token_metadata::Refresher::default()
                .refresh(&path, pause_before_accept.as_ref())?;
            emit(&output, output.status)
        }
        Request::Neighborhood {
            store,
            id,
            readable_ids,
            pause_before_accept,
        } => {
            let output = neighborhood(
                &policy,
                &store,
                &id,
                &readable_ids,
                pause_before_accept.as_ref(),
            )?;
            emit(&output, "current")
        }
        Request::Status {
            before,
            after,
            explicit,
            pause_before_accept,
        } => {
            let output = sifttext_file_graph::status::normalize(
                &policy,
                &before,
                &after,
                &explicit,
                pause_before_accept.as_ref(),
            )?;
            emit(&output, output.status)
        }
    }
}

fn main() {
    let mut diagnostics = false;
    let diagnostic = match run(&mut diagnostics) {
        Ok(("current", _)) => return,
        Ok((status, Some(refusal))) => {
            serde_json::json!({"status":status,"kind":refusal.kind,"stage":refusal.stage})
        }
        Ok((status, None)) => serde_json::json!({"status":status,"kind":"unavailable"}),
        Err(error) if error.is::<CandidateRefusal>() => {
            let refusal = error.downcast_ref::<CandidateRefusal>().unwrap();
            serde_json::json!({"status":refusal.status,"kind":refusal.failure.kind,"stage":refusal.failure.stage})
        }
        Err(error) if diagnostics => {
            let stage = if error.is::<rusqlite::Error>() {
                "index_cache"
            } else {
                "operation"
            };
            let refusal = sifttext_file_graph::index::Refusal::from_error(stage, error.as_ref());
            serde_json::json!({"status":"error","kind":refusal.kind,"stage":refusal.stage})
        }
        Err(error) => serde_json::json!({"status":"error","kind":error_kind(error.as_ref())}),
    };
    // Successful results may contain requested authored data. Failure diagnostics
    // contain fixed identities only; unavailable stdout retains its existing shape.
    eprintln!("{diagnostic}");
    std::process::exit(1);
}
