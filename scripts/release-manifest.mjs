import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';

const [version, directory] = process.argv.slice(2);
if (!/^v[0-9][a-zA-Z0-9._-]*$/.test(version ?? '')) throw Error('Invalid release version');
const info = JSON.parse(readFileSync(join(directory, 'release-info.json')));
if (info.version !== version || !/^[a-f0-9]{40}$/.test(info.source) || !/^[a-f0-9]{64}$/.test(info.knowledge)) throw Error('Invalid package identity');
const hashes = new Map();
function asset(name) {
  const sha256 = createHash('sha256').update(readFileSync(join(directory, name))).digest('hex');
  hashes.set(name, sha256);
  return {url: `https://github.com/gojack10/sifttext-core/releases/download/${version}/${name}`, sha256};
}
const manifest = {
  schema: 1,
  application: {
    version, source: info.source, format: info.format, requires_knowledge: info.knowledge,
    assets: Object.fromEntries(['darwin-arm64','darwin-x64','linux-arm64','linux-x64'].map(platform => [platform, asset(`sifttext-${platform}.tar.gz`)])),
  },
  knowledge: {revision: info.knowledge, format: info.format, requires_application: version, asset: asset('sifttext-knowledge.tar.gz')},
};
writeFileSync(join(directory, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
asset('manifest.json');
asset('install.sh');
asset('update-recovery.md');
asset('update-repairs-release.md');
writeFileSync(join(directory, 'SHA256SUMS'), `# SiftText release ${version} source ${info.source}\n` + [...hashes].map(([file, hash]) => `${hash}  ${file}\n`).join(''));
