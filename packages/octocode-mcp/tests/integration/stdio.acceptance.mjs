/** Real built-server acceptance. Run after building CLI + MCP; no mocks or installs. */
import assert from 'node:assert/strict';
import { mkdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import {
  check,
  cliCalls,
  client,
  executeCliTool,
  fixture,
  receipt,
  state,
  transport,
  values,
} from './stdio-harness.mjs';
import { catalogChecks } from './stdio-catalog-checks.mjs';
import { liveChecks } from './stdio-live-checks.mjs';
import { localChecks } from './stdio-local-checks.mjs';
import { navigationChecks } from './stdio-navigation-checks.mjs';
import { paginationChecks } from './stdio-pagination-checks.mjs';

const { TOOL_NAMES } = await import('@octocodeai/config/schema');
const suiteStartedAt = performance.now();
client.onerror = error => receipt.transportErrors.push(error.name);
const runChecks = async checks => {
  for (const [name, fn] of checks) await check(name, fn);
};

const differingFields = (left, right, prefix = '', fields = []) => {
  if (fields.length >= 40) return fields;
  if (Object.is(left, right)) return fields;
  if (
    !left ||
    !right ||
    typeof left !== 'object' ||
    typeof right !== 'object'
  ) {
    fields.push(prefix || '$');
    return fields;
  }
  const keys = new Set([...Object.keys(left), ...Object.keys(right)]);
  for (const key of [...keys].sort())
    differingFields(left[key], right[key], `${prefix}/${key}`, fields);
  return fields;
};

let pid;
try {
  await client.connect(transport);
  pid = transport.pid;
  transport.stderr?.on('data', chunk => {
    receipt.stderrBytes += chunk.length;
    receipt.stderrTail = `${receipt.stderrTail}${chunk.toString('utf8')}`.slice(
      -65_536
    );
  });
  const list = await client.listTools();
  receipt.catalog = list.tools;
  state.list = list;
  state.expectedTools = list.tools.map(tool => tool.name);
  receipt.catalogBytes = Buffer.byteLength(JSON.stringify(receipt.catalog));
  await runChecks(catalogChecks());
  await runChecks(localChecks());
  if (!values.quick) {
    await runChecks(paginationChecks());
    await runChecks(navigationChecks());
  }
  if (values.live && !values.quick) await runChecks(liveChecks());
  if (values['cli-mcp-parity']) {
    await check(
      'deterministic same-query CLI and real MCP structured result parity',
      async () => {
        const parity = [];
        receipt.cliMcpParity = parity;
        const cacheVolatileTools = new Set([
          'ghSearchRepo',
          'ghSearchCode',
          'ghStructure',
          'ghGetFileContent',
          'ghSearchHistory',
          'ghGetHistoryItem',
          'artifactSearch',
        ]);
        const liveOnlyTools = cacheVolatileTools;
        for (const name of state.expectedTools) {
          if (name === TOOL_NAMES.CLASIFY) {
            const executionVerified = receipt.calls.some(
              call =>
                call.name === name &&
                call.response.isError === false &&
                call.response.structuredContent?.results?.[0]?.status !==
                  'error'
            );
            parity.push({
              name,
              status: 'not-applicable',
              comparison: 'exact-structured-results',
              executionVerified,
              reason:
                'Independent probabilistic responses need not be identical; executionVerified records the separate live provider check.',
            });
            assert.ok(
              !values.live || executionVerified,
              'clasify: --live requires one successful provider-backed execution'
            );
            continue;
          }
          const selected = receipt.calls.find(
            call =>
              call.name === name &&
              call.response.isError === false &&
              call.response.structuredContent?.results?.[0]?.status !==
                'error' &&
              call.response.structuredContent?.results?.[0]?.status !== 'empty'
          );
          if (
            !selected &&
            liveOnlyTools.has(name) &&
            (!values.live || values.quick)
          ) {
            parity.push({
              name,
              status: 'not-run',
              reason:
                'Successful provider-backed parity requires --live without --quick.',
            });
            continue;
          }
          assert.ok(
            selected,
            `${name}: no successful non-mutating MCP call to compare`
          );
          const cliResponse = executeCliTool(name, selected.arguments.queries);
          const cliResults = cliResponse.results;
          const mcpResults = selected.response.structuredContent.results;
          const mcpBase = selected.response.structuredContent.root;
          const cliBase = cliResponse?.root;
          const differences = differingFields(mcpResults, cliResults);
          // Provider cache state depends on which cross-process arm reached the
          // provider first. For these provider tools it is receipt metadata,
          // not evidence or tool data; retain the raw pair and compare all other
          // fields explicitly. No local/AST/LSP result receives this exception.
          const cacheOnly =
            cacheVolatileTools.has(name) &&
            differences.every(field => field === '/0/cache');
          const mcpComparable = cacheOnly
            ? mcpResults.map(({ cache, ...row }) => row)
            : mcpResults;
          const cliComparable = cacheOnly
            ? cliResults.map(({ cache, ...row }) => row)
            : cliResults;
          const contractDifferences = differingFields(
            mcpComparable,
            cliComparable
          ).filter(field => !field.endsWith('/cursor'));
          const opaqueCursorDifferences = differences.filter(field =>
            field.endsWith('/cursor')
          );
          const baseEqual = mcpBase === cliBase;
          parity.push({
            name,
            arguments: selected.arguments,
            mcpResults,
            cliResults,
            mcpBase,
            cliBase,
            baseEqual,
            exact: differences.length === 0,
            rawDifferingFields: differences,
            comparison:
              cacheOnly || opaqueCursorDifferences.length
                ? 'evidence-data-and-executable-query'
                : 'exact-structured-results',
            cacheExclusionJustification: cacheOnly
              ? 'provider cache state is cross-process timing metadata; all evidence and data fields remain exact'
              : undefined,
            opaqueCursorExclusionJustification: opaqueCursorDifferences.length
              ? 'opaque cursors may differ across independent processes; the continuation tool and executable query remain exact'
              : undefined,
            differingFields: contractDifferences,
            passesContract: contractDifferences.length === 0 && baseEqual,
          });
        }
        receipt.cliMcpParitySummary = {
          compared: parity.filter(row => row.passesContract !== undefined)
            .length,
          notApplicable: parity
            .filter(row => row.status === 'not-applicable')
            .map(row => row.name),
          notRun: parity
            .filter(row => row.status === 'not-run')
            .map(row => row.name),
        };
        const requiredLocal = [
          'localSearch',
          'structureSearch',
          'astSearch',
          'localFetch',
          'lspSearch',
        ];
        assert.deepEqual(
          state.expectedTools
            .filter(name => requiredLocal.includes(name))
            .sort(),
          [...requiredLocal].sort(),
          'hermetic acceptance must expose every local/AST/LSP tool'
        );
        assert.deepEqual(
          parity
            .filter(
              row => requiredLocal.includes(row.name) && row.passesContract
            )
            .map(row => row.name)
            .sort(),
          [...requiredLocal].sort(),
          'every available local/AST/LSP tool must complete real CLI-MCP parity'
        );
        const failures = parity.filter(
          row =>
            !['not-run', 'not-applicable'].includes(row.status) &&
            !row.passesContract
        );
        assert.deepEqual(
          failures.map(row => ({
            name: row.name,
            baseEqual: row.baseEqual,
            differingFields: row.differingFields,
          })),
          []
        );
      }
    );
  }
  await check('stdio contains no parser or protocol errors', () =>
    assert.deepEqual(receipt.transportErrors, [])
  );
} finally {
  const start = Date.now();
  try {
    await client.close();
  } catch (error) {
    receipt.closeError = error.message;
  }
  receipt.shutdownMs = Date.now() - start;
  const summarizeLatencies = calls =>
    Object.fromEntries(
      [...new Set(calls.map(call => call.name))].sort().map(name => {
        const durations = calls
          .filter(call => call.name === name)
          .map(call => call.durationMs)
          .sort((left, right) => left - right);
        const percentile = fraction =>
          durations[
            Math.min(
              durations.length - 1,
              Math.ceil(durations.length * fraction) - 1
            )
          ];
        return [
          name,
          {
            count: durations.length,
            p50Ms: percentile(0.5),
            p95Ms: percentile(0.95),
            maxMs: durations.at(-1),
            totalMs: Number(
              durations.reduce((sum, duration) => sum + duration, 0).toFixed(2)
            ),
          },
        ];
      })
    );
  receipt.latencyByTool = summarizeLatencies(receipt.calls);
  receipt.cliLatencyByTool = summarizeLatencies(cliCalls);
  receipt.responseBytesByTool = Object.fromEntries(
    [...new Set(receipt.calls.map(call => call.name))].sort().map(name => {
      const sizes = receipt.calls
        .filter(call => call.name === name)
        .map(call => call.responseBytes)
        .sort((left, right) => left - right);
      const percentile = fraction =>
        sizes[
          Math.min(sizes.length - 1, Math.ceil(sizes.length * fraction) - 1)
        ];
      return [
        name,
        {
          count: sizes.length,
          p50Bytes: percentile(0.5),
          p95Bytes: percentile(0.95),
          maxBytes: sizes.at(-1),
        },
      ];
    })
  );
  receipt.totalMs = Number((performance.now() - suiteStartedAt).toFixed(2));
  await check('child shuts down and releases its PID', () => {
    assert.ok(pid);
    assert.throws(() => process.kill(pid, 0));
    assert.ok(receipt.shutdownMs < 7_000);
  });
  await mkdir(path.dirname(path.resolve(values.receipt)), { recursive: true });
  await writeFile(values.receipt, JSON.stringify(receipt, null, 2));
  if (!values.fixture) {
    await rm(fixture, { recursive: true, force: true });
  }
}
const failures = receipt.checks.filter(check => check.status === 'failed');
console.log(
  JSON.stringify({
    passed: receipt.checks.length - failures.length,
    failures,
    calledTools: [...new Set(receipt.calls.map(call => call.name))],
    latencyByTool: receipt.latencyByTool,
    cliLatencyByTool: receipt.cliLatencyByTool,
    responseBytesByTool: receipt.responseBytesByTool,
    guidance: receipt.guidance,
    totalMs: receipt.totalMs,
    receipt: values.receipt,
    shutdownMs: receipt.shutdownMs,
  })
);
if (failures.length) process.exitCode = 1;
