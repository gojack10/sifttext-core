import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { describePiSession } from "./session-descriptor.ts";

const sessionId = "019b2f48-7c51-7c23-ae42-334455667788";
const entries = [{ id: "root" }, { id: "old-branch" }, { id: "active-leaf" }];
const manager = (sessionFile?: string) => ({
  getSessionId: () => sessionId,
  getSessionFile: () => sessionFile,
  getEntries: () => entries,
  getBranch: () => [entries[0], entries[2]],
  getLeafId: () => "active-leaf",
});

test("describes persisted Pi identity, full entry order, active branch, and cutoff without host paths", async () => {
  const dir = mkdtempSync(join(tmpdir(), "pi-session-descriptor-"));
  try {
    const file = join(dir, "source.jsonl");
    writeFileSync(file, `{"type":"session","id":"${sessionId}"}\n`);
    assert.deepEqual(await describePiSession(manager(file)), {
      schema_version: 1,
      harness: "pi",
      session_id: sessionId,
      availability: "available",
      archive_key: `sessions/${sessionId}.jsonl`,
      entry_ids: ["root", "old-branch", "active-leaf"],
      branch_entry_ids: ["root", "active-leaf"],
      leaf_id: "active-leaf",
      cutoff_entry_id: "active-leaf",
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("reports a missing persistent source explicitly while preserving session metadata", async () => {
  assert.deepEqual(await describePiSession(manager()), {
    schema_version: 1,
    harness: "pi",
    session_id: sessionId,
    availability: "unavailable",
    archive_key: null,
    entry_ids: ["root", "old-branch", "active-leaf"],
    branch_entry_ids: ["root", "active-leaf"],
    leaf_id: "active-leaf",
    cutoff_entry_id: "active-leaf",
    unavailable_reason: "no_persistent_session_file",
  });
});

test("refuses session IDs that could not safely form an archive key", async () => {
  const descriptor = await describePiSession({ ...manager(), getSessionId: () => "../outside" });
  assert.equal(descriptor.availability, "unavailable");
  assert.equal(descriptor.unavailable_reason, "invalid_session_id");
});

test("refuses a regular file whose session header belongs to another session", async () => {
  const dir = mkdtempSync(join(tmpdir(), "pi-session-descriptor-"));
  try {
    const file = join(dir, "source.jsonl");
    writeFileSync(file, '{"type":"session","id":"different-session"}\n');
    const descriptor = await describePiSession(manager(file));
    assert.equal(descriptor.availability, "unavailable");
    assert.equal(descriptor.unavailable_reason, "source_identity_mismatch");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test("refuses a file without a valid Pi session header", async () => {
  const dir = mkdtempSync(join(tmpdir(), "pi-session-descriptor-"));
  try {
    const file = join(dir, "source.jsonl");
    writeFileSync(file, '{"type":"entry","id":"not-a-header"}\n');
    const descriptor = await describePiSession(manager(file));
    assert.equal(descriptor.availability, "unavailable");
    assert.equal(descriptor.unavailable_reason, "source_invalid_header");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
