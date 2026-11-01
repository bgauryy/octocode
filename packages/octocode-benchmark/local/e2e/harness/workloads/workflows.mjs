// Research workflows (docs/OCTOCODE_WORKFLOWS.md): each step runs the typed
// call the previous response emitted, never a hand-built follow-up, so a chain
// measures what an agent following the tool's own leads pays.
import fs from 'node:fs';
import path from 'node:path';
import { REPOS } from '../paths.mjs';
import { GRAMMARS, emptyDir, frequentIdentifier, largestSource } from './cases.mjs';

/** Typed calls of one kind anywhere in a response, in document order. */
export function emitted(value, kind, tool) {
  const out = [];
  const walk = node => {
    if (!node || typeof node !== 'object') return;
    if (node.kind === kind && node.tool === tool && node.input) out.push(node.input);
    for (const child of Object.values(node)) walk(child);
  };
  walk(value);
  return out;
}

/** Run `first`, then each `[kind, tool]` lead, taken from the latest response that emitted one. */
function chain(first, leads) {
  return async call => {
    const steps = [];
    const responses = [await call(first.tool, first.args)];
    steps.push({ tool: first.tool, ...responses[0].stat });
    for (const [kind, tool] of leads) {
      const input = responses.toReversed().flatMap(r => emitted(r.data, kind, tool))[0];
      if (!input) { steps.push({ tool, missing: `ended: no ${kind} ${tool} lead emitted` }); break; }
      responses.push(await call(tool, input));
      steps.push({ tool, ...responses.at(-1).stat });
    }
    return steps;
  };
}

const q = query => ({ queries: [query] });

function localChains() {
  const cases = [];
  for (const grammar of ['typescript', 'rust', 'go', 'python', 'java']) {
    const g = GRAMMARS[grammar];
    const root = path.join(REPOS, g.repo);
    if (!fs.existsSync(root)) continue;
    // A term the code itself uses: the most frequent identifier of its largest source file.
    const source = largestSource(root, g.ext);
    if (!source) continue;
    const term = frequentIdentifier(fs.readFileSync(path.join(root, source.rel), 'utf8'));
    cases.push({
      group: 'workflow', name: `local ${grammar}: search → fetch → lsp`, cwd: root, meta: { term },
      chain: chain({ tool: 'localSearch', args: q({ path: root, matchString: [term], include: g.ext.map(e => `*.${e}`) }) },
        [['related', 'localFetch'], ['related', 'lspSearch']]),
    });
  }
  return cases;
}

function githubChains() {
  return [
    { name: 'github: code search → file', first: { tool: 'ghSearchCode', args: q({ owner: 'facebook', repo: 'react', keywords: ['useEffect'] }) }, leads: [['related', 'ghGetFileContent']] },
    { name: 'github: history → item', first: { tool: 'ghSearchHistory', args: q({ operation: 'pullRequest', owner: 'facebook', repo: 'react', keywords: ['useEffect'] }) }, leads: [['related', 'ghGetHistoryItem']] },
    { name: 'github: package → tree', first: { tool: 'artifactSearch', args: q({ operation: 'resolve', ecosystem: 'npm', packageName: 'react' }) }, leads: [['related', 'ghStructure']] },
    { name: 'github: code search → next page', first: { tool: 'ghSearchCode', args: q({ owner: 'facebook', repo: 'react', keywords: ['useEffect'] }) }, leads: [['continue', 'ghSearchCode']] },
  ].map(({ name, first, leads }) => ({ group: 'workflow', name, cwd: emptyDir(), chain: chain(first, leads) }));
}

export function workflowCases() {
  return [...localChains(), ...githubChains()];
}
