// Shared accessibility-tree ref collection (page-snapshot, dom-operations diff, annotated screenshots).
// Library: imported from .octocode/ax-snapshot.mjs (cdp-sandbox stages it); never run as a CLI.

export const INTERACTIVE_ROLES = new Set([
  'button', 'link', 'textbox', 'searchbox', 'combobox', 'checkbox', 'radio',
  'switch', 'slider', 'spinbutton', 'menuitem', 'menuitemcheckbox', 'menuitemradio',
  'tab', 'option', 'listbox', 'treeitem',
]);
export const REGION_ROLES = new Set(['banner', 'navigation', 'main', 'complementary', 'contentinfo', 'search', 'form', 'region', 'dialog', 'alertdialog']);
const NAV_ROLES = new Set(['navigation', 'banner', 'contentinfo', 'complementary', 'menu', 'menubar']);
const CLICKABLE_SKIP_ROLES = new Set(['RootWebArea', 'Iframe', 'heading', 'img', 'StaticText', 'InlineTextBox', 'LineBreak', 'paragraph', 'list']);
const MAX_NAME = 80;

export const clip = (s, n) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);
export const flat = (s) => String(s ?? '').trim().replace(/\s+/g, ' ');

async function fullTree(cdp, params) {
  let { nodes } = await cdp.send('Accessibility.getFullAXTree', params);
  if (nodes.length < 10 && !params.frameId) {
    // The AX tree can lag a tick behind readyState right after navigation; one bounded retry.
    await new Promise((r) => setTimeout(r, 500));
    const retry = await cdp.send('Accessibility.getFullAXTree', params);
    if (retry.nodes.length > nodes.length) nodes = retry.nodes;
  }
  return nodes;
}

// Main-frame AX nodes plus same-process child frames, each frame's ids prefixed so they cannot collide.
async function axForest(cdp, depth, findings) {
  const params = depth > 0 ? { depth } : {};
  const main = await fullTree(cdp, params);
  const frames = []; // { ownerBackendId, nodes }
  const tree = await cdp.send('Page.getFrameTree').catch(() => null);
  const children = [];
  const walk = (ft) => { for (const c of ft?.childFrames ?? []) { children.push(c.frame); walk(c); } };
  walk(tree?.frameTree);
  for (const [i, frame] of children.entries()) {
    try {
      const owner = await cdp.send('DOM.getFrameOwner', { frameId: frame.id });
      const nodes = (await cdp.send('Accessibility.getFullAXTree', { ...params, frameId: frame.id })).nodes
        .map((n) => ({ ...n, nodeId: `f${i}:${n.nodeId}`, childIds: (n.childIds ?? []).map((c) => `f${i}:${c}`), frameUrl: frame.url }));
      frames.push({ ownerBackendId: owner.backendNodeId, nodes });
    } catch {
      findings.push(`FRAME_SKIPPED ${clip(frame.url || frame.id, 100)} (cross-origin or detached; attach to its target to inspect)`);
    }
  }
  return { main, frames };
}

// backendNodeId -> { pointerStart, handler, bounds } from one DOMSnapshot over all documents.
async function layoutInfo(cdp) {
  const { documents, strings } = await cdp.send('DOMSnapshot.captureSnapshot', { computedStyles: ['cursor'] });
  const info = new Map();
  for (const doc of documents) {
    const { nodes, layout } = doc;
    const cursorOf = new Map();
    const boundsOf = new Map();
    layout.nodeIndex.forEach((nodeIdx, i) => {
      cursorOf.set(nodeIdx, strings[layout.styles[i]?.[0]] ?? '');
      boundsOf.set(nodeIdx, layout.bounds[i]);
    });
    const parentCursor = (idx) => {
      for (let p = nodes.parentIndex[idx]; p >= 0; p = nodes.parentIndex[p]) if (cursorOf.has(p)) return cursorOf.get(p);
      return '';
    };
    nodes.backendNodeId.forEach((backendId, idx) => {
      if (nodes.nodeType[idx] !== 1) return;
      const attrs = nodes.attributes[idx] ?? [];
      let handler = false;
      for (let a = 0; a < attrs.length; a += 2) {
        const name = strings[attrs[a]];
        if (name === 'onclick' || name === 'onmousedown' || (name === 'tabindex' && strings[attrs[a + 1]] !== '-1')) handler = true;
      }
      const pointerStart = cursorOf.get(idx) === 'pointer' && parentCursor(idx) !== 'pointer';
      if (pointerStart || handler || boundsOf.has(idx)) info.set(backendId, { pointerStart, handler, bounds: boundsOf.get(idx) });
    });
  }
  return info;
}

/**
 * Walk the page in document order and keep controls, headings, named images, regions, and
 * non-semantic clickables. Options: rootBackendId, viewport ({pageX,pageY,clientWidth,clientHeight}),
 * clickable (default true), depth.
 */
