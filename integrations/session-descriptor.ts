import { lstatSync, createReadStream } from "node:fs";
import { createInterface } from "node:readline";

export interface PiSessionDescriptor {
  schema_version: 1;
  harness: "pi";
  session_id: string;
  availability: "available" | "unavailable";
  archive_key: string | null;
  entry_ids: string[];
  branch_entry_ids: string[];
  leaf_id: string | null;
  cutoff_entry_id: string | null;
  unavailable_reason?: "invalid_session_id" | "no_persistent_session_file" | "source_file_unavailable" | "source_not_regular_file" | "source_invalid_header" | "source_identity_mismatch";
}

type Entry = { id: string };
export type PiSessionManager = {
  getSessionId(): string;
  getSessionFile(): string | undefined;
  getEntries(): readonly Entry[];
  getBranch(): readonly Entry[];
  getLeafId(): string | null;
};

export async function readPiSessionHeaderId(sessionFile: string): Promise<string | null> {
  const source = createReadStream(sessionFile, { encoding: "utf8" });
  const lines = createInterface({ input: source, crlfDelay: Infinity });
  try {
    for await (const line of lines) {
      let header: unknown;
      try {
        header = JSON.parse(line);
      } catch {
        return null;
      }
      return header && typeof header === "object" && "type" in header && header.type === "session" && "id" in header && typeof header.id === "string"
        ? header.id
        : null;
    }
    return null;
  } finally {
    lines.close();
    source.destroy();
  }
}

export async function describePiSession(sessionManager: PiSessionManager): Promise<PiSessionDescriptor> {
  const sessionId = sessionManager.getSessionId();
  const entries = sessionManager.getEntries().map(({ id }) => id);
  const branch = sessionManager.getBranch().map(({ id }) => id);
  const leafId = sessionManager.getLeafId();
  const base = {
    schema_version: 1 as const,
    harness: "pi" as const,
    session_id: sessionId,
    entry_ids: entries,
    branch_entry_ids: branch,
    leaf_id: leafId,
    cutoff_entry_id: entries.at(-1) ?? null,
  };
  const unavailable = (reason: NonNullable<PiSessionDescriptor["unavailable_reason"]>): PiSessionDescriptor => ({
    ...base,
    availability: "unavailable",
    archive_key: null,
    unavailable_reason: reason,
  });

  if (!/^[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?$/.test(sessionId)) {
    return unavailable("invalid_session_id");
  }
  const sessionFile = sessionManager.getSessionFile();
  if (!sessionFile) return unavailable("no_persistent_session_file");
  try {
    if (!lstatSync(sessionFile).isFile()) return unavailable("source_not_regular_file");
    const sourceId = await readPiSessionHeaderId(sessionFile);
    if (!sourceId) return unavailable("source_invalid_header");
    if (sourceId !== sessionId) return unavailable("source_identity_mismatch");
  } catch {
    return unavailable("source_file_unavailable");
  }

  return { ...base, availability: "available", archive_key: `sessions/${sessionId}.jsonl` };
}
