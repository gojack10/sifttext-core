// Shared by the standalone gallery and the self-contained MCP resource.
function renderTreeDelta(card, payload) {
const ANCHORS = {};
const REVIEW_NODES = new Map((payload.review || []).map(node => [plainName(node.label || node.name), node]));
const D = {tree: payload.tree, nodes: Object.fromEntries(REVIEW_NODES)};
card.replaceChildren();
const FIELDS = ['name', 'micro_summary', 'status', 'scope', 'warnings'];
const LABEL = {name: 'Title', micro_summary: 'Summary', status: 'Status', scope: 'Scope', warnings: 'Warnings', see_also: 'See also', content: 'Text', files: 'Attached files'};
const STATUS_MARK = {not_examined: ' ', in_progress: '~', resolved: '✓', stuck: '/', deferred: '=', discarded: 'x'};
const STATE_MARK = {changed: ['blue', '~'], added: ['plus', '+'], removed: ['minus', '−']};
const STATUS = {' ': 'not_examined', '~': 'in_progress', '✓': 'resolved', '/': 'stuck', '=': 'deferred', 'x': 'discarded'};
const STATUS_WORD = {' ': 'Not examined', '~': 'In progress', '✓': 'Resolved', '/': 'Blocked', '=': 'Deferred', 'x': 'Discarded'};
const CHANGE = {'[+]': ['added', '+', 'added'], '[!]': ['revised', '~', 'revised'], '[-]': ['removed', '−', 'removed'], '[→][!]': ['moved', '→', 'moved here'], '[←]': ['moved', '←', 'moved away']};
const CELLS = {'│   ': 'pipe', '├── ': 'tee', '└── ': 'elbow', '    ': 'blank'};
const NODE = /^(\[\+\]|\[!\]|\[-\]|\[→\]\[!\])?\[([ ~✓\/=x])\] (.*)$/;
const isCorner = cell => cell === 'tee' || cell === 'elbow';

function h(tag, className, ...children) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.append(...children.flat(Infinity).filter(child => child !== null && child !== undefined && child !== false));
  return node;
}

// tree-render labels a node "@1 - Name (path)" when an artifact link elsewhere points into it.
function plainName(label) {
  const match = /^(@\d+) - (.+) \(\/.+\)$/.exec(label);
  if (match) ANCHORS[match[1]] = match[2];
  return match ? match[2] : label;
}

const PROPOSED = new Map(Object.entries(payload.nodes || {}).map(([label, section]) => [plainName(label), section]));

// Read the verbatim tree into node rows. Lines above the first blank line are the renderer's notation key.
const LINES = D.tree.split('\n');
const ROWS = (() => {
  const rows = [];
  let owner = null, entry = null;
  for (let index = LINES.indexOf('') + 1; index < LINES.length; index++) {
    const line = LINES[index];
    if (!line) continue;
    const cells = [];
    let rest = line;
    while (rest.slice(0, 4) in CELLS && !isCorner(cells.at(-1))) { cells.push(CELLS[rest.slice(0, 4)]); rest = rest.slice(4); }
    if (cells.length && !isCorner(cells.at(-1))) {
      if (owner) entry = detail(owner, line.slice(line.search(/[^│ ]/)), line.search(/[^│ ]/), entry);
      continue;
    }
    const row = {line: index, cells, links: [], files: [], other: []};
    const match = NODE.exec(rest);
    if (!rows.length && !match) continue;
    if (match) Object.assign(row, {kind: 'node', change: match[1] || '', status: match[2], name: plainName(match[3])});
    else if (rest === '...') row.kind = 'skip';
    else if (rest.startsWith('[←] ')) Object.assign(row, {kind: 'node', change: '[←]', name: plainName(rest.slice(4).split(' - moving under ')[0])});
    else Object.assign(row, {kind: 'node', change: rest.startsWith('[-] ') ? '[-]' : '', name: rest.replace(/^\[-\] /, '')});
    if (rows.length) rows.at(-1).parent = cells.length > rows.at(-1).cells.length;
    rows.push(row);
    owner = row.kind === 'node' ? row : null;
    entry = null;
  }
  // tree-render gives a moved-away row no status; show the one its arrival carries.
  for (const row of rows) if (row.change === '[←]') row.status = rows.find(other => other.change === '[→][!]' && other.name === row.name)?.status;
  return rows;
})();

