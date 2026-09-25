#!/usr/bin/env node
import { appendFile, readFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { runLocalSemantic } from './local-semantic.mjs';

const { values } = parseArgs({ options: {
  path: { type: 'string' }, question: { type: 'string' }, questions: { type: 'string' },
  mode: { type: 'string', default: 'scout' }, pattern: { type: 'string' },
  view: { type: 'string', default: 'none' },
  'section-pattern': { type: 'string' },
  'rank-by': { type: 'string' }, label: { type: 'string' },
  names: { type: 'string', multiple: true }, 'max-files': { type: 'string', default: '25' },
  'max-calls': { type: 'string', default: '80' }, top: { type: 'string', default: '5' },
  dedupe: { type: 'boolean' }, out: { type: 'string' }, help: { type: 'boolean', short: 'h' }
} });
if (values.help) {
  process.stdout.write(`Private local semantic discovery POC — real local MCP, no new public tool.
Usage: node src/local-semantic-cli.mjs --path DIR --question TEXT [options]
  --mode scout|lexical|rerank  Full-content screening or search baselines
  --pattern REGEX            Required for lexical/rerank; use sensible synonyms
  --view none|symbols        Scout source or headings/signatures; default none
  --section-pattern REGEX   Scout complete Markdown sections selected by headings
  --questions FILE           JSON array of typed Clasify questions instead of --question
  --rank-by ID --label NAME   Ranking question; Choice requires a probability label
  --names GLOB               Repeatable basename filter; default *.md
  --max-files N              Scan bound, default 25; omitted files stay explicit
  --max-calls N              MCP call budget, default 80
  --top N                    Concise selected list, default 5; full scores saved with --out
  --dedupe                   Content-only questions: hash identical files, classify once
  --out FILE                 Save complete trace, page verdicts, schemas and metrics
Scores route reads, never establish absence. Full scans have provider cost proportional
to the captured corpus. MCP enforces ordinary file access and sanitization.
`);
} else {
  const client = new Client({ name: 'local-semantic-poc', version: '1' });
  try {
    if (!values.path || (!values.question && !values.questions) || (values.question && values.questions)) throw new Error('provide --path and exactly one of --question or --questions');
    const questions = values.questions ? JSON.parse(await readFile(values.questions, 'utf8')) : [{ id: 'relevant', question: { type: 'noul', instructions: `Could this content contribute a concrete fact, constraint, or counterexample to this research question? ${values.question} A mere mention is insufficient; partial useful evidence counts.` } }];
    if (values.out) {
      await mkdir(dirname(resolve(values.out)), { recursive: true });
      await writeFile(`${values.out}.jsonl`, '');
    }
    const workspace = fileURLToPath(new URL('../../../', import.meta.url));
    await client.connect(new StdioClientTransport({ command: process.execPath, args: [resolve(workspace, 'packages/octocode-mcp/dist/index.js')], cwd: workspace, env: { ...process.env, OCTOCODE_STORAGE_MODE: 'memory' }, stderr: 'pipe' }));
    const catalog = await client.listTools();
    const result = await runLocalSemantic(client, { path: values.path, mode: values.mode, view: values.view, pattern: values.pattern, sectionPattern: values['section-pattern'], questions, rankBy: values['rank-by'], label: values.label, names: values.names, maxFiles: Number(values['max-files']), maxCalls: Number(values['max-calls']), top: Number(values.top), dedupe: values.dedupe, onCall: values.out ? receipt => appendFile(`${values.out}.jsonl`, `${JSON.stringify(receipt)}\n`) : undefined });
    if (values.out) {
      await mkdir(dirname(resolve(values.out)), { recursive: true });
      await writeFile(values.out, JSON.stringify({ ...result, catalog, serverInstructions: client.getInstructions() }, null, 2));
    }
    const selected = result.selected.map(file => ({ path: relative(result.root, file.path), value: file.bestPageValue ?? file.score, coverage: file.coverage, ...(file.anchors ? { anchors: file.anchors } : {}), ...(file.read?.query.startLine ? { lines: [file.read.query.startLine, file.read.query.endLine] } : {}) }));
    const ranking = { question: result.rankBy, type: questions.find(q => q.id === result.rankBy)?.question.type, ...(result.label ? { label: result.label } : {}) };
    process.stdout.write(`${JSON.stringify({ mode: result.mode, view: result.view, ranking, discoveryComplete: result.discoveryComplete, discoveredTotal: result.discoveredTotal, unscannedFiles: result.unscannedFiles, scannedFiles: result.scannedFiles, classifiedFiles: result.classifiedFiles, unresolved: result.unresolved?.map(r => ({ path: relative(result.root, r.path), reason: r.error })), selected, metrics: result.metrics, limitations: result.limitations }, null, 2)}\n`);
  } catch (error) {
    process.stderr.write(`local-semantic-poc: ${error.message}\n`); process.exitCode = 1;
  } finally { await client.close(); }
}
