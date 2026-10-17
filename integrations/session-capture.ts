import { copyFileSync, mkdirSync, renameSync, rmSync, lstatSync, createReadStream } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, resolve, sep } from "node:path";
import { describePiSession, type PiSessionDescriptor, type PiSessionManager } from "./session-descriptor.ts";

async function matchesSnapshot(file: string, descriptor: PiSessionDescriptor): Promise<boolean> {
  const stream = createReadStream(file, { encoding: "utf8" });
  const lines = createInterface({ input: stream, crlfDelay: Infinity });
  const ids: string[] = [];
  const parents = new Map<string, string | null>();
  try {
    let first = true;
    for await (const line of lines) {
      let record: unknown;
      try {
        record = JSON.parse(line);
      } catch {
        return false;
      }
      if (!record || typeof record !== "object") return false;
      if (first) {
        first = false;
        if ((record as { type?: unknown }).type !== "session" || (record as { id?: unknown }).id !== descriptor.session_id) return false;
        continue;
      }
      const { id, parentId } = record as { id?: unknown; parentId?: unknown };
      if (typeof id !== "string" || (parentId !== null && typeof parentId !== "string") || parents.has(id)) return false;
      ids.push(id);
      parents.set(id, parentId as string | null);
    }
    if (first || ids.length !== descriptor.entry_ids.length || ids.some((id, i) => id !== descriptor.entry_ids[i])) return false;
    if ((ids.at(-1) ?? null) !== descriptor.cutoff_entry_id) return false;
    if (descriptor.leaf_id !== null && !parents.has(descriptor.leaf_id)) return false;

    const branch: string[] = [];
    let current = descriptor.leaf_id;
    const seen = new Set<string>();
    while (current !== null) {
      if (!parents.has(current) || seen.has(current)) return false;
      seen.add(current);
      branch.push(current);
      current = parents.get(current)!;
    }
    branch.reverse();
    return branch.length === descriptor.branch_entry_ids.length && branch.every((id, i) => id === descriptor.branch_entry_ids[i]);
  } finally {
    lines.close();
    stream.destroy();
  }
}

/** Copies a descriptor-matching Pi snapshot to the private archive, preserving any prior copy on refusal. */
export async function capturePiSession(
  descriptor: PiSessionDescriptor,
  sourceFile: string,
  archiveRoot: string,
): Promise<string | null> {
  if (descriptor.availability !== "available" || !descriptor.archive_key) return null;
  const expectedKey = `sessions/${descriptor.session_id}.jsonl`;
  if (descriptor.archive_key !== expectedKey) throw new Error("invalid Pi archive key");
  if (!lstatSync(sourceFile).isFile()) throw new Error("Pi session source is not a regular file");

  const root = resolve(archiveRoot);
  const target = resolve(root, descriptor.archive_key);
  if (!target.startsWith(`${root}${sep}`)) throw new Error("Pi archive key escapes archive root");
  mkdirSync(dirname(target), { recursive: true });
  const temporary = `${target}.${process.pid}.${Date.now()}.tmp`;
  try {
    copyFileSync(sourceFile, temporary);
    if (!await matchesSnapshot(temporary, descriptor)) throw new Error("Pi source snapshot does not match descriptor");
    renameSync(temporary, target);
    return target;
  } finally {
    rmSync(temporary, { force: true });
  }
}

/** Describes the active Pi session and archives its exact persisted snapshot when available. */
export async function captureCurrentPiSession(
  sessionManager: PiSessionManager,
  archiveRoot: string,
): Promise<{ descriptor: PiSessionDescriptor; archivePath: string | null }> {
  const descriptor = await describePiSession(sessionManager);
  const sourceFile = sessionManager.getSessionFile();
  const archivePath = descriptor.availability === "available" && sourceFile
    ? await capturePiSession(descriptor, sourceFile, archiveRoot)
    : null;
  return { descriptor, archivePath };
}