// File one detail line under its node: status change, see_also link, artifact link, or the reason for the entry above it.
function detail(row, text, indent, entry) {
  let match;
  if ((match = /^Status: (.+) -> (.+)$/.exec(text))) { row.statusChange = [match[1], match[2]]; return null; }
  if ((match = /^\[→\] Add see_also -> (.+)$/.exec(text)) || (match = /^\[-\]\[←\] Remove see_also -> (.+)$/.exec(text))) {
    row.links.push(entry = {op: text.startsWith('[→]') ? '+' : '−', what: plainName(match[1]), reason: '', indent});
    return entry;
  }
  if (text === 'artifacts/') return null;
  if ((match = /^([+-]) (.+?) → (.+)$/.exec(text))) {
    row.files.push(entry = {op: match[1] === '+' ? '+' : '−', what: match[2], target: match[3], reason: '', indent});
    return entry;
  }
  if (entry && indent > entry.indent) { entry.reason += (entry.reason ? ' ' : '') + text; return entry; }
  row.other.push(text);
  return null;
}

// Number each distinct attached file where it first appears. A link that reaches the same file through its owner reuses the number.
// Copy/move is explicit display metadata, not an operation inferred from a link. The tool never transfers files.
const FILE_PLACES = new Map();
for (const row of ROWS) for (const entry of row.files) FILE_PLACES.set(row.name + '/' + entry.what, entry.target);
function fileSource(target, seen = new Set()) {
  const through = /^(@\d+)\/artifacts\/(.+)$/.exec(target);
  if (!through || seen.has(target)) return target;
  seen.add(target);
  const next = FILE_PLACES.get(ANCHORS[through[1]] + '/' + through[2]);
  return next ? fileSource(next, seen) : target;
}
const FILE_NOTES = (() => {
  const notes = new Map();
  for (const row of ROWS) for (const entry of row.files) {
    const source = fileSource(entry.target);
    entry.what = 'artifacts/' + entry.what;
    const transfer = payload.transfers?.[source];
    if (!notes.has(source)) notes.set(source, {number: `@${notes.size + 1} - `, what: `${entry.what} ← ${transfer ? '[' + transfer + '] ' : ''}${source}`});
    entry.number = notes.get(source).number;
  }
  return [...notes.values()];
})();

const cellsOf = cells => cells.map(cell => h('span', 'cell ' + cell));
// Guide columns for anything placed under a row: its own corner becomes a continuing line or a blank.
const under = row => [...row.cells.map(cell => cell === 'tee' ? 'pipe' : cell === 'elbow' ? 'blank' : cell), row.parent ? 'pipe' : 'blank'];
const stats = node => h('span', 'stats', h('span', 'plus', '+' + node.plus), node.minus ? h('span', 'minus', ' −' + node.minus) : null);

// Write a fact line's items, optionally cut to a number of characters. A cut line ends "...+N", N being the items not shown at all.
function fill(items, limit = Infinity) {
  let room = limit, hidden = 0;
  items.replaceChildren();
  items.entries.forEach((entry, index) => {
    if (room <= (index ? 3 : 0)) { room = 0; hidden++; return; }
    if (index) { items.append(h('span', 'sep', ' - ')); room -= 3; }
    const number = entry.number || '';
    const item = h('span', entry.op === '−' ? 'gone' : '', entry.what.slice(0, Math.max(0, room - number.length)));
    item.title = entry.reason || '';
    items.append(...[number && h('span', 'ref', number.slice(0, room)), item].filter(Boolean));
    room -= number.length + entry.what.length;
  });
  if (limit !== Infinity) items.append(h('span', 'more', '...' + (hidden ? '+' + hidden : '')));
}

// Show every fact and file-key line in full when it fits its row; otherwise cut it to the longest text that does.
function fit() {
  for (const items of card.querySelectorAll('.items')) {
    const width = items.clientWidth;
    if (!width) continue;
    fill(items);
    if (items.scrollWidth <= width) continue;
    let limit = Math.floor(width / (items.scrollWidth / items.textContent.length)) - 3;
    for (; limit > 0; limit--) {
      fill(items, limit);
      if (items.scrollWidth <= width) break;
    }
  }
}

// One line per kind of arrival, listing every item, so a skim shows what comes in without opening anything.
function factRow(row, kind, entries) {
  const items = h('span', 'items');
  items.entries = entries;
  fill(items);
  return h('div', 'row fact', h('span', 'gutter'), ...cellsOf(under(row)), h('span', 'body', kind, items));
}

