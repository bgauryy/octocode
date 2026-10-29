// Churn, hotspots and co-change from one `git log --name-only --format=@sha|author|date` over the section.
export function analyzeHistory(log, D, opts) {
  const fileIds = new Set(D.files.map((n) => n.id));
  const commits = [];
  for (const line of log.split('\n')) {
    if (line.startsWith('@')) { const [h, an, ad] = line.slice(1).split('|'); commits.push({ h, an, ad, files: [] }); }
    else if (line && commits.length) commits.at(-1).files.push(line);
  }
  const sizes = commits.map((c) => c.files.length).sort((a, b) => a - b);
  const small = commits.filter((c) => c.files.length > 0 && c.files.length <= opts.maxCommitFiles);
  const since = new Date(Date.now() - 90 * 864e5).toISOString().slice(0, 10);
  const per = new Map();
  for (const c of commits) for (const file of c.files) {
    if (!fileIds.has(file)) continue;
    const cur = per.get(file) ?? { commits: 0, recent90: 0, last: '', first: c.ad, authors: new Set() };
    cur.commits++;
    if (c.ad >= since) cur.recent90++;
    if (c.ad > cur.last) cur.last = c.ad;
    if (c.ad < cur.first) cur.first = c.ad;
    cur.authors.add(c.an);
    per.set(file, cur);
  }
  const all = new Map();
  for (const c of commits) for (const file of c.files) all.set(file, (all.get(file) ?? 0) + 1);
  const pair = new Map();
  for (const c of small) {
    const inView = c.files.filter((x) => fileIds.has(x));
    for (let i = 0; i < inView.length; i++) for (let j = i + 1; j < inView.length; j++) {
      const k = [inView[i], inView[j]].sort().join('>');
      pair.set(k, (pair.get(k) ?? 0) + 1);
    }
  }
  const imported = new Set(D.imports.flatMap((e) => [`${e.source}>${e.target}`, `${e.target}>${e.source}`]));
  const history = {
    quality: { commits: commits.length, truncated: opts.limit > 0 && commits.length >= opts.limit, small: small.length, maxPair: [...pair.values()].reduce((m, n) => Math.max(m, n), 0), medianFiles: sizes[Math.floor(sizes.length / 2)] ?? 0, maxCommitFiles: opts.maxCommitFiles },
    files: Object.fromEntries([...per].map(([k, v]) => [k, { commits: v.commits, recent90: v.recent90, first: v.first, last: v.last, authors: v.authors.size }])),
    coChange: [...pair].filter(([, n]) => n >= 3).map(([k, n]) => { const [a, b] = k.split('>'); return { a, b, count: n, imported: imported.has(`${a}>${b}`) }; }).sort((x, y) => y.count - x.count).slice(0, 10),
    top: [...all].sort((a, b) => b[1] - a[1]).slice(0, 5).map(([id, commits]) => ({ id, commits })),
    hotspots: D.files.filter((x) => per.has(x.id)).map((x) => ({ id: x.id, fanIn: x.fanIn, commits: per.get(x.id).commits, recent90: per.get(x.id).recent90 })).sort((a, b) => b.fanIn * b.commits - a.fanIn * a.commits).slice(0, 5),
  };
  return history;
}
