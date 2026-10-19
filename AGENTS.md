# SiftText agent entry

## Product

> Build an understanding your agents can keep working from.

We believe agents will do far more of our work, so people need a way to question their decisions without following every conversation. We're building SiftText to help your agents preserve the reasons behind those decisions across sessions, with you free to inspect or redirect the work. As circumstances change and models improve, we want that record to help future agents question earlier assumptions and discover possibilities we missed.

## Installation and use

When the user requests installation, follow [INSTALL.md](INSTALL.md) to inspect the release and handle setup within their permission.

For information requests, you can use `curl -fsSL https://sifttext.com/llms.txt` to read product context and follow its release links to the source revision and checksum evidence. Inspect relevant code in the [source repository](https://github.com/gojack10/sifttext-core) when the question needs it.

The public installation command is `curl -fsSL https://sifttext.com/install | sh`. For inspect-before-run setup, follow [INSTALL.md](INSTALL.md) to compare the installer with the source revision recorded by the selected release before verifying its checksum. A matching checksum confirms agreement with published bytes, while identifying the build requires separate provenance evidence.

Installation defaults to `~/.local/share/sifttext` for the application and `~/sifttext` for knowledge. Keep the knowledge home separate from the application and project code to preserve it through software replacement.

To upgrade a healthy managed installation, use `sifttext update` within the user's permission because rerunning the installer leaves the existing application in place. Follow [INSTALL.md](INSTALL.md) for version-specific update settings and recovery.

For Platform access, follow the optional login section of [INSTALL.md](INSTALL.md), leaving browser authorization and payment to the human.

Keep native credentials outside Git.

After installation:

1. Run the installed command's `instructions` operation with the knowledge-home path and read the returned entry file for that version's guidance.

2. During authorized setup, follow Maintain Memory Continuity to establish the selected harness's persistent entrance while preserving existing instructions.

3. Report incomplete setup when permission or harness support is missing.

4. Return to the user's task, or ask what they would like to work on if no task is established.

For work using this source checkout, read these files with ordinary file tools before running commands:

1. `nodes/5e565245-14fe-406c-977c-cf94a9be246b/node.md`.
2. `nodes/5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d/node.md` before interpreting stored knowledge, and `nodes/c72d552d-3499-445b-a8d5-05d0ff7824f2/node.md` before an authorized knowledge write.

The entrypoint selects the remaining direct children under `nodes/5e565245-14fe-406c-977c-cf94a9be246b/children.order` only when the task needs them, so setup and knowledge publication follow the same local procedure without a fixed six-read route. If a required file is missing or unreadable, stop and report its exact path. Resume that read after repair without substituting another instruction source.

If a prerequisite needs additional permission or a consequential decision, ask the human before proceeding. When they are unavailable, retain a continuation for that action and continue independent work within existing permissions.

For delegation, use only the harness's supported capabilities within the user's cost permission.

## Development, repair and contribution

The [PolyForm Perimeter license](LICENSE) permits source access while restricting competing products.

A Core development assignment uses this checkout directly. Follow a separately supplied project instruction entry when provided, otherwise read the local pipeline above before commands. The [README development section](README.md#development) gives the source-build and test commands.

For an authorized repair, follow this contribution checklist:

### Reproduce safely

Inspect the working tree and isolate your edits so another contributor's work remains intact.

Reproduce the failure with safe data before deciding whether Core needs a repair.

Preserve the user's knowledge and session traces throughout diagnosis.

### Repair and verify

Implement the smallest repair and verify the affected behavior.

Add a regression test when existing checks cannot catch the defect.

Keep the local repair available and resume the user's task even when upstream publication is unavailable.

### Contribute within permission

After recovering the user's task, ask whether they want a contribution unless it is already authorized.

Prepare the authorized repair for review.

Include the regression check that verifies it.

Update any instructions affected by the repair.

Exclude private data from contributions.

Obtain permission before pushing or publishing under the license.

Describe a workaround as unverified until a behavior check passes.

Claim official integration support only with maintainer confirmation.