function facts(row) {
  const rows = [];
  if (row.statusChange) rows.push(factRow(row, h('span', 'kind', 'Status'), [{what: row.statusChange.join(' → ')}]));
  for (const [list, word] of [[row.links, 'See also'], [row.files, 'File']]) {
    for (const [op, className] of [['+', 'plus'], ['−', 'minus']]) {
      const entries = list.filter(entry => entry.op === op);
      if (entries.length) rows.push(factRow(row, h('span', 'kind', h('span', className, op), ' ' + word), entries));
    }
  }
  for (const text of row.other) rows.push(factRow(row, h('span', 'kind', 'Note'), [{what: text}]));
  rows.at(-1)?.classList.add('last-fact');
  return rows;
}

const plural = (count, noun) => `${count} ${noun}${count === 1 ? '' : 's'}`;
const sentence = text => text.charAt(0).toUpperCase() + text.slice(1);
const shown = (key, value) => key === 'status' ? sentence(value.replace('stuck', 'blocked').replace('_', ' ')) : value;
const hasText = op => (op.text ?? '').trim() !== '';
const wordCount = text => text.match(/[\p{L}\p{N}]+/gu)?.length || 0;

// A revised paragraph marks changed phrases: neighbouring changed words join, absorbing unchanged gaps of a word or two.
// When under half its words then remain unmarked it is a rewrite, shown as plain before and after text.
function revision(segs) {
  const phrases = [];
  let removed = '', added = '';
  const flush = () => {
    if (removed) phrases.push(['-', removed]);
    if (added) phrases.push(['+', added]);
    removed = added = '';
  };
  segs.forEach(([op, part], index) => {
    if (op === '-') removed += part;
    else if (op === '+') added += part;
    else if ((removed || added) && index < segs.length - 1 && wordCount(part) <= 2) { removed += part; added += part; }
    else { flush(); phrases.push(['=', part]); }
  });
  flush();
  const text = (...ops) => phrases.filter(seg => ops.includes(seg[0])).map(seg => seg[1]).join('');
  const before = text('=', '-'), after = text('=', '+');
  if (wordCount(text('=')) * 2 < Math.max(wordCount(before), wordCount(after))) return [[['=', before]], [['=', after]]];
  return [phrases.filter(seg => seg[0] !== '+'), phrases.filter(seg => seg[0] !== '-')];
}

// Fold only when three or more unchanged lines would be hidden; keep the last two before a change as context.
function segment(ops) {
  const out = [];
  let run = [];
  const flush = changeFollows => {
    let keep = run.length;
    for (let text = 0; changeFollows && keep > 0 && text < 2; keep--) if (hasText(run[keep - 1])) text++;
    const hidden = run.slice(0, keep);
    if (hidden.filter(hasText).length >= 3) out.push({k: 'fold', ops: hidden}, ...run.slice(keep));
    else out.push(...run);
    run = [];
  };
  for (const op of ops) {
    if (op.k === 'eq') run.push(op);
    else { flush(true); out.push(op); }
  }
  flush(false);
  return out;
}

function linkedText(value) {
  const fragment = document.createDocumentFragment();
  let cursor = 0;
  for (const match of value.matchAll(/\[([^\]]+)\]\(([^\s)]+)\)/g)) {
    fragment.append(value.slice(cursor, match.index));
    const [, label, target] = match;
    const id = /(?:^|\/)([a-f0-9-]{36})(?:\/node\.md)?(?:#.*)?$/.exec(target)?.[1];
    const available = id && payload.records?.some(record => record.id === id);
    if (available || /^https?:\/\//i.test(target)) {
      const anchor = h('a', '', label);
      anchor.href = available ? '#node-' + id : target;
      if (available) anchor.onclick = event => {
        event.preventDefault(); event.stopPropagation();
        const row = document.getElementById('node-' + id);
        row?.click(); row?.scrollIntoView({block:'center',behavior:'smooth'});
      };
      else { anchor.target = '_blank'; anchor.rel = 'noopener noreferrer'; }
      fragment.append(anchor);
    } else fragment.append(match[0]);
    cursor = match.index + match[0].length;
  }
  fragment.append(value.slice(cursor));
  return fragment;
}
function text(segs) {
  const el = h('div', 'text');
  for (const [op, part] of segs) el.append(op === '=' ? linkedText(part) : h('span', op === '+' ? 'mark-add' : 'mark-del', linkedText(part)));
  if (!el.textContent) el.append('\u00a0');
  if (el.textContent.startsWith('#')) el.classList.add('heading');
  return el;
}
const plain = value => text([['=', value]]);
const link = (entry, reason) => h('div', 'link',
  h('span', 'status status-' + entry.status, '[' + STATUS_MARK[entry.status] + ']'),
  h('div', '', h('div', 'lname', linkedText(`[${entry.name}](${entry.id || ''})`)), h('div', 'reason', reason)));
