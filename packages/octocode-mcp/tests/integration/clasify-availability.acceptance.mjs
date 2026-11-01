/** CLI inventory is unconditional; MCP gates are configuration-only, never provider health. */
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import http from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
import { createAjv } from '../../../octocode-benchmark/local/e2e/harness/query-defaults.mjs';
import {
  cases,
  mcpTools,
  selectionTools,
  selection,
  queryFor,
  checkAdmission,
} from './clasify-availability.fixtures.mjs';

const ajv = createAjv();
const root = process.cwd();
const base = path.join(root, '.octocode/octocode-dev/clasify-availability');
const cliPath = path.join(root, 'packages/octocode/out/octocode.js');
const mcpPath = path.join(root, 'packages/octocode-mcp/dist/index.js');
const contract = JSON.parse(
  await fs.readFile(
    path.join(root, 'packages/octocode-config/contract/tool-contract.json'),
    'utf8'
  )
);
const allTools = contract.tools.map(tool => tool.name).toSorted();
assert.equal(allTools.length, 16, 'current full tool inventory');
const outputValidators = new Map(
  contract.tools.map(tool => [tool.name, ajv.compile(tool.outputSchema)])
);
await fs.mkdir(base, { recursive: true });
const fixture = await fs.mkdtemp(path.join(base, 'fixture-'));
const exec = promisify(execFile);
const timeoutMs = 15000;
async function bounded(promise, label) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`${label}: timed out after ${timeoutMs}ms`)),
          timeoutMs
        );
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
async function cli(project, env, args) {
  try {
    return {
      ...(await exec(process.execPath, [cliPath, ...args], {
        env,
        cwd: project,
        timeout: timeoutMs,
        maxBuffer: 8 << 20,
      })),
      status: 0,
    };
  } catch (error) {
    return {
      stdout: error.stdout ?? '',
      stderr: error.stderr ?? '',
      status: error.code,
    };
  }
}
async function schema(project, env, args) {
  const result = await cli(project, env, ['schema', ...args]);
  assert.equal(result.status, 0, `${args.join(' ')}: ${result.stderr}`);
  return JSON.parse(result.stdout);
}
function output(tool, value, label) {
  const validate = outputValidators.get(tool);
  assert.equal(
    validate(value),
    true,
    `${label}: output schema ${JSON.stringify(validate.errors)}`
  );
  return value;
}
function mcpOutput(tool, response, label) {
  assert.ok(response.structuredContent, `${label}: MCP structured output`);
  return output(tool, response.structuredContent, label);
}
function checkInvocation(value, failed, row, label) {
  const equal = (actual, expected, why) =>
    assert.deepEqual(actual, expected, `${label}: ${why}`);
  const error = value.results?.[0]?.data ?? value;
  const expectedFailure = !row.key || !!(row.providerStatus || row.disconnect);
  equal(failed, expectedFailure, 'invocation outcome');
  if (!row.key) {
    equal(error.errorCode, 'missingConfiguration', 'missing key');
    assert.match(error.error, /classification|OCTOCODE_CLASSIFICATION_API/i);
  } else if (expectedFailure) {
    const code =
      row.providerStatus === 401
        ? 'classificationProviderError'
        : 'classificationProviderUnavailable';
    equal(error.errorCode, code, 'provider failure code');
    const reason = row.providerStatus
      ? new RegExp(String(row.providerStatus))
      : /provider|connect|network|transport/i;
    assert.match(error.error, reason, label);
    assert.ok(error.hints?.length, `${label}: actionable provider recovery`);
  } else equal(Array.isArray(value.results?.[0]?.data?.models), true, 'models');
}
let currentCase;
let providerRequests = 0;
let providerConnections = 0;
const providerErrors = [];
const provider = http.createServer((req, res) => {
  providerRequests++;
  if (
    req.method !== 'GET' ||
    req.url !== '/v1/models' ||
    req.headers.authorization !== 'Bearer fixture-key'
  ) {
    providerErrors.push({
      method: req.method,
      url: req.url,
      authorizationMatches: req.headers.authorization === 'Bearer fixture-key',
    });
    res.writeHead(400);
    res.end();
    return;
  }
  res.writeHead(currentCase.providerStatus ?? 200, {
    'content-type': 'application/json',
  });
  res.end(JSON.stringify({ models: [] }));
});
// A closed connection simulates transport unavailability without a fixed port,
// DNS dependence or unobservable requests to another host.
provider.on('connection', socket => {
  providerConnections++;
  if (currentCase.disconnect) socket.destroy();
});
await new Promise(resolve => provider.listen(0, '127.0.0.1', resolve));
const providerUrl = `http://127.0.0.1:${provider.address().port}`;
const results = [];
try {
  for (const row of cases) {
    currentCase = row;
    const before = [providerRequests, providerConnections];
    const equal = (actual, expected, why) =>
      assert.deepEqual(actual, expected, `${row.name}: ${why}`);
    const noProviderWork = label =>
      equal(
        [providerRequests, providerConnections],
        before,
        `${label}: zero provider requests/connections`
      );
    const home = path.join(fixture, row.name, 'home');
    const project = path.join(fixture, row.name, 'project');
    await fs.mkdir(home, { recursive: true });
    await fs.mkdir(path.join(project, '.octocode'), { recursive: true });
    await fs.writeFile(
      path.join(project, 'evidence.txt'),
      'SELECTED evidence\n'
    );
    if (row.homeEnv) await fs.writeFile(path.join(home, '.env'), row.homeEnv);
    if (row.projectEnv)
      await fs.writeFile(path.join(project, '.octocode/.env'), row.projectEnv);
    if (row.config)
      await fs.writeFile(
        path.join(home, '.octocoderc'),
        JSON.stringify(row.config)
      );
    const env = {
      PATH: process.env.PATH ?? '',
      HOME: path.join(fixture, row.name),
      OCTOCODE_HOME: home,
      WORKSPACE_ROOT: project,
      ALLOWED_PATHS: project,
      OCTOCODE_STORAGE_MODE: 'memory',
      OCTOCODE_CLASSIFICATION_API_HOST: providerUrl,
      DISABLE_TOOLS: '',
      TOOLS_TO_RUN: '',
      ...(process.env.OCTOCODE_NATIVE_BIN && {
        NODE_ENV: 'test',
        OCTOCODE_NATIVE_BIN: process.env.OCTOCODE_NATIVE_BIN,
      }),
      ...row.env,
    };
    const client = new Client({ name: 'clasify-availability', version: '2' });
    let stderr = '';
    const transport = new StdioClientTransport({
      command: process.execPath,
      args: [mcpPath],
      cwd: project,
      env,
      stderr: 'pipe',
    });
    transport.stderr?.on('data', chunk => {
      stderr += chunk;
    });
    try {
      const expectedNames = mcpTools(row, allTools);
      const connecting = bounded(client.connect(transport), 'MCP initialize');
      if (expectedNames.length) await connecting;
      else
        await assert.rejects(connecting, {
          code: -32603,
          message:
            'Server initialization failed: No native tools are available',
        });
      noProviderWork('MCP initialize');
      const catalog = expectedNames.length
        ? await bounded(client.listTools(), 'MCP tools/list')
        : { tools: [] };
      noProviderWork('MCP tools/list or rejected initialization');
      const actualNames = catalog.tools.map(tool => tool.name).toSorted();
      equal(actualNames, expectedNames, 'complete MCP visibility matrix');
      const clone = catalog.tools.find(tool => tool.name === 'ghCloneRepo');
      if (clone) {
        equal(
          clone.annotations?.readOnlyHint,
          false,
          'clone writes local cache'
        );
        equal(
          clone.annotations?.openWorldHint,
          true,
          'clone uses remote source'
        );
      }
      const cliCatalog = await schema(project, env, ['--view', 'full']);
      equal(
        cliCatalog.tools.map(tool => tool.name).toSorted(),
        allTools,
        'CLI full inventory'
      );
      noProviderWork('CLI full schema');
      for (const args of [['--help'], ['clasify', '--help']]) {
        const help = await cli(project, env, args);
        equal(help.status, 0, `help ${help.stderr}`);
        assert.match(
          help.stdout,
          /clasify/i,
          `${row.name}: help exposes clasify`
        );
        noProviderWork(`CLI ${args.join(' ')}`);
      }
      const standalone = await schema(project, env, ['clasify']);
      equal(
        ajv.compile(standalone.querySchema)({ operation: 'models' }),
        true,
        'CLI clasify schema'
      );
      noProviderWork('CLI clasify schema');
      for (const name of selectionTools) {
        const mcpTool = catalog.tools.find(tool => tool.name === name);
        if (mcpTools(row, allTools).includes(name)) {
          assert.ok(mcpTool, `${row.name}: MCP ${name}`);
          checkAdmission(ajv, mcpTool.inputSchema, queryFor(name), {
            enabled: row.key,
            envelope: true,
            label: `${row.name}: MCP ${name}`,
          });
        } else equal(mcpTool, undefined, `MCP ${name} filtered`);
        const querySchema = (await schema(project, env, [name])).querySchema;
        checkAdmission(ajv, querySchema, queryFor(name), {
          enabled: row.key,
          envelope: false,
          label: `${row.name}: CLI ${name}`,
        });
        noProviderWork(`CLI/MCP ${name} schema`);
      }
      // Missing inline credentials fail before acquisition on both surfaces.
      if (!row.key) {
        const input = {
          queries: [{ path: 'evidence.txt', classification: selection }],
        };
        const rejected = await cli(project, env, [
          'localFetch',
          JSON.stringify(input),
          '--json',
        ]);
        equal(rejected.status === 0, false, 'CLI rejects unavailable context');
        assert.match(
          rejected.stdout + rejected.stderr,
          /classification|invalidInput/i
        );
        output(
          'localFetch',
          JSON.parse(rejected.stdout),
          `${row.name}: CLI inline rejection`
        );
        if (actualNames.includes('localFetch')) {
          const rejectedMcp = await bounded(
            client.callTool({ name: 'localFetch', arguments: input }),
            'inline rejection'
          );
          equal(rejectedMcp.isError, true, 'MCP rejects unavailable context');
          mcpOutput('localFetch', rejectedMcp, `${row.name}: inline rejection`);
        }
        noProviderWork('unavailable inline classification');
      }
      const input = { queries: [{ operation: 'models' }] };
      const called = await cli(project, env, [
        'clasify',
        JSON.stringify(input),
        '--json',
      ]);
      const cliValue = output(
        'clasify',
        JSON.parse(called.stdout),
        `${row.name}: CLI invocation`
      );
      checkInvocation(cliValue, called.status !== 0, row, `${row.name}: CLI`);
      if (expectedNames.length) {
        const beforeMcp = [providerRequests, providerConnections];
        const invoked = bounded(
          client.callTool({ name: 'clasify', arguments: input }),
          'clasify invocation'
        );
        if (actualNames.includes('clasify')) {
          const response = await invoked;
          const value = mcpOutput(
            'clasify',
            response,
            `${row.name}: MCP invocation`
          );
          checkInvocation(
            value,
            response.isError === true,
            row,
            `${row.name}: MCP`
          );
        } else {
          await assert.rejects(invoked, {
            code: -32602,
            message: 'Tool clasify not found',
          });
          equal(
            [providerRequests, providerConnections],
            beforeMcp,
            'hidden MCP call: zero provider work'
          );
        }
      }
      if (row.key)
        equal(
          providerConnections > before[1],
          true,
          'invocation reaches provider'
        );
      else noProviderWork('missing-key standalone invocation');
      if (row.key && !row.disconnect && row.providerStatus !== 503) {
        const expected = actualNames.includes('clasify') ? 2 : 1;
        equal(
          providerRequests - before[0],
          expected,
          'one request per invocation, no startup ping'
        );
      }
      equal(providerErrors, [], 'provider request contract');
      results.push({
        name: row.name,
        key: row.key,
        mcpTools: actualNames,
        mcpInitialized: expectedNames.length > 0,
        cliTools: allTools.length,
        discoveryProviderRequests: 0,
        discoveryProviderConnections: 0,
        invocationProviderRequests: providerRequests - before[0],
        invocationProviderConnections: providerConnections - before[1],
        passed: true,
      });
    } catch (error) {
      results.push({ name: row.name, passed: false, error: error.message });
      throw new Error(`${row.name}: ${error.message}\n${stderr}`, {
        cause: error,
      });
    } finally {
      await bounded(client.close(), 'MCP close').catch(() => transport.close());
    }
  }
} finally {
  provider.closeAllConnections();
  await new Promise(resolve => provider.close(resolve));
  await fs.writeFile(
    path.join(base, 'results.json'),
    `${JSON.stringify(results, null, 2)}\n`
  );
  await fs.rm(fixture, { recursive: true, force: true });
}
console.log(
  `PASS ${results.length} configuration-only MCP / unconditional CLI scenarios`
);
