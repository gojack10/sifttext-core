import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { captureCurrentPiSession } from "./session-capture.ts";
import { checkpointPiSession } from "./session-checkpoint.ts";
import type { PiSessionDescriptor, PiSessionManager } from "./session-descriptor.ts";

/** Captures the active Pi snapshot, then commits only that archive entry when available. */
export async function retainCurrentPiSession(
  sessionManager: PiSessionManager,
  archiveRoot: string,
): Promise<{ descriptor: PiSessionDescriptor; archivePath: string | null; commit: string | null }> {
  const captured = await captureCurrentPiSession(sessionManager, archiveRoot);
  const commit = captured.archivePath ? checkpointPiSession(captured.descriptor, archiveRoot) : null;
  return { ...captured, commit };
}

/** Attach the captured session through the Core CLI, after its Git checkpoint succeeds. */
export async function retainAndAttachPiSession(
  sessionManager: PiSessionManager,
  archiveRoot: string,
  sifttext: string,
  home: string,
  nodeId: string,
  topic: string,
): Promise<{ descriptor: PiSessionDescriptor; archivePath: string | null; commit: string | null; attachment: unknown }> {
  const retained = await retainCurrentPiSession(sessionManager, archiveRoot);
  const temp = mkdtempSync(join(tmpdir(), "sifttext-session-descriptor-"));
  try {
    const descriptorFile = join(temp, "descriptor.json");
    writeFileSync(descriptorFile, JSON.stringify(retained.descriptor), { mode: 0o600 });
    const output = execFileSync(sifttext, ["session-attach", home, nodeId, descriptorFile, topic, archiveRoot], { encoding: "utf8" });
    return { ...retained, attachment: JSON.parse(output) };
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}
