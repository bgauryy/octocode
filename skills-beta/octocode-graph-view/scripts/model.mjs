// Overlay: validate model.json against the graph, assign layers, derive upward imports.
export function resolveModel(D, M) {
  const warnings = [];
  const ids = new Set([...D.files.map((n) => n.id), ...(M.nodes ?? []).map((n) => n.id)]);
  const glob = (g) => new RegExp(`^${g.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*\*\//g, '\x01').replace(/\*\*/g, '\0').replace(/\*/g, '[^/]*').replace(/\0/g, '.*').replace(/\x01/g, '(?:.*/)?')}$`);
  const layers = [...(M.layers ?? [])].sort((a, b) => a.order - b.order);
  const byId = new Map(layers.map((l) => [l.id, l]));
  const palette = ['#4e79a7', '#59a14f', '#edc948', '#e15759', '#b07aa1', '#76b7b2', '#ff9da7', '#9c755f'];
  layers.forEach((l, i) => { l.color ??= palette[i % palette.length]; l.matchers = l.match.map(glob); });
  const layerOf = {};
  for (const n of D.files) layerOf[n.id] = layers.find((l) => l.matchers.some((m) => m.test(n.id)))?.id;
  for (const a of M.assign ?? []) {
    if (!ids.has(a.id)) warnings.push(`assign: unknown node ${a.id}`);
    else if (!byId.has(a.layer)) warnings.push(`assign ${a.id}: unknown layer ${a.layer}`);
    else layerOf[a.id] = a.layer;
  }
  for (const n of M.nodes ?? []) {
    if (n.layer && !byId.has(n.layer)) warnings.push(`node ${n.id}: unknown layer ${n.layer}`);
    else if (n.layer) layerOf[n.id] = n.layer;
  }
  const ev = (e) => e && e.path && e.line;
  for (const e of M.edges ?? []) {
    for (const k of ['source', 'target']) if (!ids.has(e[k])) warnings.push(`edge ${e.source} > ${e.target}: unknown ${k} ${e[k]}`);
    if (!ev(e.evidence)) warnings.push(`edge ${e.source} > ${e.target}: missing evidence`);
  }
  for (const fl of M.flows ?? []) {
    fl.steps.forEach((st, i) => {
      for (const k of ['from', 'to']) if (!ids.has(st[k])) warnings.push(`flow ${fl.id} step ${i + 1}: unknown ${k} ${st[k]}`);
      if (!ev(st.evidence)) warnings.push(`flow ${fl.id} step ${i + 1}: missing evidence`);
    });
  }
  for (const x of M.findings ?? []) if (x.confidence === 'confirmed' && (!x.evidence?.length || !x.evidence.every(ev))) warnings.push(`finding "${x.title}": confirmed needs evidence`);
  const allowed = new Set((M.allowed ?? []).map((a) => `${a.source}>${a.target}`));
  const order = (id) => byId.get(layerOf[id])?.order;
  const upward = D.imports.filter((e) => {
    const [a, b] = [order(e.source), order(e.target)];
    return a != null && b != null && a > b && !byId.get(layerOf[e.source]).exempt && !allowed.has(`${e.source}>${e.target}`);
  }).map((e) => ({ source: e.source, target: e.target, line: e.line }));
  const unplaced = D.files.filter((n) => !layerOf[n.id]).map((n) => n.id);
  if (layers.length && unplaced.length) warnings.push(`${unplaced.length} files match no layer (first: ${unplaced[0]})`);
  return {
    meta: M.meta ?? {},
    layers: layers.map(({ matchers, ...l }) => l),
    layerOf,
    nodes: M.nodes ?? [],
    edges: M.edges ?? [],
    flows: M.flows ?? [],
    findings: M.findings ?? [],
    upward,
    warnings,
  };

}
