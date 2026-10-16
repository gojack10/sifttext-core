import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync, symlinkSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';

const installer = new URL('../install.sh', import.meta.url).pathname;
test('bootstrap verifies one package, preserves repeat installs and refuses damaged downloads', () => {
  const root=mkdtempSync(join(tmpdir(),'sifttext-bootstrap-'));
  try {
    const fixture=join(root,'fixture'), commands=join(root,'commands');
    mkdirSync(fixture);mkdirSync(commands);mkdirSync(join(fixture,'package'));
    writeFileSync(join(fixture,'package','sifttext'),`#!/bin/sh
set -eu
case "$1" in
 runtime-check) [ "\${FAIL_RUNTIME:-0}" != 1 ] ;;
 install-home) if [ ! -f "$2/state.json" ]; then printf 'fixture\\n' > "$2/state.json"; mkdir -p "$3"; fi ;;
 instructions) printf 'fixture instructions\\n' ;;
esac
`,{mode:0o755});
    assert.equal(spawnSync('tar',['-czf',join(fixture,'archive.tar.gz'),'-C',fixture,'package']).status,0);
    const platform=process.platform==='darwin'?'darwin':'linux';
    const cpu=process.arch==='arm64'?'arm64':'x64';
    const hash=createHash('sha256').update(readFileSync(join(fixture,'archive.tar.gz'))).digest('hex');
    writeFileSync(join(fixture,'SHA256SUMS'),`# SiftText release v0.1.0 source fixture\n${hash}  sifttext-${platform}-${cpu}.tar.gz\n`);
    writeFileSync(join(commands,'curl'),`#!/bin/sh
set -eu
out=; url=
while [ "$#" -gt 0 ]; do
 case "$1" in -o|--output) out=$2; shift 2 ;; https://*) url=$1; shift ;; *) shift ;; esac
done
printf '%s\\n' "$url" >> "$FIXTURE/requests"
case "$url" in
 */SHA256SUMS) cp "$FIXTURE/SHA256SUMS" "$out" ;;
 *.tar.gz) if [ "\${BAD_DOWNLOAD:-0}" = 1 ]; then printf damaged > "$out"; else cp "$FIXTURE/archive.tar.gz" "$out"; fi ;;
 *) exit 25 ;;
esac
`,{mode:0o755});
    const env={...process.env,PATH:commands+':'+process.env.PATH,FIXTURE:fixture};
    const prefix=join(root,'application'), home=join(root,'knowledge');
    mkdirSync(home);writeFileSync(join(home,'customer.txt'),'preserve');
    const run=(path=prefix,more={})=>spawnSync('sh',[installer,path,home],{env:{...env,...more},encoding:'utf8'});
    let result=run();assert.equal(result.status,0,result.stderr);
    assert.equal(readFileSync(join(fixture,'requests'),'utf8').trim().split('\n').length,2);
    assert.equal(readFileSync(join(home,'customer.txt'),'utf8'),'preserve');
    result=run();assert.equal(result.status,0,result.stderr);
    assert.equal(readFileSync(join(fixture,'requests'),'utf8').trim().split('\n').length,2);
    const broken=join(root,'damaged');result=run(broken,{BAD_DOWNLOAD:'1'});assert.notEqual(result.status,0);assert.ok(!existsSync(broken));
    const refused=join(root,'refused');result=run(refused,{FAIL_RUNTIME:'1'});assert.notEqual(result.status,0);assert.ok(!existsSync(refused));
    const alias=join(root,'alias');symlinkSync(home,alias);result=run(alias);assert.notEqual(result.status,0);
    const overlap=join(home,'application');result=run(overlap);assert.notEqual(result.status,0);assert.ok(!existsSync(overlap));
    writeFileSync(join(commands,'git'),'#!/bin/sh\nexit 1\n',{mode:0o755});
    const missing=join(root,'missing-git');result=run(missing);assert.notEqual(result.status,0);assert.ok(!existsSync(missing));
    assert.equal(readFileSync(join(home,'customer.txt'),'utf8'),'preserve');
  } finally {rmSync(root,{recursive:true,force:true});}
});
