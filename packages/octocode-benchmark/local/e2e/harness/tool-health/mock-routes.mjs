// Request routing of the tool-health mock: the jev classification provider, the
// npm registry, GitHub GraphQL, then GitHub REST (mock-routes-github.mjs).
// `r` = { method, url, p, body, headers, send, model, base, classification }.
import { DATE, sha } from './mock-model.mjs';
import { graphqlAnswer } from './mock-graphql.mjs';
import { SEARCH_ROUTES, repoRoute } from './mock-routes-github.mjs';

/** A jev System One answer per requested question, shaped by the question's type. */
function jevAnswer(question) {
  if (question.type === 'noul') return { type: 'noul', noul: 0.9 };
  if (question.type === 'choice') {
    const labels = Object.keys(question.criteria ?? {});
    const rest = labels.length > 1 ? 0.2 / (labels.length - 1) : 0;
    const probabilities = Object.fromEntries(labels.map((l, i) => [l, i === 0 ? (labels.length > 1 ? 0.8 : 1) : rest]));
    return { type: 'choice', choice: labels[0], confidence: probabilities[labels[0]], probabilities };
  }
  if (question.type === 'score') {
    const levels = question.criteria ?? [];
    const top = levels.length - 1;
    const probabilities = Object.fromEntries(levels.map((_, i) => [String(i), i === top ? 1 : 0]));
    const legend = Object.fromEntries(levels.map((l, i) => [String(i), l]));
    return { type: 'score', score: top, confidence: 1, probabilities, legend };
  }
  if (question.type === 'box') return { type: 'box', box: { top: 0.1, bottom: 0.9, left: 0.2, right: 0.8 }, confidence: null };
  return { type: question.type };
}

/** Build provider answers without applying tool-side classification policy. */
export function classificationAnswer(request, answerFor = jevAnswer) {
  const answers = Object.fromEntries(Object.entries(request.questions ?? {}).map(([id, question]) => [id, answerFor(question, id)]));
  return { model: request.model || 'jev-mock', answers, usage: { input_tokens: 10, output_tokens: 2 } };
}

async function classificationRoute(r) {
  if (r.method === 'GET' && r.p === '/v1/models') {
    r.send(200, { models: [
      { name: 'jev-latest', description: 'Current stable fixture model', release_date: '2026-01-01' },
      { name: 'jev-1.14.0', description: 'Image and box fixture model', release_date: '2026-02-01' },
    ] });
    return true;
  }
  if (r.method !== 'POST' || r.p !== '/v1/systemone') return false;
  const request = JSON.parse(r.body || '{}');
  r.classification.requests.push(request);
  const token = /^(?:token|bearer)\s+(.+)$/i.exec(r.headers.authorization ?? '')?.[1];
  const responder = r.classification.responders?.get(token) ?? r.classification.responder;
  if (responder) {
    const response = await responder(request, r.headers);
    r.send(response.status ?? 200, response.body);
    return true;
  }
  r.send(200, classificationAnswer(request));
  return true;
}

function npmSearchRoute(r) {
  if (r.method !== 'GET' || r.p !== '/-/v1/search') return false;
  const params = r.url.searchParams;
  const text = (params.get('text') ?? '').toLowerCase().split(/\s+/).filter(Boolean);
  const size = Number(params.get('size') ?? 20), from = Number(params.get('from') ?? 0);
  const hits = r.model.packages.filter(pkg => text.every(t => pkg.name.includes(t) || pkg.description.toLowerCase().includes(t) || pkg.keywords.includes(t)));
  r.send(200, {
    objects: hits.slice(from, from + size).map(pkg => ({
      package: { name: pkg.name, version: pkg.version, description: pkg.description, keywords: pkg.keywords, date: DATE(5), links: { npm: `https://www.npmjs.com/package/${pkg.name}`, repository: 'https://github.com/octo/demo' }, publisher: { username: 'octo-dev' } },
      score: { final: 0.5, detail: { quality: 0.5, popularity: 0.5, maintenance: 0.5 } }, searchScore: 1,
    })),
    total: hits.length, time: DATE(6),
  });
  return true;
}

const REPOSITORY = { type: 'git', url: 'git+https://github.com/octo/demo.git' };

function npmPackageRoute(r) {
  const m = r.method === 'GET' && /^\/((?:@[^/]+\/)?[^/@]+)(?:\/([^/]+))?$/.exec(r.p);
  const pkg = m && r.model.packages.find(x => x.name === m[1]);
  if (!pkg) return false;
  const manifest = version => ({
    name: pkg.name, version, description: pkg.description, license: 'MIT', homepage: 'https://github.com/octo/demo#readme', repository: REPOSITORY, dependencies: pkg.dependencies, keywords: pkg.keywords,
    dist: { tarball: `${r.base}/${pkg.name}/-/${pkg.name}-${version}.tgz`, shasum: sha(version), integrity: `sha512-${Buffer.from(sha(version)).toString('base64')}` }, gitHead: r.model.head.oid,
  });
  if (m[2]) {
    const version = m[2] === 'latest' ? pkg.version : m[2];
    if (pkg.versions.includes(version)) r.send(200, manifest(version));
    else r.send(404, { error: 'version not found' });
    return true;
  }
  r.send(200, {
    name: pkg.name, description: pkg.description, 'dist-tags': { latest: pkg.version }, versions: Object.fromEntries(pkg.versions.map(v => [v, manifest(v)])),
    time: Object.fromEntries(pkg.versions.map((v, i) => [v, DATE(i)])), license: 'MIT', repository: REPOSITORY,
  });
  return true;
}

/** GitHub GraphQL (`<host>/api/graphql` beside an `/api/v3` REST base). */
function graphqlRoute(r) {
  if (r.method !== 'POST' || (r.p !== '/api/graphql' && r.p !== '/graphql')) return false;
  const { query = '', variables = {} } = JSON.parse(r.body || '{}');
  const answer = graphqlAnswer(r.model, query, variables);
  if (!answer) return false;
  r.send(200, answer);
  return true;
}

function restRoute(r) {
  if (r.method !== 'GET') return false;
  const search = SEARCH_ROUTES[r.p];
  return search ? search(r) : repoRoute(r);
}

const ROUTES = [classificationRoute, npmSearchRoute, npmPackageRoute, graphqlRoute, restRoute];

/** Answer `r` with the first route that handles it; false when none does. */
export async function route(r) {
  for (const handle of ROUTES) if (await handle(r)) return true;
  return false;
}
