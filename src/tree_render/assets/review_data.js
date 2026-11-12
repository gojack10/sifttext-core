// Native snapshots carry exact records; the shared view uses jsdiff (BSD-3-Clause).
function prepareTreeReview(payload) {
  const segments = (before, after) => Diff.diffWordsWithSpace(before, after).map(p => [p.added ? '+' : p.removed ? '-' : '=', p.value]);
  const lines = (before, after) => {
    const left = before ? before.split('\n') : [], right = after ? after.split('\n') : [];
    const chunks = Diff.diffArrays(left, right), result = [];
    for (let i = 0; i < chunks.length; i++) {
      const chunk = chunks[i];
      if (!chunk.added && !chunk.removed) { result.push(...chunk.value.map(text => ({k:'eq', text}))); continue; }
      if (chunk.removed && chunks[i + 1]?.added) {
        const removed = [...chunk.value], added = [...chunks[++i].value];
        while (removed.length && added.length) {
          const segs = segments(removed[0], added[0]);
          const equal = segs.filter(s => s[0] === '=').reduce((n, s) => n + s[1].length, 0);
          if (equal * 4 <= removed[0].length + added[0].length) break;
          result.push({k:'mod', segs}); removed.shift(); added.shift();
        }
        result.push(...removed.map(text => ({k:'del', text})), ...added.map(text => ({k:'add', text})));
      } else result.push(...chunk.value.map(text => ({k:chunk.removed ? 'del' : 'add', text})));
    }
    return result;
  };
  const records = payload.records || [];
  const names = Object.fromEntries(records.map(r => [r.id, r.after || r.before || {}]));
  const review = records.map(record => {
    const before = record.before || {}, after = record.after || {};
    const fields = Object.fromEntries(['name','micro_summary','status','scope','warnings'].map(key => {
      const old = before[key] || '', next = after[key] || '';
      return [key, {before:old, after:next, segs:old && next && old !== next ? segments(old,next) : null}];
    }));
    const oldLinks = new Map((before.see_also || []).map(e => [e.target,e]));
    const newLinks = new Map((after.see_also || []).map(e => [e.target,e]));
    const see_also = [...new Set([...oldLinks.keys(),...newLinks.keys()])].map(target => {
      const old = oldLinks.get(target), next = newLinks.get(target);
      const linked = names[target] || payload.references?.[target] || {};
      return {id:target, name:linked.name || target, status:linked.status || 'not_examined',
        change:!next ? 'del' : !old ? 'add' : old.reason !== next.reason ? 'mod' : 'eq',
        reason:(next || old).reason, was:old?.reason};
    });
    const operations = lines((before.content || '').replace(/^\n|\n$/g,''), (after.content || '').replace(/^\n|\n$/g,''));
    return {id:record.id, label:record.label, name:(record.after || record.before).name, added:!record.before,
      fields, see_also, lines:operations, files:[],
      plus:operations.filter(op => ['add','mod'].includes(op.k)).length,
      minus:operations.filter(op => ['del','mod'].includes(op.k)).length};
  });
  return {...payload, review, nodes:Object.fromEntries(records.map(r => [r.label,(r.after || r.before).content || '']))};
}
