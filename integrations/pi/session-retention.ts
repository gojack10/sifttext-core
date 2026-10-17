import { execFileSync } from "node:child_process";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { retainAndAttachPiSession, retainCurrentPiSession } from "../session-retention.ts";

export default function (pi: ExtensionAPI) {
  pi.registerTool({
    name: "sifttext_attach_session",
    label: "Attach Pi session",
    description: "When the current Pi session materially informed a specific knowledge node, attach it only to the explicitly selected node. Requires opt-in retention and a private knowledge worktree; never attach to every node or expose transcript content.",
    parameters: Type.Object({
      worktree: Type.String({ description: "Absolute path to the authorized private knowledge worktree, not the accepted home" }),
      nodeId: Type.String({ description: "Exact UUID of the node this session informed" }),
      topic: Type.String({ description: "Short kebab-case topic for the readable attachment name" }),
    }),
    async execute(_id, params, _signal, _onUpdate, ctx) {
      const home = process.env.SIFTTEXT_HOME;
      if (!home) return { content: [{ type: "text", text: "SIFTTEXT_HOME is not set" }], details: undefined, isError: true };
      try {
        const config = JSON.parse(execFileSync(process.env.SIFTTEXT_BIN || "sifttext", ["session-retention-config", home], { encoding: "utf8", timeout: 5_000, maxBuffer: 4_096 })) as { enabled?: boolean; archive_root?: string | null };
        if (config.enabled !== true || typeof config.archive_root !== "string") {
          return { content: [{ type: "text", text: "Session retention is not enabled; no session was attached" }], details: undefined, isError: true };
        }
        const result = await retainAndAttachPiSession(
          ctx.sessionManager,
          config.archive_root,
          process.env.SIFTTEXT_BIN || "sifttext",
          params.worktree,
          params.nodeId,
          params.topic,
        );
        const attachment = result.attachment as { status: string; attachment_name: string };
        return {
          content: [{ type: "text", text: JSON.stringify({ status: attachment.status, nodeId: params.nodeId, attachment_name: attachment.attachment_name }) }],
          details: undefined,
        };
      } catch {
        return { content: [{ type: "text", text: "Session capture or attachment failed; ordinary knowledge work remains available" }], details: undefined, isError: true };
      }
    },
  });

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
