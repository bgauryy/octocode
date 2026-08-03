import { writeFileSync } from 'fs';
import { join, resolve } from 'path';
import { pathToFileURL } from 'url';

const { waitForPageReady } = await import(pathToFileURL(resolve(process.cwd(), '.octocode', 'dom-actionability.mjs')).href);

// Compact, ref-based alternative to selector-guessing or screenshots: capture
// the accessibility tree, keep only interactive/named nodes, give each a
// short ref backed by a stable backendDOMNodeId. Pass that ref straight to
// dom-operations-check.mjs's DOM_REF to act on it without writing a selector.
//
// Env:
//   SNAPSHOT_DEPTH  max AX tree depth to request (default: unlimited -> -1)
//   SNAPSHOT_MAX    max refs to keep, highest-signal first (default: 60)

const DEPTH = Number.parseInt(process.env.SNAPSHOT_DEPTH ?? '-1', 10);
const MAX_REFS = Math.max(1, Math.min(300, Number.parseInt(process.env.SNAPSHOT_MAX ?? '60', 10)));

const INTERACTIVE_ROLES = new Set([
  'button', 'link', 'textbox', 'searchbox', 'combobox', 'checkbox', 'radio',
  'switch', 'slider', 'spinbutton', 'menuitem', 'menuitemcheckbox', 'menuitemradio',
  'tab', 'option', 'listbox', 'listitem_selectable',
]);

export async function run(cdp) {
  await cdp.send('Accessibility.enable');
  await cdp.send('DOM.enable');

  const ready = await waitForPageReady(cdp);
  if (!ready) console.log('[FINDING] PAGE_NOT_FULLY_LOADED document.readyState never reached "complete" within timeout — snapshot may be incomplete');

  let { nodes } = await cdp.send('Accessibility.getFullAXTree', DEPTH > 0 ? { depth: DEPTH } : {});
  if (nodes.length < 10) {
    // Chrome's accessibility tree can lag a tick behind document.readyState; one bounded retry
    // catches the case reproduced empirically (near-empty tree moments after a fresh navigation).
    await new Promise((r) => setTimeout(r, 500));
    const retry = await cdp.send('Accessibility.getFullAXTree', DEPTH > 0 ? { depth: DEPTH } : {});
    if (retry.nodes.length > nodes.length) {
      console.log(`[FINDING] AX_TREE_RETRY first capture had ${nodes.length} nodes, retry had ${retry.nodes.length} — using retry`);
      nodes = retry.nodes;
    }
  }

  const kept = [];
  const seen = new Set();
  let duplicatesDropped = 0;
  for (const node of nodes) {
    if (node.ignored) continue;
    const role = node.role?.value ?? '';
    const name = node.name?.value ?? '';
    if (!role || !node.backendDOMNodeId) continue;
    const interactive = INTERACTIVE_ROLES.has(role);
    const named = Boolean(name && name.trim());
    if (!interactive && !(named && ['heading', 'img', 'text'].includes(role))) continue;
    const trimmedName = name.trim();
    // Responsive layouts commonly duplicate whole nav/footer blocks (desktop +
    // mobile variants) — same role+name, different node. Keep the first
    // (typically the primary, DOM-earlier one) and drop exact repeats instead
    // of burning refs/tokens on look-alike entries.
    if (trimmedName) {
      const dupKey = `${role}|${trimmedName}`;
      if (seen.has(dupKey)) { duplicatesDropped++; continue; }
      seen.add(dupKey);
    }
    kept.push({ role, name: trimmedName, backendDOMNodeId: node.backendDOMNodeId, interactive });
  }

  // Highest signal first: interactive+named, then interactive, then named-only.
  kept.sort((a, b) => {
    const score = (n) => (n.interactive && n.name ? 2 : n.interactive ? 1 : 0);
    return score(b) - score(a);
  });
  const trimmed = kept.slice(0, MAX_REFS);

  const refs = {};
  const lines = [];
  trimmed.forEach((node, i) => {
    const ref = `e${i + 1}`;
    refs[ref] = { backendDOMNodeId: node.backendDOMNodeId, role: node.role, name: node.name };
    lines.push(`[${ref}] ${node.role}${node.name ? ` "${node.name}"` : ''}`);
  });

  const artifactPath = join(cdp.outputDir, 'page-snapshot.json');
  writeFileSync(artifactPath, `${JSON.stringify({ url: cdp.targetInfo.url, refs }, null, 2)}\n`, { mode: 0o600 });
  cdp.upsertResourceMap?.('page-snapshot', {
    type: 'page-snapshot',
    targetUrl: cdp.targetInfo.url,
    refCount: trimmed.length,
    totalAxNodes: nodes.length,
    artifactPath,
  });

  console.log(`[METRIC] SNAPSHOT refs=${trimmed.length} totalAxNodes=${nodes.length}${duplicatesDropped ? ` duplicatesDropped=${duplicatesDropped}` : ''}`);
  for (const line of lines) console.log(`[SNAPSHOT] ${line}`);
  console.log(`[ARTIFACT] PAGE_SNAPSHOT ${artifactPath}`);
  console.log('[REASON] Act on a ref with dom-operations-check.mjs: DOM_REF=e3 DOM_ACTION=click');
}
