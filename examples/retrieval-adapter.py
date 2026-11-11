"""Read-only presentation adapter for the retrieval experiment.

The native custom CLI ranks single or multiple phrasings. This adapter keeps the
previous experiment's three-body trolley and producer-independent stdin outline.
Set SIFTTEXT_RETRIEVAL_ROOT, SIFTTEXT_RETRIEVAL_CLI and SIFTTEXT_RETRIEVAL_ENCODER;
pass the run label, then ordinary CLI arguments. Run with uv and PyYAML.
"""
import csv
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import yaml

ROOT = Path(os.environ['SIFTTEXT_RETRIEVAL_ROOT']).resolve()
CORPUS = ROOT / 'corpus'
NATIVE = Path(os.environ['SIFTTEXT_RETRIEVAL_CLI']).resolve()
ENCODER = Path(os.environ['SIFTTEXT_RETRIEVAL_ENCODER']).resolve()
UUID = re.compile(r'[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}', re.I)


def source(node_id):
    path = CORPUS / 'nodes' / node_id / 'node.md'
    text = path.read_text()
    _, front, body = text.split('---', 2)
    return yaml.safe_load(front), body.lstrip('\r\n')


def card(node_id):
    front, _ = source(node_id)
    out = {key: front[key] for key in ('id', 'name', 'status', 'micro_summary')}
    for key in ('warnings', 'blocker', 'deferred_reason', 'discard_reason'):
        if front.get(key):
            out[key] = front[key]
    return out


def ancestors(node_id):
    chain = []
    path = CORPUS / 'nodes' / node_id / 'parent'
    while path.is_symlink():
        node_id = Path(os.readlink(path)).name
        chain.append(node_id)
        path = CORPUS / 'nodes' / node_id / 'parent'
    return list(reversed(chain))


def children(node_id):
    directory = CORPUS / 'nodes' / node_id / 'children'
    if not directory.is_dir():
        return []
    entries = [p.name for p in directory.iterdir() if p.is_symlink()]
    return sorted(entries, key=lambda i: (source(i)[0].get('position', 0), i))


def render(document):
    return subprocess.run(
        [str(ENCODER)], input=json.dumps(document, ensure_ascii=False),
        text=True, stdout=subprocess.PIPE, check=True,
    ).stdout.rstrip('\n')


def native(args):
    env = dict(os.environ, SIFTTEXT_NO_UPDATE='1')
    env.pop('SIFTTEXT_INSTALL_ROOT', None)
    return subprocess.run([str(NATIVE), *args], text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, env=env)


def unescape(value):
    if value.startswith('"') and value.endswith('"'):
        value = value[1:-1].replace('""', '"')
        value = re.sub(r'\\(n|\\)', lambda m: '\n' if m[1] == 'n' else '\\', value)
    return value


def columns(line):
    # Tabs in quoted LEAN cells are literal; CSV has the same doubled-quote rule.
    return next(csv.reader(io.StringIO(line), delimiter='\t', quotechar='"'))


def parse_search(text):
    metadata, rows, fields = {}, [], None
    for line in text.splitlines():
        if line.startswith('rows['):
            fields = line.split(':', 1)[1].split('\t')
        elif line.startswith('  ') and fields:
            values = columns(line[2:])
            row = {}
            for key, value in zip(fields, values):
                if key == '~':
                    break
                row[key] = unescape(value)
            if '~' in fields:
                for value in values[fields.index('~'):]:
                    key, cell = value.split(':', 1)
                    row[key] = unescape(cell)
            rows.append(row)
        elif ':' in line:
            key, value = line.split(':', 1)
            metadata[key] = unescape(value)
    if metadata.get('status') != 'current':
        raise ValueError('native search did not return a current result')
    return metadata, rows


def search_packet(args):
    result = native(args)
    if result.returncode:
        sys.stderr.write(result.stderr)
        sys.stdout.write(result.stdout)
        raise SystemExit(result.returncode)
    meta, native_rows = parse_search(result.stdout)
    rows, notices = [], {}
    for position, hit in enumerate(native_rows):
        node_id = hit['id']
        front, body = source(node_id)
        row = card(node_id)
        chain = ancestors(node_id)
        row['trail'] = ' > '.join(source(i)[0]['name'] for i in chain)
        for ancestor_id in chain:
            ancestor, _ = source(ancestor_id)
            if ancestor.get('warnings'):
                notices[ancestor_id] = f"{ancestor['name']}: {ancestor['warnings']}"
        if position < 3:
            row['content'] = body
            if front.get('scope'):
                row['scope'] = front['scope']
            if front.get('see_also'):
                row['see_also'] = '\n'.join(
                    f"[{source(link['target'])[0]['name']}](../{link['target']}/node.md): {link['reason']}"
                    for link in front['see_also']
                )
        elif hit.get('matching_excerpt'):
            row['matching_excerpt'] = hit['matching_excerpt']
        rows.append(row)
    child_ids = children(rows[0]['id']) if rows else []
    present = {row['id'] for row in rows}
    for row in rows:
        if row['id'] in child_ids:
            row['child_of_first'] = True
    packet = {'total_matches': int(meta.get('total_matches', len(rows))), 'rows': rows}
    if notices:
        packet['warnings'] = '\n'.join(notices.values())
    text = render(packet)
    missing = [card(i) for i in child_ids if i not in present]
    if missing:
        text += '\n' + render({'rows': missing}).replace('rows[', 'children_of_first[', 1)
    return text + '\n', {'hit_ids': [r['id'] for r in rows], 'full_ids': [r['id'] for r in rows[:3]]}


def piped_outline():
    selected, seen = [], set()
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        # Path records, rg path:line:hit records, and bare UUIDs are accepted.
        location = line.split(':', 1)[0]
        matches = UUID.findall(location)
        if not matches:
            raise ValueError(f'input is not a node ID or path: {line}')
        node_id = matches[-1].lower()
        if not (CORPUS / 'nodes' / node_id / 'node.md').is_file():
            raise ValueError(f'node not found in corpus: {node_id}')
        if node_id not in seen:
            seen.add(node_id)
            selected.append(card(node_id))
    return render({'rows': selected}) + '\n', {'hit_ids': [r['id'] for r in selected]}


def main():
    run = sys.argv[1]
    args = sys.argv[2:]
    started = time.perf_counter()
    details = {}
    kind = args[0] if args else 'help'
    try:
        if args in ([], ['--help'], ['-h']):
            print('Retrieval comparison CLI: outline STORE UUID [full|asc]; outline STORE -; search STORE QUERY... [--under UUID] [-y]; instructions [STORE].')
            return
        if args[0] in ('search', 'outline'):
            store = Path(args[1]).resolve()
            if store != CORPUS:
                raise ValueError(f'use the shared frozen STORE: {CORPUS}')
        if args[:1] == ['outline'] and len(args) >= 3 and args[2] == '-':
            text, details = piped_outline()
        elif args[:1] == ['search']:
            text, details = search_packet(args)
        else:
            result = native(args)
            sys.stderr.write(result.stderr)
            text = result.stdout
            if result.returncode:
                sys.stdout.write(text)
                raise SystemExit(result.returncode)
        sys.stdout.write(text)
    finally:
        record = {'command': args, 'seconds': time.perf_counter() - started, **details}
        with (ROOT / run / 'commands.jsonl').open('a') as log:
            log.write(json.dumps(record, ensure_ascii=False) + '\n')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'rejected: {error}', file=sys.stderr)
        raise SystemExit(1)
