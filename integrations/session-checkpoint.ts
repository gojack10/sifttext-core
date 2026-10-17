import { execFileSync } from "node:child_process";
import { lstatSync, realpathSync } from "node:fs";
import { isAbsolute, relative, resolve, sep } from "node:path";
import type { PiSessionDescriptor } from "./session-descriptor.ts";

/** Commit one validated archived session without including unrelated staged files. */
export function checkpointPiSession(descriptor: PiSessionDescriptor, archiveRoot: string): string | null {
  if (descriptor.availability !== "available" || !descriptor.archive_key) return null;
  if (descriptor.archive_key !== `sessions/${descriptor.session_id}.jsonl`) throw new Error("invalid Pi archive key");

  const root = realpathSync(resolve(archiveRoot));
  const file = resolve(root, descriptor.archive_key);
  const path = relative(root, file);
  if (isAbsolute(path) || path === ".." || path.startsWith(`..${sep}`)) throw new Error("Pi archive key escapes archive root");
  if (!lstatSync(file).isFile()) throw new Error("Pi archive is not a regular file");
  const git = (args: string[]) => execFileSync("git", ["-C", root, ...args], { encoding: "utf8" }).trim();
  if (git(["rev-parse", "--show-toplevel"]) !== root) throw new Error("Pi archive root must be a Git worktree");

  git(["add", "--", descriptor.archive_key]);
  let hasHead = true;
  try {
    git(["rev-parse", "--verify", "--quiet", "HEAD"]);
  } catch (error) {
    if ((error as { status?: number }).status !== 1) throw error;
    hasHead = false;
  }
  if (hasHead) {
    try {
      execFileSync("git", ["-C", root, "diff", "--cached", "--quiet", "HEAD", "--", descriptor.archive_key], { stdio: "ignore" });
      return git(["rev-parse", "HEAD"]);
    } catch (error) {
      if ((error as { status?: number }).status !== 1) throw error;
    }
  }
  git(["commit", "--only", "-m", `session: capture ${descriptor.session_id}`, "--", descriptor.archive_key]);
  return git(["rev-parse", "HEAD"]);
}