const file = entry => h('div', 'file', `${entry.path} ← ${entry.transfer ? '[' + entry.transfer + '] ' : ''}${entry.target}`, h('div', 'reason', entry.reason));

// A part contains unchanged pieces, folds, or before/after pairs where either side may be absent.
const same = (el, blank) => ({k: 'eq', el, blank});
const pair = (before, after, blank) => ({k: 'pair', before, after, blank});
function pieces(ops) {
  return segment(ops).map(op => {
    if (op.k === 'fold') return {k: 'fold', items: op.ops.map(inner => same(plain(inner.text), !hasText(inner))), count: op.ops.filter(hasText).length, noun: 'unchanged line'};
    if (op.k === 'eq') return same(plain(op.text), !hasText(op));
    if (op.k === 'mod') { const [before, after] = revision(op.segs); return pair(text(before), text(after)); }
    return op.k === 'del' ? pair(plain(op.text), null, !hasText(op)) : pair(null, plain(op.text), !hasText(op));
  });
}
function fieldOps(key, field) {
  if (field.before === field.after) return [{k: 'eq', text: shown(key, field.after)}];
  if (field.segs) return [{k: 'mod', segs: key === 'status' ? [['-', shown(key, field.before)], ['+', shown(key, field.after)]] : field.segs}];
  return [field.before ? {k: 'del', text: shown(key, field.before)} : null, field.after ? {k: 'add', text: shown(key, field.after)} : null].filter(Boolean);
}

// Green + means gains only, red − losses only, blue ~ a mix of both.
function partState(list) {
  const pairs = list.filter(piece => piece.k === 'pair' && !piece.blank);
  const gains = pairs.some(piece => piece.after), loses = pairs.some(piece => piece.before);
  return gains && loses ? 'changed' : gains ? 'added' : loses ? 'removed' : 'same';
}

function parts(node) {
  const list = [];
  const part = (label, made) => list.push({label, state: partState(made), pieces: made});
  for (const key of FIELDS) {
    const field = node.fields[key];
    if (field.before || field.after) part(LABEL[key], pieces(fieldOps(key, field)));
  }
  if (node.see_also.length) {
    const unchanged = node.see_also.filter(entry => entry.change === 'eq');
    const changed = node.see_also.filter(entry => entry.change !== 'eq');
    part(LABEL.see_also, [
      unchanged.length ? {k: 'fold', items: unchanged.map(entry => same(link(entry, entry.reason))), count: unchanged.length, noun: 'unchanged see-also link'} : null,
      ...changed.map(entry => entry.change === 'del' ? pair(link(entry, entry.reason), null)
        : pair(entry.change === 'mod' ? link(entry, entry.was) : null, link(entry, entry.reason))),
    ].filter(Boolean));
  }
  part(LABEL.content, pieces(node.lines));
  if (node.files.length) part(LABEL.files, node.files.map(entry => entry.op === '+' ? pair(null, file(entry)) : pair(file(entry), null)));
  return list;
}

// One row that shows or hides everything folded behind it.
function revealer(label, bodies) {
  const action = h('span', 'act', 'show');
  const button = h('button', 'fold', label + ' - ', action);
  button.type = 'button';
  button.setAttribute('aria-expanded', 'false');
  button.onclick = event => {
    event.stopPropagation();
    const shown = action.textContent === 'show';
    bodies.forEach(body => body.classList.toggle('shown', shown));
    action.textContent = shown ? 'hide' : 'show';
    button.setAttribute('aria-expanded', shown);
  };
  return button;
}
const folded = (...children) => h('div', 'folded', h('div', '', children));
function fold(children, count, noun) {
  const body = folded(children);
  return [revealer(plural(count, noun), [body]), body];
}
const block = (kind, el) => h('div', 'block ' + kind, el);

