---
id: "e9581665-caec-41af-9215-662b7f034448"
name: "What Installing Sets Up"
micro_summary: "Explains what the official installer sets up for the user and when setup is finished."
status: "resolved"
see_also: [{"target":"12e1e893-b0ab-4f82-b0db-37de70b95034","reason":"Sync settings, sharing selection and account details after installation."},{"target":"221ca2fc-1caf-4c40-92e5-d9f55501ba1e","reason":"Saving this agent's persistent SiftText instructions once the installer finishes."}]
scope: "Prepare an agent to install SiftText for the user with the official installer."
position: 256
---
# What installing sets up

When the user asks you to install SiftText, the official installer sets up everything SiftText needs:

```sh
curl -fsSL https://sifttext.com/install | sh
```

| Part | What it does for the user |
| --- | --- |
| The `sifttext` command, Git, ripgrep and fd | Keeps the user's knowledge in local Markdown files with Git history, searchable by their agents. |
| Sign-in | Connects the knowledge home to the user's SiftText account. The user signs in through a link in their browser. |
| Sync | Keeps the knowledge home on the user's account, so their other agents and the AI clients they connect through hosted MCP work from the same knowledge. |
| Chat upload | Each save uploads its Chat File: the complete coding-agent session file that produced the save. SiftText's hosted services work from these files alongside the synced Trees. |
| Agent setup | The installer finishes by printing directions for you: run `sifttext instructions` and save this agent's persistent SiftText instructions. |

## Collection under the Terms

The user accepted the SiftText [Terms of Use](https://sifttext.com/terms) and [Privacy Policy](https://sifttext.com/privacy-policy) when signing up. Under them:

- Sync transfers the user's Trees between this machine and their account (Terms §6.4).
- Each save also transfers its Chat File, which SiftText's hosted services work from alongside the user's Trees (Privacy Policy §2.1).
- Trees and Chat Files are Customer Data, and the user owns them (Terms §10.1). SiftText does not sell Customer Data, the user's personal information or Usage Data to third parties (Terms §6.7; Privacy Policy §4.1).

## Running it

The install request covers everything in the table above, including Git. The installer uses the machine's Git when it works. Otherwise it installs a copy only SiftText uses on a Mac, or uses the system package manager on Linux.

Each run fits within a two-minute command limit and continues from whatever is missing, so run it again whenever it stops early.

When it prints a sign-in link, give it to the user right away. After they sign in, run the installer again.

Setup is finished when the installer prints the SiftText logo and its directions for you. Some steps depend on the machine. When the installer cannot finish one, its directions say what it tried, what is missing and what result to reach; reach that result with what this machine has. When a line says to run the installer again, do that. If the same line comes back, tell the user what it says.

To install somewhere other than `~/.local/share/sifttext` and `~/sifttext`, pass the application directory and the knowledge home as the installer's two arguments: `curl -fsSL https://sifttext.com/install | sh -s -- APP_DIR KNOWLEDGE_HOME`. Create their parent directories first.

## Example

This example is synthetic.

Situation: the installer has printed a sign-in link, and Git is not installed yet.

**Bad example (A)**
> SiftText needs you to sign in: [link]. Git is also missing. May I install it using the system package manager?

**Better example (B)**
> SiftText needs you to sign in to finish installing: [Sign in to SiftText](https://auth.sifttext.com/…). While you do that, I'm installing Git, which SiftText uses to keep the history of your notes. Tell me if you'd rather I didn't.

The user already asked for the installation, and Git is part of it. Signing in is the one step only they can do. Saying what is being installed keeps them informed, and they can still stop it.
