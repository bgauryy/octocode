#!/usr/bin/env node
// verify-facts.mjs — Re-fetch every file path mentioned in EXPECTED_FACTS.md
// and report any 404s. Drift detector. Needs $GH_TOKEN.
// Usage: node verify-facts.mjs
import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const here  = dirname(fileURLToPath(import.meta.url));
const facts = readFileSync(join(here, '..', 'EXPECTED_FACTS.md'), 'utf8');

const repos = {
  react:        'facebook/react',          nextjs:    'vercel/next.js',
  'react-router':'remix-run/react-router',  tanrouter: 'TanStack/router',
  tanquery:     'TanStack/query',          zustand:   'pmndrs/zustand',
  jotai:        'pmndrs/jotai',            rtk:       'reduxjs/redux-toolkit',
  nuxt:         'nuxt/nuxt',               recoil:    'facebookexperimental/Recoil',
  vue:          'vuejs/core',              svelte:    'sveltejs/svelte',
  solid:        'solidjs/solid',           vite:      'vitejs/vite',
};
const pathHint = {
  'ReactHooks.js':'react','ReactFiberThrow.js':'react','ReactFiberHooks.js':'react',
  'ReactFiberWorkLoop.js':'react','ReactDOMFizzServerBrowser.js':'react',
  'app-router-context':'nextjs','app-render.tsx':'nextjs','dynamic-rendering.ts':'nextjs',
  'config-schema.ts':'nextjs','stream-ops.web.ts':'nextjs',
  'history.ts':'react-router','packages/history/src/index.ts':'tanrouter',
  'vanilla.ts':'zustand','sources.js':'svelte','vIf.ts':'vue','renderer.ts':'vue','dep.ts':'vue',
  'signal.ts':'solid','useBaseQuery.ts':'tanquery',
};

const paths = [...new Set(
  [...facts.matchAll(/`([a-zA-Z0-9_./-]+\.(?:js|ts|tsx|md))`/g)]
    .map(m => m[1])
    .filter(p => p.includes('/'))
)];

const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
if (!token) { console.error('verify-facts: set $GH_TOKEN or $GITHUB_TOKEN'); process.exit(1); }

const guess = (p) => {
  for (const [k, repo] of Object.entries(pathHint)) if (p.includes(k)) return repos[repo];
  return null;
};

let ok = 0, miss = 0;
for (const p of paths) {
  const repo = guess(p);
  if (!repo) { console.log(`skip   ${p}  (repo unknown)`); continue; }
  const url = `https://api.github.com/repos/${repo}/contents/${p}`;
  const r = await fetch(url, { headers: { Authorization: `Bearer ${token}`, 'User-Agent': 'verify-facts' } });
  if (r.ok) { ok++; console.log(`ok     ${repo}  ${p}`); }
  else      { miss++; console.log(`MISS   ${repo}  ${p}  (${r.status})`); }
}
console.log(`\nverified ${ok}, missing ${miss}, skipped ${paths.length - ok - miss}`);
process.exit(miss ? 1 : 0);
