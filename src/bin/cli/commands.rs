//! Single public command catalogue for full help and the onboarding index.
use serde_json::json;
use sifttext_file_graph::{Result, lean_render};

pub(super) const UPDATE_GUIDE: &str = "8b5361e1-20d7-42a9-9dca-26ba3bac9f25";
const READ_GUIDE: &str = "5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d";
const SOURCE_GUIDE: &str = "6da0b35a-955a-4ad2-bdce-2994ba26119c";

struct Command {
    syntax: &'static str,
    purpose: &'static str,
    guide: Option<&'static str>,
    indexed: bool,
}

const fn command(syntax: &'static str, purpose: &'static str) -> Command {
    Command { syntax, purpose, guide: None, indexed: true }
}

impl Command {
    const fn guide(mut self, guide: &'static str) -> Self {
        self.guide = Some(guide);
        self
    }
    const fn full_help_only(mut self) -> Self {
        self.indexed = false;
        self
    }
}

struct Group {
    name: &'static str,
    guide: &'static str,
    commands: &'static [Command],
}

const GROUPS: &[Group] = &[
    Group {
        name: "start",
        guide: sifttext_file_graph::local::ENTRY,
        commands: &[
            command("instructions [STORE]", "Find onboarding and pending work"),
            command("init HOME", "Create a bundled knowledge home"),
            command("init HOME --empty", "Create an empty knowledge home"),
            command("init HOME --bundle BUNDLE", "Create a home from a bundle"),
            command("help", "List commands and guides").full_help_only(),
            command("-h", "List commands and guides").full_help_only(),
            command("--help", "List commands and guides").full_help_only(),
        ],
    },
    Group {
        name: "retrieve",
        guide: "526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8",
        commands: &[
            command("read STORE UUID [-y]", "Read node Markdown").guide(READ_GUIDE),
            command("outline STORE UUID [full] [-y]", "Read a shallow or full outline").guide(READ_GUIDE),
            command("search STORE QUERY [UNDER_UUID] [-y]", "Search with ancestor context"),
            command("sql STORE 'SELECT ...' [-y]", "Query the read-only index").guide("4cc0b396-4b55-415e-bb26-b51c97e6ca78"),
            command("index STORE [--repair]", "Refresh derived retrieval files"),
            command("tree-render STORE SELECTION.json", "Render selected trees or proposals").guide("5c7e9d4c-0fe4-4f3a-a098-62177ab4d43f").full_help_only(),
            command("tree-render WORKTREE --auto [PROPOSAL.json]", "Derive draft selection; declare artifact links yourself").guide("5c7e9d4c-0fe4-4f3a-a098-62177ab4d43f").full_help_only(),
        ],
    },
    Group {
        name: "write",
        guide: "fb1e0c9f-8c04-4baf-91e6-4688856a14af",
        commands: &[
            command("worktree HOME", "Allocate a private draft"),
            command("create WORKTREE NAME SUMMARY [PARENT_UUID]", "Create a root or child"),
            command("normalize BEFORE WORKTREE [EXPLICIT_UUID ...] [-y]", "Apply draft status changes"),
            command("submit REPO DRAFT COMMIT BASE [EXPLICIT_UUID ... | --received-statuses] [-y]", "Publish a checked draft"),
            command("accepted REPO [-y]", "Open the checked accepted view"),
            command("advance HOME", "Fast-forward a clean home"),
            command("migrate-order WORKTREE [--legacy]", "Convert draft child ordering").full_help_only(),
        ],
    },
    Group {
        name: "check",
        guide: "1b668703-b591-4790-ae6b-5c4202413de5",
        commands: &[
            command("epistemic-check FILE|-", "Review node grounding"),
            command("epistemic-check --packet FILE", "Review a staged packet"),
            command("slop-check FILE|-", "Review writing quality").guide("869c330b-ec65-4ecd-9d03-9a09dab87010"),
            command("check-result ID [--delete]", "Retrieve or delete a saved check"),
            command("credits", "Show checker credits"),
            command("credits --buy-10 [REQUEST_KEY]", "Request a credit checkout"),
        ],
    },
    Group {
        name: "sync-and-account",
        guide: "a472e05c-9bbf-48f2-89d7-6c6092c339ed",
        commands: &[
            command("sync enable HOME", "Enable hosted sync"),
            command("sync disable HOME", "Disable hosted sync"),
            command("sync decline HOME", "Decline hosted sync"),
            command("sync status HOME", "Inspect retained sync state"),
            command("sync once HOME [--quiet]", "Run one sync exchange"),
            command("sync run HOME", "Run the scheduled sync exchange"),
            command("sync policy HOME FILE.json", "Set sharing rules"),
            command("login", "Sign in to an account"),
            command("account", "Inspect account access"),
            command("logout", "Revoke the saved login"),
        ],
    },
    Group {
        name: "maintain",
        guide: UPDATE_GUIDE,
        commands: &[
            command("update [-y]", "Check and apply official updates"),
            command("update --status", "Check available releases"),
            command("update --local-status", "Read recorded update state"),
            command("update --resume", "Resume a knowledge repair"),
            command("update --rollback", "Restore a previous release"),
            command("update --rebuild", "Rebuild pinned local source"),
            command("update --prepare", "Prepare installed data offline without upgrading"),
            command("update --harness-instructions-applied", "Record applied persistent instructions").guide(sifttext_file_graph::updates::HARNESS_GUIDE),
            command("update --config", "Read update settings"),
            command("update --grant standing|ask", "Set update permission"),
            command("update --attach-node UUID", "Attach a repair undertaking"),
            command("update --pin IDENTITY", "Pin an application build"),
            command("update --unpin", "Follow official application updates"),
            command("source-code", "Find matching source and guidance").guide(SOURCE_GUIDE),
            command("runtime-check", "Verify executable assets").guide(SOURCE_GUIDE),
        ],
    },
    Group {
        name: "sessions",
        guide: "3651b9b0-fc21-4c67-9d79-84ef3af963ce",
        commands: &[
            command("session-retention-config HOME", "Read session retention settings"),
            command("session-capture SOURCE DESCRIPTOR.json ARCHIVE_ROOT", "Retain a session in Git"),
            command("session-attach HOME NODE DESCRIPTOR.json TOPIC ARCHIVE_ROOT [-y]", "Attach a retained session"),
            command("session-backup HOME", "Push the opted-in archive backup"),
            command("session-restore HOME ARCHIVE_ROOT", "Restore retained session links"),
        ],
    },
];

pub(super) fn help() -> Result<String> {
    let rows = GROUPS.iter().flat_map(|group| {
        group.commands.iter().map(|command| vec![
            json!(format!("sifttext {}", command.syntax)),
            json!(command.guide.unwrap_or(group.guide)),
            json!(command.purpose),
        ])
    }).collect::<Vec<_>>();
    Ok(lean_render::render_table("commands", &["command", "guide", "purpose"], &rows)?)
}

pub(super) fn index() -> Result<String> {
    let rows = GROUPS.iter().map(|group| {
        let mut names = Vec::new();
        for command in group.commands.iter().filter(|command| command.indexed) {
            let name = command.syntax.split_whitespace().next().unwrap();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        vec![json!(group.name), json!(names.join(", ")), json!(group.guide)]
    }).collect::<Vec<_>>();
    Ok(lean_render::render_table("command-groups", &["group", "commands", "guide"], &rows)?)
}
