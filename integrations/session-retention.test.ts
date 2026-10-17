import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { retainAndAttachPiSession, retainCurrentPiSession } from "./session-retention.ts";

const sessionId = "019b2f48-7c51-7c23-ae42-334455667788";
const git = (root: string, ...args: string[]) => execFileSync("git", ["-C", root, ...args], { encoding: "utf8" }).trim();

test("captures and checkpoints the current Pi session in one call", async () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-retention-"));
  try {
    const archive = join(root, "archive");
    const source = join(root, "native.jsonl");
    const bytes = `{"type":"session","id":"${sessionId}"}\n`;
    writeFileSync(source, bytes);
    execFileSync("git", ["init", "-q", archive]);
    git(archive, "config", "user.name", "Test");
    git(archive, "config", "user.email", "test@example.invalid");
    writeFileSync(join(archive, "README"), "initial\n");
    git(archive, "add", "README");
    git(archive, "commit", "-qm", "initial");

    const result = await retainCurrentPiSession({
      getSessionId: () => sessionId,
      getSessionFile: () => source,
      getEntries: () => [],
      getBranch: () => [],
      getLeafId: () => null,
    }, archive);

    assert.equal(result.archivePath, join(archive, "sessions", `${sessionId}.jsonl`));
    assert.equal(result.commit, git(archive, "rev-parse", "HEAD"));
    assert.equal(readFileSync(result.archivePath!, "utf8"), bytes);
    assert.equal(git(archive, "show", "--format=", "--name-only", "HEAD"), `sessions/${sessionId}.jsonl`);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("attaches only after archive capture and checkpoint via the CLI contract", async () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-retention-"));
  try {
    const archive = join(root, "archive");
    const source = join(root, "native.jsonl");
    const argsFile = join(root, "args.json");
    const cli = join(root, "sifttext-stub");
    writeFileSync(source, `{"type":"session","id":"${sessionId}"}\n`);
    execFileSync("git", ["init", "-q", archive]);
    git(archive, "config", "user.name", "Test");
    git(archive, "config", "user.email", "test@example.invalid");
    writeFileSync(join(archive, "README"), "initial\n");
    git(archive, "add", "README");
    git(archive, "commit", "-qm", "initial");
    writeFileSync(cli, `#!/usr/bin/env node\nconst fs = require("node:fs"); const args = process.argv.slice(2); fs.writeFileSync(${JSON.stringify(argsFile)}, JSON.stringify({args, descriptor: JSON.parse(fs.readFileSync(args[3], "utf8"))})); process.stdout.write(JSON.stringify({status:"attached"}));\n`);
    chmodSync(cli, 0o755);

    const result = await retainAndAttachPiSession({
      getSessionId: () => sessionId,
      getSessionFile: () => source,
      getEntries: () => [],
      getBranch: () => [],
      getLeafId: () => null,
    }, archive, cli, "/home", "node-id", "topic");

    assert.deepEqual(result.attachment, { status: "attached" });
    const { args, descriptor } = JSON.parse(readFileSync(argsFile, "utf8"));
    assert.deepEqual(args.slice(0, 3), ["session-attach", "/home", "node-id"]);
    assert.equal(descriptor.session_id, sessionId);
    assert.deepEqual(args.slice(4), ["topic", archive]);
    assert.equal(result.commit, git(archive, "rev-parse", "HEAD"));
    assert.equal(readFileSync(join(archive, "sessions", `${sessionId}.jsonl`), "utf8"), readFileSync(source, "utf8"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("records unavailable Pi source metadata without capture or Git checkpoint", async () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-retention-"));
  try {
    const argsFile = join(root, "args.json");
    const cli = join(root, "sifttext-stub");
    writeFileSync(cli, `#!/usr/bin/env node\nconst fs = require("node:fs"); const args = process.argv.slice(2); fs.writeFileSync(${JSON.stringify(argsFile)}, JSON.stringify({args, descriptor: JSON.parse(fs.readFileSync(args[3], "utf8"))})); process.stdout.write(JSON.stringify({status:"recorded-unavailable"}));\n`);
    chmodSync(cli, 0o755);

    const result = await retainAndAttachPiSession({
      getSessionId: () => sessionId,
      getSessionFile: () => join(root, "missing-session.jsonl"),
      getEntries: () => [],
      getBranch: () => [],
      getLeafId: () => null,
    }, join(root, "no-archive"), cli, "/home", "node-id", "topic");

    const { args, descriptor } = JSON.parse(readFileSync(argsFile, "utf8"));
    assert.equal(result.archivePath, null);
    assert.equal(result.commit, null);
    assert.deepEqual(result.attachment, { status: "recorded-unavailable" });
    assert.deepEqual(args.slice(0, 3), ["session-attach", "/home", "node-id"]);
    assert.deepEqual(args.slice(4), ["topic", join(root, "no-archive")]);
    assert.equal(descriptor.availability, "unavailable");
    assert.equal(descriptor.archive_key, null);
    assert.equal(descriptor.unavailable_reason, "source_file_unavailable");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
