import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { captureCurrentPiSession, capturePiSession } from "./session-capture.ts";
import { describePiSession } from "./session-descriptor.ts";

const sessionId = "019b2f48-7c51-7c23-ae42-334455667788";
const manager = (source: string) => ({
  getSessionId: () => sessionId,
  getSessionFile: () => source,
  getEntries: () => [{ id: "entry-1" }],
  getBranch: () => [{ id: "entry-1" }],
  getLeafId: () => "entry-1",
});

test("captures and refreshes validated synthetic Pi bytes at the relative archive key", async () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-capture-"));
  try {
    const source = join(root, "native.jsonl");
    const archive = join(root, "private-archive");
    const first = `{"type":"session","id":"${sessionId}"}\n{"type":"message","id":"entry-1","parentId":null,"text":"first"}\n`;
    writeFileSync(source, first);
    const { archivePath: target } = await captureCurrentPiSession(manager(source), archive);
    assert.equal(target, join(archive, "sessions", `${sessionId}.jsonl`));
    assert.equal(readFileSync(target!, "utf8"), first);

    const updated = `${first}{"type":"message","id":"entry-2","parentId":"entry-1","text":"continued"}\n`;
    writeFileSync(source, updated);
    const continuedManager = {
      ...manager(source),
      getEntries: () => [{ id: "entry-1" }, { id: "entry-2" }],
      getBranch: () => [{ id: "entry-1" }, { id: "entry-2" }],
      getLeafId: () => "entry-2",
    };
    assert.equal(await capturePiSession(await describePiSession(continuedManager), source, archive), target);
    assert.equal(readFileSync(target!, "utf8"), updated);

    const stale = await describePiSession(manager(source));
    await assert.rejects(capturePiSession(stale, source, archive), /does not match descriptor/);
    assert.equal(readFileSync(target!, "utf8"), updated);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("does not archive unavailable or mismatched session sources", async () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-capture-"));
  try {
    const source = join(root, "native.jsonl");
    writeFileSync(source, '{"type":"session","id":"other"}\n');
    const descriptor = await describePiSession(manager(source));
    assert.equal(descriptor.availability, "unavailable");
    assert.equal(await capturePiSession(descriptor, source, join(root, "archive")), null);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("rejects an archive key that is not the descriptor session path", async () => {
  const root = mkdtempSync(join(tmpdir(), "pi-session-capture-"));
  try {
    const source = join(root, "native.jsonl");
    writeFileSync(source, `{"type":"session","id":"${sessionId}"}\n`);
    const descriptor = await describePiSession(manager(source));
    await assert.rejects(
      capturePiSession({ ...descriptor, archive_key: "../../outside.jsonl" }, source, join(root, "archive")),
      /invalid Pi archive key/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
