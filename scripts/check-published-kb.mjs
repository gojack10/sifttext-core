#!/usr/bin/env node
// Publication policy, not a customer-store validator. Read exact Git objects.
import { execFileSync } from 'node:child_process';

const git = (...args) => execFileSync('git', args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
const tree = process.argv[2] ?? git('write-tree').trim();
const entries = git('ls-tree', '-rz', '--full-tree', tree, '--', 'nodes').split('\0').filter(Boolean);
let count = 0;
let failed = false;
for (const entry of entries) {
  const [header, path] = entry.split('\t');
  if (!/^nodes\/[^/]+\/node\.md$/.test(path)) continue;
  count++;
  const [mode, type, oid] = header.split(' ');
  const text = git('cat-file', 'blob', oid);
  const frontmatter = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/.exec(text)?.[1];
  const statuses = frontmatter?.split(/\r?\n/).filter(line => /^status\s*:/.test(line)) ?? [];
  let status;
  try { status = JSON.parse(statuses[0]?.slice(statuses[0].indexOf(':') + 1)); } catch {}
  if (mode !== '100644' || type !== 'blob' || statuses.length !== 1 || status !== 'resolved') {
    console.error(`${path}: published instructions require exactly one status: "resolved"`);
    failed = true;
  }
}
if (!count) {
  console.error('No published KB nodes found in the selected Git tree.');
  failed = true;
}
process.exitCode = failed ? 1 : 0;
