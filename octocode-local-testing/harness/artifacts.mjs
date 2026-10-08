// artifactSearch fidelity: every registry answer is compared with that
// registry's own public API (ground truth), and the one source lead
// (hints.viewReleaseSource, else hints.viewRepo) must name and open the same repository.
import { checks, rowData, startServer, writeResults } from './mcp-client.mjs';

const { check, summary } = checks('artifacts');
const client = await startServer();
const { call, follow } = client;
const UA = { 'user-agent': 'octocode-local-testing (bench; contact: maintainers)' };

async function json(url) {
  const response = await fetch(url, { headers: UA, signal: AbortSignal.timeout(60000) });
  if (!response.ok) throw new Error(`${url} → ${response.status}`);
  return response.json();
}
async function text(url) {
  const response = await fetch(url, { headers: UA, signal: AbortSignal.timeout(60000) });
  if (!response.ok) throw new Error(`${url} → ${response.status}`);
  return response.text();
}
/** owner/repo from any GitHub URL form (git+https, .git, trailing paths). */
function githubSlug(url) {
  const match = String(url ?? '').match(/github\.com[/:]([^/]+)\/([^/#?]+)/i);
  return match ? `${match[1]}/${match[2].replace(/\.git$/, '')}`.toLowerCase() : null;
}

const CASES = [
  { ecosystem: 'npm', name: 'typescript', truth: async () => { const p = await json('https://registry.npmjs.org/typescript'); return { version: p['dist-tags'].latest, repo: githubSlug(p.repository?.url) }; } },
  { ecosystem: 'pypi', name: 'requests', truth: async () => { const p = await json('https://pypi.org/pypi/requests/json'); return { version: p.info.version, repo: githubSlug(Object.values(p.info.project_urls ?? {}).find(u => /github\.com/.test(u))) }; } },
  { ecosystem: 'crates', name: 'serde', truth: async () => { const p = await json('https://crates.io/api/v1/crates/serde'); return { version: p.crate.max_stable_version ?? p.crate.newest_version, repo: githubSlug(p.crate.repository) }; } },
  { ecosystem: 'maven', name: 'com.google.guava:guava', truth: async () => { const xml = await text('https://repo1.maven.org/maven2/com/google/guava/guava/maven-metadata.xml'); return { version: xml.match(/<release>([^<]+)<\/release>/)?.[1], repo: 'google/guava' }; } },
  { ecosystem: 'nuget', name: 'Newtonsoft.Json', truth: async () => { const p = await json('https://api.nuget.org/v3-flatcontainer/newtonsoft.json/index.json'); const stable = p.versions.filter(v => !v.includes('-')); return { version: stable.at(-1), repo: 'jamesnk/newtonsoft.json' }; } },
  { ecosystem: 'go', name: 'github.com/spf13/cobra', truth: async () => { const p = await json('https://proxy.golang.org/github.com/spf13/cobra/@latest'); return { version: p.Version, repo: 'spf13/cobra' }; } },
  { ecosystem: 'packagist', name: 'monolog/monolog', truth: async () => { const p = await json('https://repo.packagist.org/p2/monolog/monolog.json'); const v = p.packages['monolog/monolog'][0]; return { version: v.version, repo: githubSlug(v.source?.url) }; } },
  { ecosystem: 'rubygems', name: 'rails', truth: async () => { const p = await json('https://rubygems.org/api/v1/gems/rails.json'); return { version: p.version, repo: githubSlug(p.source_code_uri ?? p.homepage_uri) }; } },
];

const norm = v => String(v ?? '').replace(/^v/, '');
const table = [];
for (const c of CASES) {
  let truth;
  try {
    truth = await c.truth();
  } catch (error) {
    check(`${c.ecosystem}: registry ground truth reachable`, false, String(error.message));
    continue;
  }
  const out = await call('artifactSearch', { ecosystem: c.ecosystem, packageName: c.name });
  const data = rowData(out);
  const artifact = data?.artifacts?.[0];
  const toolRepo = githubSlug(artifact?.repository ?? artifact?.homepage ?? artifact?.sourceUrl);
  const leads = ['viewReleaseSource', 'viewRepo'].filter(name => data?.hints?.[name]);
  const viewHint = data?.hints?.[leads[0]];
  const view = viewHint?.query?.queries?.[0];
  const viewSlug = view ? `${view.owner}/${view.repo}`.toLowerCase() : null;
  table.push({ ecosystem: c.ecosystem, name: c.name, tool: artifact?.version, registry: truth.version, repo: toolRepo, truthRepo: truth.repo, lead: leads.join('+'), ref: view?.ref ?? '', path: view?.path ?? '' });
  check(`${c.ecosystem} ${c.name}: latest version equals the registry`, norm(artifact?.version) === norm(truth.version), `tool=${artifact?.version} registry=${truth.version}`);
  if (truth.repo) {
    check(`${c.ecosystem} ${c.name}: repository equals the registry's`, toolRepo === truth.repo, `tool=${toolRepo} registry=${truth.repo}`);
    check(`${c.ecosystem} ${c.name}: the source lead names that repository`, viewSlug === truth.repo, `${leads[0]}=${viewSlug}`);
  }
  if (view) {
    check(`${c.ecosystem}: one source lead`, leads.length === 1, leads.join(','));
    // AR2: the row alone carries `sourceRef` + `verification` ∈ {provenance, tag, registryRef}; leads do
    // not repeat the label. A release ref is `provenance`/`tag` when checked, else the registry's unchecked
    // claim `registryRef`; no ref → a `viewRepo` lead and no `sourceRef`.
    const refLabels = ['provenance', 'tag', 'registryRef'];
    const sourceLeads = ['viewReleaseSource', 'viewRepo', 'readManifest'].map(name => data?.hints?.[name]).filter(Boolean);
    const labelOk = viewHint.source === undefined && sourceLeads.every(lead => lead.verification === undefined)
      && (view.ref ? refLabels.includes(artifact?.verification) && artifact?.sourceRef === view.ref : leads[0] === 'viewRepo' && artifact?.sourceRef === undefined);
    check(`${c.ecosystem}: the row alone labels its source (AR2)`, labelOk, JSON.stringify({ leads: sourceLeads.map(l => l.verification), lead: viewHint, sourceRef: artifact?.sourceRef, verification: artifact?.verification }));
    // Replay the continuation verbatim (tool + query); an unpushed release ref recovers without ref.
    let tree = await follow(viewHint);
    if (tree.rowErrors && view.ref) {
      const { ref, ...recovery } = view;
      tree = await follow({ tool: viewHint.tool, query: { queries: [recovery] } });
    }
    check(`${c.ecosystem} ${c.name}: hints.${leads[0]} opens the repository`, !tree.isError && (rowData(tree)?.entries ?? []).length > 0, tree.text.slice(0, 120));
  }
}
console.table(table);

// Keyword discovery returns packages that match the keywords.
const discovery = rowData(await call('artifactSearch', { ecosystem: 'npm', keywords: ['yaml', 'parser'], pageSize: 10 }));
const rows = discovery?.artifacts ?? [];
const relevant = rows.filter(a => /yaml/i.test(`${a.name} ${a.description ?? ''}`)).length;
check('npm keyword discovery: results match the keywords', rows.length > 0 && relevant / rows.length >= 0.8, `${relevant}/${rows.length}`);

const result = summary();
writeResults('artifacts', { ...result, table });
client.close();
process.exitCode = result.failed.length ? 1 : 0;
