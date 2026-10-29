// Which graph claims get tested: upward imports, cycle edges, heavy imports into hubs, ranked file findings, top hubs.
export function selectLeads(D, model, max) {
  const fileIds = new Set(D.files.map((n) => n.id));
  const fanIn = new Map(D.files.map((n) => [n.id, n.fanIn]));
  const cycleSets = D.insights.cycles.top.map((c) => new Set(c.nodes));
  const groupOf = new Map(D.files.map((n) => [n.id, n.group]));
  const leads = [];
  const add = (l) => {
    const id = `${l.kind}:${l.file}${l.target ? `>${l.target}` : ''}:${l.detector}`;
    if (!leads.some((x) => x.id === id) && leads.length < max) leads.push({ id, ...l, signals: { graph: l.graph } });
  };
  for (const e of (model?.upward ?? []).slice(0, 3)) add({ kind: 'edge', detector: 'upward-import', file: e.source, target: e.target, line: e.line, graph: { layers: [model.layerOf[e.source], model.layerOf[e.target]] } });
  D.imports
    .filter((e) => cycleSets.some((s) => s.has(e.source) && s.has(e.target)))
    .sort((a, b) => fanIn.get(b.target) - fanIn.get(a.target))
    .slice(0, 3)
    .forEach((e) => add({ kind: 'edge', detector: 'cycle-edge', file: e.source, target: e.target, line: e.line, graph: { inCycle: true, targetFanIn: fanIn.get(e.target) } }));
  const cross = (e) => Number(groupOf.get(e.source) !== groupOf.get(e.target));
  D.imports
    .filter((e) => !leads.some((l) => l.file === e.source && l.target === e.target))
    .sort((a, b) => cross(b) - cross(a) || fanIn.get(b.target) - fanIn.get(a.target))
    .slice(0, 2)
    .forEach((e) => add({ kind: 'edge', detector: 'hub-edge', file: e.source, target: e.target, line: e.line, graph: { crossGroup: !!cross(e), targetFanIn: fanIn.get(e.target) } }));
  const FILE_DETECTORS = ['god-file', 'critical-file', 'single-point-of-failure', 'misplaced-file'];
  D.insights.issues
    .filter((x) => FILE_DETECTORS.includes(x.detector) && fileIds.has(x.subject))
    .slice(0, 3)
    .forEach((x) => add({ kind: 'file', detector: x.detector, file: x.subject, title: x.title, graph: { score: x.score } }));
  for (const h of [...D.files].sort((a, b) => b.fanIn - a.fanIn)) if (leads.length < max) add({ kind: 'file', detector: 'hub', file: h.id, title: `fan-in ${h.fanIn}`, graph: { fanIn: h.fanIn } });
  return leads;
}
