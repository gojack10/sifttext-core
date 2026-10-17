import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readlinkSync, realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { retainAndAttachPiSession } from "./session-retention.ts";

const bin = process.env.SIFTTEXT_E2E_BIN;
const sessionId = "019b2f48-7c51-7c23-ae42-334455667788";
const git = (root: string, ...args: string[]) => execFileSync("git", ["-C", root, ...args], { encoding: "utf8" }).trim();

test("real CLI captures, checkpoints, attaches, and restores a relocated synthetic Pi session", { skip: !bin }, async () => {
  const root = mkdtempSync(join(realpathSync(tmpdir()), "pi-session-e2e-"));
  try {
    const home = join(root, "home");
    const archive = join(root, "archive");
    const source = join(root, "native.jsonl");
    execFileSync(bin!, ["init", home, "--empty"], { stdio: "pipe" });
    const worktree = JSON.parse(execFileSync(bin!, ["worktree", home], { encoding: "utf8" })).worktree as string;
    const node = JSON.parse(execFileSync(bin!, ["create", worktree, "Evidence", "Synthetic session evidence"], { encoding: "utf8" })).id as string;

    execFileSync("git", ["init", "-q", archive]);
    git(archive, "config", "user.name", "Test");
    git(archive, "config", "user.email", "test@example.invalid");
    writeFileSync(join(archive, "README"), "initial\n");
    git(archive, "add", "README");
    git(archive, "commit", "-qm", "initial");

    const entry = { id: "entry-1", parentId: null, type: "message", message: { role: "user", content: "synthetic" } };
    const bytes = `${JSON.stringify({ type: "session", id: sessionId })}\n${JSON.stringify(entry)}\n`;
    writeFileSync(source, bytes);
    const manager = {
      getSessionId: () => sessionId,
      getSessionFile: () => source,
      getEntries: () => [entry],
      getBranch: () => [entry],
      getLeafId: () => entry.id,
    };

    const result = await retainAndAttachPiSession(manager, archive, bin!, worktree, node, "synthetic-session");
    assert.equal(result.attachment.status, "attached");
    assert.equal(result.archivePath, join(archive, "sessions", `${sessionId}.jsonl`));
    assert.equal(result.commit, git(archive, "rev-parse", "HEAD"));
    assert.equal(readlinkSync(join(worktree, "nodes", node, "artifacts", "chats", "pi-synthetic-session.jsonl")), result.archivePath);
    assert.equal(readFileSync(result.archivePath!, "utf8"), bytes);
    const sidecar = JSON.parse(readFileSync(join(worktree, "nodes", node, "session-sources.json"), "utf8"));
    assert.equal(sidecar[0].session_id, sessionId);
    assert.equal(sidecar[0].attachment_name, "pi-synthetic-session.jsonl");

    const relocated = join(root, "relocated-archive");
    renameSync(archive, relocated);
    const restored = JSON.parse(execFileSync(bin!, ["session-restore", worktree, relocated], { encoding: "utf8" }));
    assert.equal(restored.status, "restored");
    assert.equal(restored.links, 1);
    const link = join(worktree, "nodes", node, "artifacts", "chats", "pi-synthetic-session.jsonl");
    assert.equal(readlinkSync(link), join(relocated, "sessions", `${sessionId}.jsonl`));
    assert.equal(readFileSync(link, "utf8"), bytes);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
