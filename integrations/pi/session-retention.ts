import { execFileSync } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { retainCurrentPiSession } from "../session-retention.ts";

export default function (pi: ExtensionAPI) {
  pi.on("agent_end", async (_event, ctx) => {
    const home = process.env.SIFTTEXT_HOME;
    if (!home) return;

    try {
      const config = JSON.parse(
        execFileSync(process.env.SIFTTEXT_BIN || "sifttext", ["session-retention-config", home], {
          encoding: "utf8",
          timeout: 5_000,
          maxBuffer: 4_096,
        }),
      ) as { enabled?: boolean; archive_root?: string | null };
      if (config.enabled !== true) return;
      if (typeof config.archive_root !== "string") throw new Error("missing archive root");
      await retainCurrentPiSession(ctx.sessionManager, config.archive_root);
    } catch {
      console.error("[sifttext] optional Pi session retention failed; knowledge work remains available");
    }
  });
}
