import { createHash } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { isAbsolute, relative, resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { selectMarkdownSections } from './sections.mjs';

const bytes = value => Buffer.byteLength(JSON.stringify(value));
const envelope = action => action.query.queries ? action.query : { queries: [action.query] };

export function answerValue(answer, label) {
  if (typeof answer?.noul === 'number') return answer.noul;
  if (typeof answer?.score === 'number') return answer.score;
  if (label && typeof answer?.probabilities?.[label] === 'number') return answer.probabilities[label];
  return null;
}

export function rankResources(resources, questionId, label) {
  return resources.map(resource => {
    const pages = resource.pages.map(page => ({ page, value: answerValue(page.answers?.[questionId], label) }));
    const ranked = pages.filter(p => p.value !== null).sort((a, b) => b.value - a.value);
    const best = ranked[0];
    const sourceView = resource.view !== 'symbols';
    const focus = sourceView ? best?.page.focus?.[questionId] : undefined;
    const scope = sourceView ? (focus ?? best?.page.scope) : undefined;
    const ranges = scope?.lineRanges ?? (Number.isInteger(scope?.startLine) && Number.isInteger(scope?.endLine)
      ? [{ startLine: scope.startLine, endLine: scope.endLine }] : [{}]);
    const reads = ranges.map(range => ({ tool: 'localFetch', query: {
      path: resource.path, ...range, minify: 'none', reasoning: 'Verify this semantic lead against exact source.'
    } }));
    return {
      path: resource.path,
      coverage: resource.coverage,
      ...(resource.selectionScope ? { selectionScope: resource.selectionScope } : {}),
      view: resource.view ?? 'none',
      pages: resource.pages.length,
      bestPageValue: best?.value ?? null,
      answers: best?.page.answers ?? {},
      ...(best?.page.source ? { source: best.page.source } : {}),
      ...(scope?.lineRanges ? { lineRanges: scope.lineRanges } : {}),
      ...(resource.duplicateOf ? { duplicateOf: resource.duplicateOf } : {}),
      ...(resource.error ? { error: resource.error } : {}),
      read: reads[0], reads
    };
  }).sort((a, b) => (b.bestPageValue ?? -Infinity) - (a.bestPageValue ?? -Infinity) || a.path.localeCompare(b.path));
}

/** Private experiment: discovery and classification use the real MCP runtime. */
export async function runLocalSemantic(client, options) {
  const root = await realpath(options.path);
  const mode = options.mode ?? 'scout';
  const view = options.view ?? 'none';
  const maxFiles = options.maxFiles ?? 25;
  const maxCalls = options.maxCalls ?? 80;
  const top = options.top ?? 5;
  const questions = options.questions;
  if (!['scout', 'lexical', 'rerank'].includes(mode)) throw new Error('mode must be scout, lexical, or rerank');
  if (!['none', 'symbols'].includes(view)) throw new Error('view must be none or symbols');
  if (options.sectionPattern && (mode !== 'scout' || view !== 'none')) throw new Error('sectionPattern requires Scout with original source view');
  for (const [key, value] of Object.entries({ maxFiles, maxCalls, top })) {
    if (!Number.isInteger(value) || value < 1) throw new Error(`${key} must be a positive integer`);
  }
  if (!Array.isArray(questions) || questions.length < 1 || questions.length > 5) throw new Error('provide 1–5 questions');
  if (mode !== 'scout' && !options.pattern) throw new Error('lexical/rerank requires pattern');
  const rankBy = options.rankBy ?? questions[0].id;
  const rankedQuestion = questions.find(q => q.id === rankBy);
  if (!rankedQuestion) throw new Error('rankBy must identify an input question');
  if (rankedQuestion.question.type === 'choice' && !options.label) throw new Error('Choice ranking requires an explicit label');
  const calls = [];
  const started = performance.now();
  const metrics = { calls: 0, requestBytes: 0, responseBytes: 0, structuredBytes: 0, providerInputTokens: 0, providerOutputTokens: 0, providerUsageMissing: 0, hashBytes: 0 };
  async function call(name, args) {
    if (calls.length >= maxCalls) throw new Error('MCP call budget exhausted; narrow the directory');
    const begin = performance.now();
    const raw = await client.callTool({ name, arguments: args }, undefined, { timeout: 180000 });
    const data = raw.structuredContent;
    calls.push({ name, args, ms: Math.round(performance.now() - begin), response: raw });
    await options.onCall?.(calls.at(-1));
    metrics.calls++; metrics.requestBytes += bytes(args); metrics.responseBytes += bytes(raw);
    metrics.structuredBytes += bytes(data ?? null);
    if (raw.isError || !data) throw new Error(`MCP ${name} failed: ${JSON.stringify(raw)}`);
    for (const query of data.queries ?? []) {
      if (query.usage?.input_tokens == null || query.usage?.output_tokens == null) metrics.providerUsageMissing++;
      else { metrics.providerInputTokens += query.usage.input_tokens; metrics.providerOutputTokens += query.usage.output_tokens; }
    }
    return data;
  }
  const finish = result => ({
    version: 2, mode, view, root, questions, rankBy, label: options.label,
    ...result, metrics: { ...metrics, elapsedMs: Math.round(performance.now() - started) }, calls,
    limitations: [
      'Development retrieval experiment, not an agent quality benchmark.',
      'bestPageValue is a maximum page judgment, not a calibrated file probability; longer files have more chances to score.',
      'No low score establishes absence. Partial, errored, and unscanned files remain unresolved.',
      'Follow only focus keyed to the ranking question; missing focus requires the source scope or another exact lookup.',
      ...(view === 'symbols' ? ['Outline judgments cover headings/signatures only. Their view positions must not be replayed as source lines.'] : []),
      'Response bytes measure MCP transport, not model tokens. Provider tokens are separate.',
      ...(options.dedupe ? ['Exact byte deduplication assumes a content-only question; identical bytes at different paths need not have identical repository meaning. Relevance scores cannot establish novelty or near-duplicate equivalence.'] : [])
    ]
  });

  if (mode !== 'scout') {
    const response = await call('localSearch', { queries: [{
      path: root, searchText: options.pattern, regex: 'rust', include: options.names ?? ['*.md'],
      resultView: 'paginated', pageSize: Math.min(maxFiles, 8),
      maxMatchesPerFile: 3, contextLines: 2, matchContentLength: 500,
      reasoning: 'Locate useful documentation using a lexical anchor and bounded snippets.',
      ...(mode === 'rerank' ? { semanticRerank: { questions: [{ id: rankedQuestion.id, question: rankedQuestion.question.instructions }] } } : {})
    }], responseCharLength: 50000 });
    const row = response.results?.[0];
    if (row?.status === 'error' || !row?.data) throw new Error(JSON.stringify(row ?? response));
    const data = row.data;
    const candidates = (data.files ?? []).map((file, index) => ({
      path: resolve(response.base ?? root, file.path),
      ...(mode === 'rerank' ? { score: data.semanticRerank?.candidates?.[index]?.score } : {}),
      anchors: (file.matches ?? []).map(m => m.line)
    }));
    if (mode === 'rerank' && data.semanticRerank?.usage) {
      metrics.providerInputTokens += data.semanticRerank.usage.input_tokens ?? 0;
      metrics.providerOutputTokens += data.semanticRerank.usage.output_tokens ?? 0;
    }
    return finish({ candidates, selected: candidates.slice(0, top), search: data, discoveryComplete: !data.pagination?.hasMore, retainedContinuation: data.semanticRerank?.next ?? data.next });
  }

  let args = { queries: [{ operation: 'files', path: root, names: options.names ?? ['*.md'], entryType: 'f', detail: 'full', sort: 'path', pageSize: 100, reasoning: 'Discover file identities without loading source bodies into the agent context.' }], responseCharLength: 50000 };
  const files = [];
  let discoveryComplete;
  let discoveredTotal = null;
  let discoveryNext;
  while (true) {
    const response = await call('astSearch', args);
    const row = response.results?.[0];
    if (row?.status === 'error' || !row?.data) throw new Error(JSON.stringify(row ?? response));
    const data = row.data;
    discoveredTotal = data.pagination?.totalFiles ?? discoveredTotal;
    for (const file of data.files ?? []) {
      if (files.length >= maxFiles) break;
      const path = resolve(response.base ?? root, file.path);
      const actual = await realpath(path);
      const rel = relative(root, actual);
      if (rel.startsWith('..') || isAbsolute(rel)) throw new Error('Discovered file escapes requested root');
      files.push({ path, size: file.size, lines: file.lineCount });
    }
    discoveryNext = data.next?.nextPage;
    if (!discoveryNext || files.length >= maxFiles) {
      discoveryComplete = !discoveryNext && (discoveredTotal === null || files.length >= discoveredTotal);
      break;
    }
    args = envelope(discoveryNext);
  }
  const representatives = [];
  const hashes = new Map();
  for (const file of files) {
    if (options.dedupe) {
      const body = await readFile(file.path); metrics.hashBytes += body.length;
      file.hash = createHash('sha256').update(body).digest('hex');
      const existing = hashes.get(file.hash);
      if (existing) { file.duplicateOf = existing.path; continue; }
      hashes.set(file.hash, file);
    }
    representatives.push(file);
  }
  const resources = new Map();
  const regions = [];
  for (const file of representatives) {
    if (!options.sectionPattern) { regions.push(file); continue; }
    let next = { tool: 'localFetch', query: { path: file.path, minify: 'symbols', fullContent: true, reasoning: 'Choose complete sections from source-labeled headings before semantic screening.' } };
    let outline = '';
    let totalLines;
    while (next) {
      const response = await call(next.tool, { ...envelope(next), responseCharLength: 50000 });
      const row = response.results?.[0];
      if (row?.status === 'error' || row?.data?.contentView !== 'symbols') throw new Error('Cannot select sections without a source-labeled outline');
      outline += `${row.data.content}\n`; totalLines = row.data.totalLines;
      next = row.data.next?.continue;
    }
    const sections = selectMarkdownSections(outline, totalLines, options.sectionPattern);
    if (!sections.length) resources.set(file.path, { path: file.path, view, pages: [], coverage: 'unscanned', error: 'No heading matched the selection; body relevance remains unresolved' });
    for (const section of sections) regions.push({ ...file, section });
  }
  const batchSize = Math.floor(25 / questions.length);
  for (let offset = 0; offset < regions.length; offset += batchSize) {
    const batch = regions.slice(offset, offset + batchSize);
    const ids = new Map(batch.map((file, index) => [`f${offset + index}`, file]));
    const pending = [{ id: `scan-${offset}`, reasoning: 'Choose useful unread source scopes for the research question; preserve uncertainty and verify selected leads.', resources: [...ids].map(([id, file]) => ({ id, context: { tool: 'localFetch', query: { path: file.path, ...(file.section ? { startLine: file.section.startLine, endLine: file.section.endLine } : {}), minify: view, reasoning: 'Screen this unread candidate through the ordinary sanitized file reader.' } } })), questions }];
    while (pending.length) {
      const current = pending.shift();
      const response = await call('clasify', current);
      if (!Array.isArray(response.queries)) throw new Error(`Unexpected clasify envelope: ${JSON.stringify(response)}`);
      for (const query of response.queries) {
        for (const resource of query.resources ?? []) {
          const file = ids.get(resource.resourceId);
          if (!file) throw new Error('Classification lost resource identity');
          const stored = resources.get(file.path) ?? { path: file.path, view, pages: [], coverage: 'unknown' };
          stored.pages.push(...(resource.pages ?? []));
          stored.regionCoverage ??= {};
          stored.regionCoverage[resource.resourceId] = resource.coverage;
          const coverages = Object.values(stored.regionCoverage);
          stored.coverage = coverages.every(c => c === 'complete') ? 'complete'
            : coverages.every(c => c === 'error') ? 'error' : 'partial';
          stored.selectionScope = options.sectionPattern ? 'selected-sections' : 'file';
          if (resource.error) stored.error = resource.error;
          if (stored.error || stored.pages.some(p => p.error)) stored.coverage = 'partial';
          resources.set(file.path, stored);
        }
        const next = query.next?.clasify;
        if (next) pending.push(next.query);
      }
      if (response.next?.clasify) pending.push(response.next.clasify.query);
    }
  }
  for (const file of representatives) {
    if (!resources.has(file.path)) resources.set(file.path, { path: file.path, view, pages: [], coverage: 'error', error: 'No resource result returned' });
  }
  for (const file of files) {
    if (!file.duplicateOf) continue;
    const source = resources.get(file.duplicateOf);
    const now = createHash('sha256').update(await readFile(file.path)).digest('hex');
    const original = createHash('sha256').update(await readFile(file.duplicateOf)).digest('hex');
    if (now !== file.hash || original !== file.hash) throw new Error('Duplicate changed during classification; rerun');
    resources.set(file.path, { ...source, path: file.path, duplicateOf: file.duplicateOf });
  }
  const candidates = rankResources([...resources.values()], rankBy, options.label);
  return finish({ discoveredTotal, discoveryComplete, discoveryNext, unscannedFiles: discoveredTotal === null ? null : Math.max(0, discoveredTotal - files.length), scannedFiles: files.length, classifiedFiles: new Set(regions.map(r => r.path)).size, selectedSections: regions.filter(r => r.section).map(r => ({ path: r.path, ...r.section })), unresolved: candidates.filter(r => r.bestPageValue === null || r.coverage !== 'complete'), candidates, selected: candidates.slice(0, top), resources: [...resources.values()] });
}