// Reworded pieces split left/right; additions and removals keep the full width.
function beside(list) {
  const draw = piece => piece.blank ? null
    : piece.k === 'fold' ? fold(piece.items.map(draw), piece.count, piece.noun)
    : piece.k === 'eq' ? block('eq', piece.el)
    : piece.before && piece.after ? h('div', 'two', block('del', piece.before), block('add', piece.after))
    : block(piece.before ? 'del' : 'add', piece.before || piece.after);
  return h('div', 'pieces', list.map(draw));
}

const tag = state => STATE_MARK[state] && h('span', 'tag ' + STATE_MARK[state][0], STATE_MARK[state][1]);

// Only parts that change are shown. Unchanged parts stay folded in their own positions behind one row at the top.
function panel(row) {
  const node = REVIEW_NODES.get(row.name);
  const files = row.files.map(entry => {
    const source = fileSource(entry.target);
    return {op: entry.op, path: entry.number + entry.what, target: source,
      transfer: payload.transfers?.[source], reason: entry.reason};
  });
  if (!node) return h('div', 'panel', h('div', 'section',
    h('div', 'shead', 'Proposed text - comparison data unavailable'),
    h('pre', 'text', PROPOSED.get(row.name) || 'Only the location or attached files change'),
    files.length > 0 && h('div', 'pieces', files.map(entry => block(entry.op === '+' ? 'add' : 'del', file(entry))))));
  const list = parts({...node, files});
  const splits = list.some(part => part.pieces.some(piece => piece.before && piece.after));
  const unchanged = [];
  const sections = list.map(part => {
    const section = h('div', 'section', h('div', 'shead', tag(part.state), part.label), beside(part.pieces));
    if (part.state !== 'same' || payload.static_view) return section;
    unchanged.push(folded(section));
    return unchanged.at(-1);
  });
  return h('div', 'panel',
    unchanged.length > 0 && revealer(unchanged.length + ' unchanged', unchanged),
    splits && h('div', 'section columns', h('div', 'two heads', h('div', '', 'Now'), h('div', '', 'After this change'))),
    sections);
}