export async function collectRefs(cdp, { rootBackendId = null, viewport = null, clickable = true, depth = -1 } = {}) {
  await cdp.send('Accessibility.enable');
  await cdp.send('DOM.enable');
  const findings = [];
  const { main, frames } = await axForest(cdp, depth, findings);
  const all = [...main, ...frames.flatMap((f) => f.nodes)];
  const byId = new Map(all.map((n) => [n.nodeId, n]));
  const frameRoots = new Map();
  for (const f of frames) {
    const childIds = new Set(f.nodes.flatMap((n) => n.childIds));
    frameRoots.set(f.ownerBackendId, f.nodes.filter((n) => !childIds.has(n.nodeId)));
  }
  const layout = clickable || viewport ? await layoutInfo(cdp).catch(() => null) : null;
  const inViewport = (id) => {
    if (!viewport) return true;
    const b = layout?.get(id)?.bounds;
    if (!b) return false;
    const [x, y, w, h] = b;
    return w > 0 && h > 0 && y + h > viewport.pageY && y < viewport.pageY + viewport.clientHeight && x + w > viewport.pageX && x < viewport.pageX + viewport.clientWidth;
  };

  let starts;
  if (rootBackendId) {
    let start = all.find((n) => n.backendDOMNodeId === rootBackendId);
    if (!start) {
      const partial = await cdp.send('Accessibility.getPartialAXTree', { backendNodeId: rootBackendId, fetchRelatives: false }).catch(() => null);
      start = partial?.nodes?.map((n) => byId.get(n.nodeId)).find(Boolean);
    }
    if (!start) throw new Error('root element has no accessibility node; try a parent selector');
    starts = [start];
  } else {
    const childIds = new Set(main.flatMap((n) => n.childIds ?? []));
    starts = main.filter((n) => !childIds.has(n.nodeId));
  }

  const useful = [];
  const regions = [];
  const seen = new Set();
  let duplicatesDropped = 0;
  const stack = [...starts].reverse().map((node) => ({ node, inNav: false, inControl: false }));
  const visited = new Set();
  while (stack.length) {
    const item = stack.pop();
    if (item.exit) {
      if (item.region) item.region.end = useful.length;
      // A clickable wrapper around real controls adds nothing: the controls are the targets.
      if (item.clickable && useful.slice(item.at + 1).some((u) => u && u.interactive && u.role !== 'clickable')) useful.splice(item.at, 1, null);
      continue;
    }
    const node = item.node;
    if (!node || visited.has(node.nodeId)) continue;
    visited.add(node.nodeId);
    const role = node.role?.value ?? '';
    const name = flat(node.name?.value);
    const id = node.backendDOMNodeId;
    const visible = id && inViewport(id);
    const interactive = INTERACTIVE_ROLES.has(role);
    if (!node.ignored && id && visible) {
      if (REGION_ROLES.has(role) && (role !== 'region' || name)) {
        const region = { role, name: clip(name, 60), backendDOMNodeId: id, start: useful.length, end: useful.length };
        regions.push(region);
        stack.push({ exit: true, region });
      }
      if (interactive || (name && ['heading', 'img'].includes(role))) {
        // Responsive layouts duplicate nav/footer blocks: keep the first same role+name there.
        // Content repeats (a "hide" link per row) stay: each one acts on a different item.
        const dupKey = name && (item.inNav || NAV_ROLES.has(role)) ? `${role}|${name}` : null;
        if (dupKey && seen.has(dupKey)) duplicatesDropped++;
        else {
          if (dupKey) seen.add(dupKey);
          const level = role === 'heading' ? node.properties?.find((p) => p.name === 'level')?.value?.value : undefined;
          useful.push({ role, name: clip(name, MAX_NAME), fullName: name, backendDOMNodeId: id, interactive, level, frame: node.frameUrl });
        }
      }
    }
    // Non-semantic clickables (div with a click handler and cursor:pointer): Chrome often marks
    // them ignored/generic, so the role filter above never sees them.
    let isClickable = false;
    if (clickable && id && visible && !interactive && !item.inControl && !CLICKABLE_SKIP_ROLES.has(role)) {
      const l = layout?.get(id);
      const focusable = node.properties?.some((p) => p.name === 'focusable' && p.value?.value);
      if (l && (l.pointerStart || l.handler || (focusable && !node.ignored && role === 'generic'))) {
        isClickable = true;
        stack.push({ exit: true, clickable: true, at: useful.length });
        useful.push({ role: 'clickable', name: clip(name, MAX_NAME), fullName: name, backendDOMNodeId: id, interactive: true, frame: node.frameUrl });
      }
    }
    const inNav = item.inNav || (!node.ignored && NAV_ROLES.has(role));
    const inControl = item.inControl || interactive || isClickable;
    const kids = [...(node.childIds ?? [])];
    const frameKids = id ? frameRoots.get(id) ?? [] : [];
    for (const child of [...kids.map((k) => byId.get(k)), ...frameKids].reverse()) stack.push({ node: child, inNav, inControl });
  }

  // Drop wrappers removed above and re-index region spans.
  const keep = [];
  const remap = [];
  useful.forEach((u) => { remap.push(keep.length); if (u) keep.push(u); });
  remap.push(keep.length);
  for (const r of regions) { r.start = remap[r.start]; r.end = remap[r.end]; }
  return { useful: keep, regions, totalNodes: all.length, duplicatesDropped, findings };
}
