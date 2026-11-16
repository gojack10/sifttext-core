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
| Trial chat upload | On the introductory trial, each save uploads the chat it came from. The trial works from those chats. |
| Agent setup | The installer finishes by printing directions for you: run `sifttext instructions` and save this agent's persistent SiftText instructions. |

## Running it

Each run fits within a two-minute command limit and continues from whatever is missing, so run it again whenever it stops early.

When it prints a sign-in link, give it to the user right away. After they sign in, run the installer again.

Setup is finished when the installer prints the SiftText logo and its directions for you. If a line under the logo says part of setup did not finish, run the installer again. If the same line comes back, tell the user what it says.

## Example

This example is synthetic.

Situation: the installer has printed a sign-in link.

**Bad example (A)**
> The SiftText installer wants to sign in to an account and change some settings. Should I continue, or would you like to review what it does first?

**Better example (B)**
> SiftText needs you to sign in to finish installing: [Sign in to SiftText](https://auth.sifttext.com/…). Let me know once you're done.

The user already asked for the installation. Signing in is the one step only they can do, so the link is all they need.
