import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { capturePiSession } from "./session-capture.ts";
import { checkpointPiSession } from "./session-checkpoint.ts";
import type { PiSessionDescriptor } from "./session-descriptor.ts";

const sessionId = "019b2f48-7c51-7c23-ae42-334455667788";
const descriptor: PiSessionDescriptor = {
  schema_version: 1,
  harness: "pi",
  session_id: sessionId,
  availability: "available",
  archive_key: `sessions/${sessionId}.jsonl`,
  entry_ids: [],
  branch_entry_ids: [],
  leaf_id: null,
  cutoff_entry_id: null,
};

const initRepo = (root: string) => {
  const git = (...args: string[]) => execFileSync("git", ["-C", root, ...args], { encoding: "utf8" }).trim();
  git("init", "-q");
  git("config", "user.name", "Test");
  git("config", "user.email", "test@example.invalid");
  return git;
};

test("checkpoints only the captured session, preserves other staged work, and skips no-op commits", () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-checkpoint-"));
  const git = initRepo(root);
  try {
    mkdirSync(join(root, "sessions"));
    writeFileSync(join(root, descriptor.archive_key!), "snapshot 1\n");
    writeFileSync(join(root, "unrelated.txt"), "base\n");
    git("add", ".");
    git("commit", "-qm", "initial");

    writeFileSync(join(root, descriptor.archive_key!), "snapshot 2\n");
    writeFileSync(join(root, "unrelated.txt"), "later unrelated edit\n");
    git("add", "unrelated.txt");
    const commit = checkpointPiSession(descriptor, root);
    assert.ok(commit);
    assert.equal(readFileSync(join(root, descriptor.archive_key!), "utf8"), "snapshot 2\n");
    assert.equal(git("show", "--format=", "--name-only", "HEAD"), descriptor.archive_key);
    assert.equal(git("status", "--porcelain"), "M  unrelated.txt");
    assert.equal(checkpointPiSession(descriptor, root), commit);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("commits the output of descriptor-validated capture", async () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-checkpoint-"));
  const sourceRoot = mkdtempSync(join(tmpdir(), "pi-session-native-"));
  const git = initRepo(root);
  try {
    mkdirSync(join(root, "sessions"));
    writeFileSync(join(root, descriptor.archive_key!), "old snapshot\n");
    git("add", descriptor.archive_key!);
    git("commit", "-qm", "initial snapshot");

    const source = join(sourceRoot, "session.jsonl");
    writeFileSync(source, `{"type":"session","id":"${sessionId}"}\n`);
    const archivePath = await capturePiSession(descriptor, source, root);
    assert.equal(archivePath, join(root, descriptor.archive_key!));
    const commit = checkpointPiSession(descriptor, root);
    assert.ok(commit);
    assert.equal(readFileSync(archivePath!, "utf8"), readFileSync(source, "utf8"));
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(sourceRoot, { recursive: true, force: true });
  }
});

test("refuses an archive key that does not match the session identity", () => {
  assert.throws(() => checkpointPiSession({ ...descriptor, archive_key: "sessions/other.jsonl" }, tmpdir()), /invalid Pi archive key/);
});

test("requires the archive root itself to be a Git worktree", () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-checkpoint-root-"));
  try {
    execFileSync("git", ["init", "-q", root]);
    mkdirSync(join(root, "sessions", "sessions"), { recursive: true });
    writeFileSync(join(root, "sessions", descriptor.archive_key!), "snapshot\n");
    assert.throws(() => checkpointPiSession(descriptor, join(root, "sessions")), /archive root must be a Git worktree/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
