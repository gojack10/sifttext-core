#!/usr/bin/env node
// Structural publication policy, not an epistemic verdict on maintained docs.
import { execFileSync } from 'node:child_process';

const git = (...args) => execFileSync('git', args, { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
const tree = process.argv[2] ?? git('write-tree').trim();
const entries = git('ls-tree', '-rz', '--full-tree', tree, '--', 'nodes').split('\0').filter(Boolean);
let count = 0;
let failed = false;
for (const entry of entries) {
  const [header, path] = entry.split('\t');
  const match = /^nodes\/([^/]+)\/node\.md$/.exec(path);
  if (!match) continue;
  count++;
  const [mode, type, oid] = header.split(' ');
  const text = git('cat-file', 'blob', oid);
  const frontmatter = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/.exec(text)?.[1];
  const field = key => {
    const lines = frontmatter?.split(/\r?\n/).filter(line => line.startsWith(`${key}:`)) ?? [];
    if (lines.length !== 1) return undefined;
    try { return JSON.parse(lines[0].slice(key.length + 1)); } catch { return undefined; }
  };
  if (mode !== '100644' || type !== 'blob' || field('id') !== match[1]
      || !/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(match[1])
      || ['name', 'micro_summary'].some(key => typeof field(key) !== 'string' || !field(key).trim())
      || !['not_examined', 'in_progress', 'stuck', 'deferred', 'discarded', 'resolved'].includes(field('status'))) {
    console.error(`${path}: published documentation requires valid native identity and metadata`);
    failed = true;
  }
}
if (!count) {
  console.error('No published KB nodes found in the selected Git tree.');
  failed = true;
}
process.exitCode = failed ? 1 : 0;