// Scroll input during a motion is discarded, not queued. Afterwards only the coasting tail of a wheel gesture that began
// during the motion is discarded: events arriving in quick succession with shrinking size. A pause or a larger event is new input.
const SCROLL_KEYS = new Set(['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' ']);
function holdScroll() {
  let size = 0, at = 0, ended = false;
  const key = event => { if (SCROLL_KEYS.has(event.key)) event.preventDefault(); };
  const touch = event => event.preventDefault();
  const wheel = event => {
    const amount = Math.abs(event.deltaY);
    const coasting = event.timeStamp - at < 100 && (amount < size || amount <= 3);
    if (ended && !coasting) { removeEventListener('wheel', wheel); return; }
    size = amount;
    at = event.timeStamp;
    event.preventDefault();
  };
  addEventListener('keydown', key);
  addEventListener('touchmove', touch, {passive: false});
  addEventListener('wheel', wheel, {passive: false});
  return () => {
    ended = true;
    removeEventListener('keydown', key);
    removeEventListener('touchmove', touch);
    if (!at) removeEventListener('wheel', wheel);
  };
}

// One motion at a time, starting gently and settling softly; starting another completes the current one at once.
let finishMotion = null;
function animate(step, done) {
  finishMotion?.();
  let frame, releaseScroll;
  const began = performance.now();
  const finish = () => { cancelAnimationFrame(frame); finishMotion = null; step(1); done(); releaseScroll?.(); };
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) { finish(); return; }
  releaseScroll = holdScroll();
  finishMotion = finish;
  const tick = now => {
    const elapsed = Math.max(now - began, 0) / 400;
    if (elapsed >= 1) { finish(); return; }
    step((1 - Math.cos(Math.PI * (1 - (1 - elapsed) ** 1.6))) / 2);
    frame = requestAnimationFrame(tick);
  };
  frame = requestAnimationFrame(tick);
}

// One inline expansion at a time. Keep the clicked row anchored instead of navigating or scrolling it to the top.
let opened = null;
function close(restore = true) {
  finishMotion?.();
  if (!opened) return;
  const {el, expansion, group, x, y} = opened;
  group.forEach(part => part.classList.remove('lit'));
  el.classList.remove('open');
  el.setAttribute('aria-expanded', 'false');
  card.querySelector('.tree').classList.remove('focused');
  fit();
  opened = null;
  if (!restore) { expansion.remove(); return; }
  el.focus({preventScroll: true});
  // Collapse the part on screen while the view returns to where it was before opening.
  const clip = expansion.firstChild, from = scrollY;
  const visible = Math.max(0, Math.min(clip.offsetHeight, innerHeight - clip.getBoundingClientRect().top));
  animate(progress => {
    clip.style.height = visible * (1 - progress) + 'px';
    expansion.style.opacity = 1 - progress;
    scrollTo({left: x, top: from + (y - from) * progress, behavior: 'instant'});
  }, () => expansion.remove());
}

function toggle(row, el) {
  if (opened?.row === row) { close(); return; }
  const anchor = el.getBoundingClientRect().top;
  close(false);
  scrollBy({top: el.getBoundingClientRect().top - anchor, behavior: 'instant'});
  const group = [el];
  while (group.at(-1).nextElementSibling?.classList.contains('fact')) group.push(group.at(-1).nextElementSibling);
  const expansion = h('div', 'expansion', h('div', 'clip', panel(row)));
  opened = {row, el, expansion, group, x: scrollX, y: scrollY};
  group.forEach(part => part.classList.add('lit'));
  el.classList.add('open');
  el.setAttribute('aria-expanded', 'true');
  card.querySelector('.tree').classList.add('focused');
  el.after(expansion);
  // Reveal the diff and bring it into view as one motion: scroll only as far as needed, never moving the clicked row above the top.
  // Only a screenful is animated; the remainder of a taller diff is already below the view when the motion ends.
  const clip = expansion.firstChild, height = clip.offsetHeight, start = scrollY;
  const reveal = Math.min(height, innerHeight);
  clip.style.height = '0px';
  expansion.style.opacity = 0;
  const distance = Math.max(0, Math.min(el.getBoundingClientRect().bottom + height + 12 - innerHeight, el.getBoundingClientRect().top - 12));
  animate(progress => {
    clip.style.height = reveal * progress + 'px';
    expansion.style.opacity = progress;
    if (distance) scrollTo({top: start + distance * progress, behavior: 'instant'});
  }, () => { clip.style.height = expansion.style.opacity = ''; });
}

function icon(path, className) {
  const template = document.createElement('template');
  template.innerHTML = `<svg class="${className}" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="${path}"/></svg>`;
  return template.content.firstChild;
}
function copyButton(name, text) {
  const label = h('span', '', name);
  const button = h('button', 'btn', icon('M5 5h9v9H5zM2 11V2h9', ''), label, icon('m3 8 3 3 7-7', 'check'));
  button.type = 'button';
  button.onclick = async () => {
    try {
      try { await navigator.clipboard.writeText(text); }
      catch {
        const area = h('textarea', '', text);
        area.style.cssText = 'position:fixed;opacity:0';
        document.body.append(area);
        area.select();
        const copied = document.execCommand('copy');
        area.remove();
        if (!copied) throw new Error('Clipboard blocked');
      }
      button.classList.add('copied');
    } catch { label.textContent = 'Copy blocked'; }
    setTimeout(() => { button.classList.remove('copied'); label.textContent = name; }, 1800);
  };
  return button;
}

function draw(card) {
  const counts = new Map();
  const count = (key, amount = 1) => counts.set(key, (counts.get(key) || 0) + amount);
  const weight = h('div', 'weight');
  let plus = 0, minus = 0;
  const tree = h('div', 'tree');
  for (const row of ROWS) {
    const [kind, glyph, word] = CHANGE[row.change] || [];
    const el = h('div', 'row ' + row.kind + (kind ? ' changed ' + kind : ''), h('span', 'gutter', glyph), ...cellsOf(row.cells));
    el.dataset.line = row.line;
    const identity = REVIEW_NODES.get(row.name)?.id;
    if (identity && row.change !== '[←]') el.id = 'node-' + identity;
    if (row.change === '[←]') el.classList.add('away');
    if (row.kind === 'skip') { el.append(h('span', 'body', '...')); tree.append(el); continue; }
    const node = D.nodes[row.name];
    el.append(h('span', 'body',
      row.status === undefined ? null : h('span', 'status status-' + STATUS[row.status], '[' + row.status + ']' + (row.change === '[←]' ? ' ' : '')),
      h('span', 'name', row.name), row.change && node ? stats(node) : null));
    if (row.change) count(row.change);
    count('links', row.links.length);
    count('files', row.files.length);
    const below = facts(row);
    if (node || PROPOSED.has(row.name) || row.links.length || row.files.length) {
      el.classList.add('pick');
      el.tabIndex = 0;
      el.setAttribute('role', 'button');
      el.setAttribute('aria-expanded', 'false');
      const pick = event => { event.stopPropagation(); if (getSelection().isCollapsed) toggle(row, el); };
      el.onclick = pick;
      // The lines beneath a node open its diff too.
      for (const fact of below) {
        fact.classList.add('pick');
        fact.onclick = pick;
      }
      el.onkeydown = event => { if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); toggle(row, el); } };
    }
    if (row.change && row.change !== '[←]' && node && node.plus + node.minus) {
      plus += node.plus;
      minus += node.minus;
      const seg = h('button', 'seg', h('i', 'p'), h('i', 'm'));
      seg.type = 'button';
      seg.title = `${row.name} - +${node.plus} −${node.minus}`;
      seg.style.flexGrow = node.plus + node.minus;
      seg.firstChild.style.flexGrow = node.plus;
      seg.lastChild.style.flexGrow = node.minus;
      seg.onclick = () => { if (opened?.row !== row) toggle(row, el); };
      weight.append(seg);
    }
    tree.append(el, ...below);
  }

  // Click the expansion or surrounding tree to collapse, except when selecting text.
  tree.onclick = () => { if (getSelection().isCollapsed) close(); };
  // The counts double as the key to the change marks.
  const summary = h('div', 'summary');
  for (const [mark, [kind, glyph, word]] of Object.entries(CHANGE)) {
    if (counts.get(mark)) summary.append(h('span', '', h('span', kind, glyph), ' ', h('b', '', counts.get(mark)), ' ' + word));
  }
  if (counts.get('links')) summary.append(h('span', '', h('b', '', counts.get('links')), ' ' + (counts.get('links') === 1 ? 'see-also link' : 'see-also links')));
  if (counts.get('files')) summary.append(h('span', '', h('b', '', counts.get('files')), ' ' + (counts.get('files') === 1 ? 'attached file' : 'attached files')));
  if (REVIEW_NODES.size) summary.append(h('span', 'grow'), h('span', '', h('span', 'plus', '+' + plus), ' ', h('span', 'minus', '−' + minus), ' lines'));

  const legend = h('div', 'legend');
  for (const status of new Set(ROWS.map(row => row.status).filter(status => status !== undefined))) legend.append(h('span', '', h('span', 'status status-' + STATUS[status], '[' + status + ']'), ' ' + STATUS_WORD[status]));

  card.append(
    h('div', 'head', h('span', 'title', 'Tree Delta'), copyButton('Copy tree', D.tree)),
    summary, weight, legend,
    (payload.work_record || []).map(stage => h('section', 'section', h('h3', '', stage.title), plain(stage.body))),
    h('div', 'notes', FILE_NOTES.map(note => { const line = h('div', 'items'); line.entries = [note]; fill(line); return line; })),
    tree);
}

// Copying a selection of tree rows yields the CLI's original lines.
const onCopy = event => {
  const selection = getSelection();
  if (!selection.anchorNode || !card.contains(selection.anchorNode)) return;
  const picked = [...card.querySelectorAll('.tree > .row[data-line]')].filter(row => selection.containsNode(row, true));
  if (selection.isCollapsed || !picked.length || selection.anchorNode.parentElement?.closest('.panel')) return;
  event.clipboardData.setData('text/plain', picked.map(row => LINES[row.dataset.line]).join('\n'));
  event.preventDefault();
};
const onKey = event => { if (event.key === 'Escape') close(); };
document.addEventListener('copy', onCopy);
addEventListener('keydown', onKey);
draw(card);
// Refit the fact lines whenever the available width or the font changes.
let fitted = 0;
const observer = new ResizeObserver(([entry]) => { if (entry.contentRect.width !== fitted) { fitted = entry.contentRect.width; fit(); } });
observer.observe(card);
document.fonts.ready.then(fit);
return () => {
  finishMotion?.();
  observer.disconnect();
  document.removeEventListener('copy', onCopy);
  removeEventListener('keydown', onKey);
};
}
