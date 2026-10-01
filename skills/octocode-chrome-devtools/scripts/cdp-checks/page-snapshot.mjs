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
//   SNAPSHOT_STDOUT summary keeps refs on disk; default prints refs for direct interaction
//   SNAPSHOT_TEXT   print the first N chars (max 4000) of main/body text; full text -> page-text.txt

const DEPTH = Number.parseInt(process.env.SNAPSHOT_DEPTH ?? '-1', 10);
const MAX_REFS = Math.max(1, Math.min(300, Number.parseInt(process.env.SNAPSHOT_MAX ?? '60', 10)));
const MAX_NAME = 80;
const TEXT_CHARS = Math.max(0, Math.min(4000, Number.parseInt(process.env.SNAPSHOT_TEXT ?? '0', 10) || 0));

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

  // getFullAXTree returns nodes in no useful order (shallow footers come before deep
  // content); walk childIds depth-first so refs read top-to-bottom like the page.
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));
  const childIds = new Set(nodes.flatMap((n) => n.childIds ?? []));
  const ordered = [];
  const stack = nodes.filter((n) => !childIds.has(n.nodeId)).reverse();
  const visited = new Set();
  while (stack.length) {
    const n = stack.pop();
    if (!n || visited.has(n.nodeId)) continue;
    visited.add(n.nodeId);
    ordered.push(n);
    for (const id of [...(n.childIds ?? [])].reverse()) stack.push(byId.get(id));
  }
  for (const n of nodes) if (!visited.has(n.nodeId)) ordered.push(n);

  const kept = [];
  const seen = new Set();
  let duplicatesDropped = 0;
  for (const node of ordered) {
    if (node.ignored) continue;
    const role = node.role?.value ?? '';
    const name = node.name?.value ?? '';
    if (!role || !node.backendDOMNodeId) continue;
    const interactive = INTERACTIVE_ROLES.has(role);
    const named = Boolean(name && name.trim());
    if (!interactive && !(named && ['heading', 'img'].includes(role))) continue;
    const flat = name.trim().replace(/\s+/g, ' ');
    const trimmedName = flat.length > MAX_NAME ? `${flat.slice(0, MAX_NAME - 1)}…` : flat;
    // Responsive layouts commonly duplicate whole nav/footer blocks (desktop +
    // mobile variants) — same role+name, different node. Keep the first
    // (typically the primary, DOM-earlier one) and drop exact repeats instead
    // of burning refs/tokens on look-alike entries.
    if (trimmedName) {
      const dupKey = `${role}|${trimmedName}`;
      if (seen.has(dupKey)) { duplicatesDropped++; continue; }
      seen.add(dupKey);
    }
    const level = role === 'heading' ? node.properties?.find((p) => p.name === 'level')?.value?.value : undefined;
    kept.push({ role, name: trimmedName, fullName: name.trim(), backendDOMNodeId: node.backendDOMNodeId, interactive, level });
  }

  // Document order; unnamed images add no signal.
  const useful = kept.filter((n) => n.interactive || n.name);
  const trimmed = useful.slice(0, MAX_REFS);
  const truncated = useful.length - trimmed.length;

  const refs = {};
  const lines = [];
  trimmed.forEach((node, i) => {
    const ref = `e${i + 1}`;
    refs[ref] = { backendDOMNodeId: node.backendDOMNodeId, role: node.role, name: node.fullName };
    const role = node.role === 'heading' ? `h${node.level ?? ''}`.replace(/^h$/, 'heading') : node.role;
    lines.push(`[${ref}] ${role}${node.name ? ` "${node.name}"` : ''}`);
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

  const title = (await cdp.send('Runtime.evaluate', { expression: 'document.title', returnByValue: true }).catch(() => null))?.result?.value ?? '';
  console.log(`[PAGE] "${String(title).slice(0, 120)}" ${cdp.targetInfo.url}`);
  console.log(`[METRIC] SNAPSHOT refs=${trimmed.length} totalAxNodes=${nodes.length}${duplicatesDropped ? ` duplicatesDropped=${duplicatesDropped}` : ''}${truncated ? ` truncated=${truncated} (raise SNAPSHOT_MAX)` : ''}`);
  if (process.env.SNAPSHOT_STDOUT !== 'summary') {
    for (const line of lines) console.log(`[SNAPSHOT] ${line}`);
  }
  console.log(`[ARTIFACT] PAGE_SNAPSHOT ${artifactPath}`);

  if (TEXT_CHARS > 0) {
    const text = (await cdp.send('Runtime.evaluate', {
      returnByValue: true,
      expression: `((document.querySelector('main,[role=main],article') || document.body)?.innerText || '').replace(/\\s+/g, ' ').trim()`,
    }).catch(() => null))?.result?.value ?? '';
    const textPath = join(cdp.outputDir, 'page-text.txt');
    writeFileSync(textPath, `${text}\n`, { mode: 0o600 });
    console.log(`[TEXT] ${text.slice(0, TEXT_CHARS)}${text.length > TEXT_CHARS ? `… (+${text.length - TEXT_CHARS} chars)` : ''}`);
    console.log(`[ARTIFACT] PAGE_TEXT ${textPath}`);
  }
  if (process.env.SNAPSHOT_STDOUT !== 'summary') console.log('[REASON] Use a verified ref from this snapshot with dom-operations-check.mjs; confirm current state before acting.');
}
