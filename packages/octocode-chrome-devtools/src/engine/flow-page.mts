import { capturePath } from './capture-path.mjs';
import { chromeOutputLimits as limits } from './chrome-contract.mjs';
import { readFileSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const execute = promisify(execFile);
const parse = (text: string) =>
  JSON.parse(text, (_key, value, context) =>
    typeof value === 'number' && context?.source
      ? JSON.rawJSON(context.source)
      : value
  );
const query = (
  file: string,
  pointer = '',
  cursor = 0,
  limit: number = limits.extractionRows,
  workspace = process.cwd()
) => ({
  tool: 'query',
  query: {
    file: capturePath(file, workspace),
    limit,
    cursor,
    ...(pointer ? { pointer } : {}),
  },
});
const argumentsFor = (input: any) =>
  Object.entries(input).flatMap(([key, value]) => ['--' + key, String(value)]);
export const readerRoute = (
  args: string[],
  tool: string,
  workspace: string
) => {
  const query: any = {};
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].slice(2),
      value = args[i + 1];
    query[key] = ['cursor', 'limit', 'offset', 'length'].includes(key)
      ? Number(value)
      : ['where', 'select'].includes(key)
        ? JSON.parse(value)
        : value;
    // Preserve numeric predicate lexemes in legacy args instead of rounding them
    // while converting a continuation to typed JSON inputs.
    if (key === 'where') {
      let numeric = false;
      JSON.parse(value, (_key, item) => {
        if (typeof item === 'number') numeric = true;
        return item;
      });
      if (numeric) {
        const exact = [...args];
        const fileAt = exact.indexOf('--file') + 1;
        if (fileAt) exact[fileAt] = capturePath(exact[fileAt], workspace);
        return { tool, query: { args: exact } };
      }
    }
  }
  if (query.file) query.file = capturePath(query.file, workspace);
  return { tool, query };
};

// Read pages through the existing evidence engine: one index and one pagination contract.
export async function flowPage(
  file: string,
  readNext: (file: string, format?: string, length?: number) => any,
  queryScript: string,
  workspace = process.cwd()
) {
  const pageQuery = (
    file: string,
    pointer = '',
    cursor = 0,
    limit: number = limits.extractionRows
  ) => query(file, pointer, cursor, limit, workspace);
  const flow = JSON.parse(readFileSync(file, 'utf8'));
  if (!Array.isArray(flow.steps)) throw Error('Browser result needs steps');
  const steps: any[] = [];
  const sources: any[] = [];
  for (const source of flow.steps) {
    if (steps.length >= limits.steps) break;
    // Completed input operations need acknowledgement, not an echoed event/target record.
    if (
      source.status === 'complete' &&
      !source.condition &&
      ['act', 'wait', 'waitEvent', 'listen'].includes(source.op)
    )
      continue;
    const { artifact, startedAt, finishedAt, source: origin, ...row } = source;
    if (origin) {
      const { capturedAt, ...location } = origin;
      const identity = {
        ...location,
        targetId: row.targetId,
        ...(row.sessionId ? { sessionId: row.sessionId } : {}),
      };
      delete row.targetId;
      delete row.sessionId;
      let id = sources.findIndex(
        item => JSON.stringify(item) === JSON.stringify(identity)
      );
      if (id < 0) {
        id = sources.length;
        sources.push(
          Buffer.byteLength(JSON.stringify(identity)) <= limits.cdpBytes
            ? identity
            : { next: pageQuery(file, '/steps', flow.steps.indexOf(source), 1) }
        );
      }
      row.source = id;
      row.capturedAt = capturedAt;
    }
    if (
      artifact &&
      row.status === 'complete' &&
      ['extract', 'cdp', 'protocol'].includes(row.op)
    ) {
      if (row.op === 'extract') {
        const next = pageQuery(artifact);
        const result = await execute(
          process.execPath,
          [queryScript, ...argumentsFor(next.query)],
          { maxBuffer: 1024 * 1024, timeout: 10000, cwd: workspace }
        );
        const page = parse(result.stdout);
        delete page.index;
        delete page.indexReused;
        if (page.next?.continue)
          page.next = readerRoute(
            page.next.continue.args.slice(1),
            'query',
            workspace
          );
        if (page.next) page.next.query.limit = limits.continuationRows;
        for (const item of page.rows)
          if (item.next?.continue)
            item.next = readerRoute(
              item.next.continue.args.slice(1),
              'artifact',
              workspace
            );
        row.data =
          Buffer.byteLength(JSON.stringify(page)) <= limits.extractionBytes
            ? page
            : { next: pageQuery(artifact, '', 0, limits.continuationRows) };
      } else {
        const text = readFileSync(artifact, 'utf8');
        row.data =
          Buffer.byteLength(text) <= limits.cdpBytes
            ? parse(text)
            : { next: readNext(artifact, 'json', 1000) };
        if (row.data && !Object.keys(row.data).length) delete row.data;
      }
    } else if (
      artifact &&
      (row.status !== 'complete' || !['act', 'wait', 'goto'].includes(row.op))
    )
      row.next = readNext(artifact);
    if (
      row.status === 'complete' &&
      row.op === 'cdp' &&
      !row.data &&
      !row.condition
    )
      continue;
    if (Buffer.byteLength(JSON.stringify(row)) > limits.stepBytes) {
      for (const key of Object.keys(row))
        if (!['index', 'op', 'status'].includes(key)) delete row[key];
      row.next = pageQuery(file, '/steps', flow.steps.indexOf(source), 1);
    }
    // Never skip a step when its page would exceed the window.
    if (
      steps.length &&
      Buffer.byteLength(JSON.stringify([...steps, row])) > limits.stepBytes
    )
      break;
    steps.push(row);
  }
  return {
    flow: {
      coverage: flow.coverage,
      ...(flow.eventCoverage?.length
        ? {
            events: flow.eventCoverage
              .slice(0, limits.steps)
              .map(({ artifact, ...entry }: any) => entry),
          }
        : {}),
      ...(sources.length ? { sources } : {}),
      requestedSteps: flow.requestedSteps,
      completedSteps: flow.completedSteps,
      ...(flow.failure
        ? {
            failure:
              Buffer.byteLength(JSON.stringify(flow.failure)) <= limits.cdpBytes
                ? flow.failure
                : { next: readNext(file, 'json', 1000) },
          }
        : {}),
      steps,
    },
    next: {
      steps: pageQuery(file, '/steps', 0, limits.steps),
      ...(flow.eventCoverage?.length > limits.steps
        ? {
            events: pageQuery(
              file,
              '/eventCoverage',
              limits.steps,
              limits.steps
            ),
          }
        : {}),
    },
  };
}
