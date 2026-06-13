#!/usr/bin/env node
import { performance } from 'perf_hooks';
import { readFile } from 'fs/promises';
import path from 'path';
import { fileURLToPath } from 'url';
import {
  LSPClient,
  getLanguageServerForFile,
  isLanguageServerAvailable,
  releaseAllPooledClients,
} from '../dist/index.js';

const benchmarkRoot = path.dirname(fileURLToPath(import.meta.url));
const requestedLanguages = new Set(
  process.argv
    .slice(2)
    .filter(arg => !arg.startsWith('--'))
    .map(arg => arg.toLowerCase())
);

const CASES = [
  {
    id: 'typescript',
    title: 'TypeScript',
    files: ['src/interface.ts', 'src/service.ts', 'src/index.ts'],
    entry: 'src/index.ts',
    operations: {
      definition: {
        file: 'src/index.ts',
        needle: 'FriendlyGreeter',
        occurrence: 1,
        expect: { minLocations: 1, fileIncludes: 'src/service.ts' },
      },
      references: {
        file: 'src/service.ts',
        needle: 'welcome',
        expect: { minLocations: 3, fileIncludes: 'src/index.ts' },
      },
      hover: {
        file: 'src/index.ts',
        needle: 'welcome',
        occurrence: 1,
        expect: { textIncludes: 'welcome' },
      },
      documentSymbols: {
        file: 'src/service.ts',
        expect: { names: ['FriendlyGreeter', 'welcome'] },
      },
      typeDefinition: {
        file: 'src/service.ts',
        needle: 'greeter',
        occurrence: 1,
        expect: { minLocations: 1, fileIncludes: 'src/interface.ts' },
      },
      implementation: {
        file: 'src/interface.ts',
        needle: 'greet',
        expect: { minLocations: 1, fileIncludes: 'src/service.ts' },
      },
      callHierarchy: {
        file: 'src/service.ts',
        needle: 'welcome',
        expect: { prepared: 'welcome', incoming: 'main', outgoing: 'greet' },
      },
    },
  },
  {
    id: 'javascript',
    title: 'JavaScript',
    files: ['src/service.js', 'src/index.js'],
    entry: 'src/index.js',
    operations: {
      definition: {
        file: 'src/index.js',
        needle: 'FriendlyGreeter',
        occurrence: 1,
        expect: { minLocations: 1, fileIncludes: 'src/service.js' },
      },
      references: {
        file: 'src/service.js',
        needle: 'welcome',
        expect: { minLocations: 3, fileIncludes: 'src/index.js' },
      },
      hover: {
        file: 'src/index.js',
        needle: 'welcome',
        occurrence: 1,
        expect: { textIncludes: 'welcome' },
      },
      documentSymbols: {
        file: 'src/service.js',
        expect: { names: ['FriendlyGreeter', 'welcome'] },
      },
      callHierarchy: {
        file: 'src/service.js',
        needle: 'welcome',
        expect: { prepared: 'welcome', incoming: 'main' },
      },
    },
  },
  {
    id: 'python',
    title: 'Python',
    files: ['service.py', 'main.py'],
    entry: 'main.py',
    operations: {
      definition: {
        file: 'main.py',
        needle: 'FriendlyGreeter',
        expect: { minLocations: 1, fileIncludes: 'service.py' },
      },
      references: {
        file: 'service.py',
        needle: 'welcome',
        expect: { minLocations: 2, fileIncludes: 'main.py' },
      },
      hover: {
        file: 'main.py',
        needle: 'welcome',
        expect: { textIncludes: 'welcome' },
      },
      documentSymbols: {
        file: 'service.py',
        expect: { names: ['FriendlyGreeter', 'welcome'] },
      },
      callHierarchy: {
        file: 'service.py',
        needle: 'welcome',
        expect: { prepared: 'welcome', incoming: 'main' },
      },
    },
  },
  {
    id: 'go',
    title: 'Go',
    files: ['go.mod', 'service/service.go', 'main.go'],
    entry: 'main.go',
    operations: {
      definition: {
        file: 'main.go',
        needle: 'FriendlyGreeter',
        expect: { minLocations: 1, fileIncludes: 'service/service.go' },
      },
      references: {
        file: 'service/service.go',
        needle: 'Welcome',
        expect: { minLocations: 2, fileIncludes: 'main.go' },
      },
      hover: {
        file: 'main.go',
        needle: 'Welcome',
        expect: { textIncludes: 'Welcome' },
      },
      documentSymbols: {
        file: 'service/service.go',
        expect: { names: ['FriendlyGreeter', 'Welcome'] },
      },
      typeDefinition: {
        file: 'service/service.go',
        needle: 'greeter',
        expect: { minLocations: 1, fileIncludes: 'service/service.go' },
      },
      implementation: {
        file: 'service/service.go',
        needle: 'Greet',
        expect: { minLocations: 1, fileIncludes: 'service/service.go' },
      },
      callHierarchy: {
        file: 'service/service.go',
        needle: 'Welcome',
        expect: { prepared: 'Welcome', incoming: 'main', outgoing: 'Greet' },
      },
    },
  },
  {
    id: 'rust',
    title: 'Rust',
    files: ['Cargo.toml', 'src/lib.rs', 'src/main.rs'],
    entry: 'src/main.rs',
    operations: {
      definition: {
        file: 'src/main.rs',
        needle: 'FriendlyGreeter',
        expect: { minLocations: 1, fileIncludes: 'src/lib.rs' },
      },
      references: {
        file: 'src/lib.rs',
        needle: 'welcome',
        expect: { minLocations: 2, fileIncludes: 'src/main.rs' },
      },
      hover: {
        file: 'src/main.rs',
        needle: 'welcome',
        expect: { textIncludes: 'welcome' },
      },
      documentSymbols: {
        file: 'src/lib.rs',
        expect: { names: ['FriendlyGreeter', 'welcome'] },
      },
      typeDefinition: {
        file: 'src/lib.rs',
        needle: 'greeter',
        expect: { minLocations: 1, fileIncludes: 'src/lib.rs' },
      },
      implementation: {
        file: 'src/lib.rs',
        needle: 'greet',
        expect: { minLocations: 1, fileIncludes: 'src/lib.rs' },
      },
      callHierarchy: {
        file: 'src/lib.rs',
        needle: 'welcome',
        expect: { prepared: 'welcome', incoming: 'main', outgoing: 'greet' },
      },
    },
  },
  {
    id: 'cpp',
    title: 'C++',
    files: [
      'compile_flags.txt',
      'include/greeter.hpp',
      'src/greeter.cpp',
      'src/main.cpp',
    ],
    entry: 'src/main.cpp',
    operations: {
      definition: {
        file: 'src/main.cpp',
        needle: 'welcome',
        expect: { minLocations: 1, fileIncludes: 'greeter' },
      },
      references: {
        file: 'include/greeter.hpp',
        needle: 'welcome',
        expect: { minLocations: 2, fileIncludes: 'src/main.cpp' },
      },
      hover: {
        file: 'src/main.cpp',
        needle: 'welcome',
        expect: { textIncludes: 'welcome' },
      },
      documentSymbols: {
        file: 'include/greeter.hpp',
        expect: { names: ['Greeter', 'FriendlyGreeter', 'welcome'] },
      },
      typeDefinition: {
        file: 'src/greeter.cpp',
        needle: 'greeter',
        expect: { minLocations: 1, fileIncludes: 'include/greeter.hpp' },
      },
      implementation: {
        file: 'include/greeter.hpp',
        needle: 'greet',
        expect: { minLocations: 1, fileIncludes: 'src/greeter.cpp' },
      },
      callHierarchy: {
        file: 'include/greeter.hpp',
        needle: 'welcome',
        expect: { prepared: 'welcome', incoming: 'main', outgoing: 'greet' },
      },
    },
  },
];

const CAPABILITIES = {
  definition: 'definitionProvider',
  references: 'referencesProvider',
  hover: 'hoverProvider',
  documentSymbols: 'documentSymbolProvider',
  typeDefinition: 'typeDefinitionProvider',
  implementation: 'implementationProvider',
  callHierarchy: 'callHierarchyProvider',
};

function positionFor(content, needle, occurrence = 0) {
  let index = -1;
  let start = 0;
  for (let i = 0; i <= occurrence; i++) {
    index = content.indexOf(needle, start);
    start = index + needle.length;
  }
  if (index < 0) {
    throw new Error(`Needle not found: ${needle}`);
  }
  index += Math.floor(needle.length / 2);
  const before = content.slice(0, index);
  const lines = before.split(/\r?\n/);
  return { line: lines.length - 1, character: lines.at(-1).length };
}

function relativeFile(caseRoot, filePath) {
  return path.relative(caseRoot, filePath).split(path.sep).join('/');
}

function hoverText(hover) {
  const contents = hover?.contents;
  if (typeof contents === 'string') return contents;
  if (Array.isArray(contents)) {
    return contents
      .map(item => (typeof item === 'string' ? item : item.value ?? ''))
      .join('\n');
  }
  if (contents && typeof contents === 'object') return contents.value ?? '';
  return '';
}

function symbolNames(symbols) {
  const names = [];
  const visit = symbol => {
    if (symbol?.name) names.push(symbol.name);
    for (const child of symbol?.children ?? []) visit(child);
  };
  for (const symbol of symbols) visit(symbol);
  return names;
}

function locationsMatch(caseRoot, locations, expect) {
  if (locations.length < (expect.minLocations ?? 1)) {
    return `expected at least ${expect.minLocations ?? 1} location(s), got ${locations.length}`;
  }
  if (expect.fileIncludes) {
    const matched = locations.some(location =>
      relativeFile(caseRoot, location.uri).includes(expect.fileIncludes)
    );
    if (!matched) {
      return `expected a location under ${expect.fileIncludes}, got ${locations
        .map(location => relativeFile(caseRoot, location.uri))
        .join(', ')}`;
    }
  }
  return null;
}

function namesMatch(actualNames, expectedNames) {
  const missing = expectedNames.filter(name => !actualNames.includes(name));
  if (missing.length > 0) {
    return `missing symbol(s): ${missing.join(', ')}; saw ${actualNames.join(', ')}`;
  }
  return null;
}

async function runOperation(client, testCase, operationName, operation) {
  const caseRoot = path.join(benchmarkRoot, testCase.id);
  const filePath = path.join(caseRoot, operation.file);
  const startedAt = performance.now();

  if (operationName === 'documentSymbols') {
    const symbols = await client.documentSymbols(filePath);
    const names = symbolNames(symbols);
    const error = namesMatch(names, operation.expect.names);
    return result(operationName, startedAt, !error, error, {
      symbols: names.length,
      sample: names.slice(0, 8),
    });
  }

  const content = await readFile(filePath, 'utf8');
  const position = positionFor(
    content,
    operation.needle,
    operation.occurrence ?? 0
  );

  if (operationName === 'definition') {
    const locations = await client.gotoDefinition(filePath, position);
    const error = locationsMatch(caseRoot, locations, operation.expect);
    return result(operationName, startedAt, !error, error, {
      locations: locations.map(location => relativeFile(caseRoot, location.uri)),
    });
  }

  if (operationName === 'references') {
    const locations = await client.findReferences(filePath, position, true);
    const error = locationsMatch(caseRoot, locations, operation.expect);
    return result(operationName, startedAt, !error, error, {
      locations: locations.map(location => relativeFile(caseRoot, location.uri)),
    });
  }

  if (operationName === 'hover') {
    const hover = await client.hover(filePath, position);
    const text = hoverText(hover);
    const expected = operation.expect.textIncludes;
    const ok = expected ? text.includes(expected) : text.length > 0;
    return result(
      operationName,
      startedAt,
      ok,
      ok ? null : `expected hover text to include ${expected}`,
      { text: text.replace(/\s+/g, ' ').trim().slice(0, 120) }
    );
  }

  if (operationName === 'typeDefinition') {
    const locations = await client.typeDefinition(filePath, position);
    const error = locationsMatch(caseRoot, locations, operation.expect);
    return result(operationName, startedAt, !error, error, {
      locations: locations.map(location => relativeFile(caseRoot, location.uri)),
    });
  }

  if (operationName === 'implementation') {
    const locations = await client.implementation(filePath, position);
    const error = locationsMatch(caseRoot, locations, operation.expect);
    return result(operationName, startedAt, !error, error, {
      locations: locations.map(location => relativeFile(caseRoot, location.uri)),
    });
  }

  if (operationName === 'callHierarchy') {
    const items = await client.prepareCallHierarchy(filePath, position);
    const root = items[0];
    if (!root) {
      return result(operationName, startedAt, false, 'no call hierarchy root');
    }
    const incoming = await client.getIncomingCalls(root);
    const outgoing = await client.getOutgoingCalls(root);
    const incomingNames = incoming.map(call => call.from.name);
    const outgoingNames = outgoing.map(call => call.to.name);
    const errors = [];
    if (operation.expect.prepared && root.name !== operation.expect.prepared) {
      errors.push(`expected root ${operation.expect.prepared}, got ${root.name}`);
    }
    if (
      operation.expect.incoming &&
      !incomingNames.includes(operation.expect.incoming)
    ) {
      errors.push(
        `expected incoming ${operation.expect.incoming}, got ${incomingNames.join(', ')}`
      );
    }
    if (
      operation.expect.outgoing &&
      !outgoingNames.includes(operation.expect.outgoing)
    ) {
      errors.push(
        `expected outgoing ${operation.expect.outgoing}, got ${outgoingNames.join(', ')}`
      );
    }
    return result(
      operationName,
      startedAt,
      errors.length === 0,
      errors.join('; ') || null,
      { root: root.name, incoming: incomingNames, outgoing: outgoingNames }
    );
  }

  throw new Error(`Unknown operation: ${operationName}`);
}

function result(operation, startedAt, ok, error, details = {}) {
  return {
    operation,
    status: ok ? 'pass' : 'fail',
    durationMs: Math.round(performance.now() - startedAt),
    ...(error ? { error } : {}),
    ...details,
  };
}

async function runCase(testCase) {
  const caseRoot = path.join(benchmarkRoot, testCase.id);
  const entryPath = path.join(caseRoot, testCase.entry);
  const serverConfig = await getLanguageServerForFile(entryPath, caseRoot);
  const serverAvailable = await isLanguageServerAvailable(entryPath, caseRoot);
  const output = {
    id: testCase.id,
    title: testCase.title,
    workspaceRoot: caseRoot,
    server: serverConfig
      ? {
          command:
            serverConfig.command === process.execPath
              ? 'node'
              : serverConfig.command,
          args: serverConfig.args ?? [],
          languageId: serverConfig.languageId,
        }
      : null,
    serverAvailable,
    operations: [],
  };

  if (!serverConfig || !serverAvailable) {
    output.operations.push({
      operation: '*',
      status: 'skip',
      reason: 'language server unavailable',
    });
    return output;
  }

  const client = new LSPClient(serverConfig);
  try {
    try {
      await client.start();
    } catch (error) {
      output.operations.push({
        operation: '*',
        status: 'fail',
        error: `language server failed to initialize: ${
          error instanceof Error ? error.message : String(error)
        }`,
        stderr: client.getRecentStderr().slice(-8),
      });
      return output;
    }

    for (const file of testCase.files) {
      if (path.extname(file)) {
        await client.openDocument(path.join(caseRoot, file));
      }
    }
    await client.waitForReady(30_000);

    for (const [operationName, operation] of Object.entries(
      testCase.operations
    )) {
      const capability = CAPABILITIES[operationName];
      if (capability && !client.hasCapability(capability)) {
        output.operations.push({
          operation: operationName,
          status: 'skip',
          reason: `${capability} unsupported`,
        });
        continue;
      }
      try {
        output.operations.push(
          await runOperation(client, testCase, operationName, operation)
        );
      } catch (error) {
        output.operations.push({
          operation: operationName,
          status: 'fail',
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }
  } finally {
    await client.stop();
  }

  return output;
}

function printReport(results) {
  console.log('Octocode LSP real benchmark');
  console.log(`Root: ${benchmarkRoot}`);
  console.log('');

  for (const item of results) {
    const server = item.server
      ? `${item.server.command} ${(item.server.args ?? []).join(' ')}`
      : 'none';
    console.log(`${item.title} (${item.id})`);
    console.log(`  serverAvailable: ${item.serverAvailable}`);
    console.log(`  server: ${server}`);
    for (const operation of item.operations) {
      const duration =
        operation.durationMs !== undefined ? ` ${operation.durationMs}ms` : '';
      const suffix = operation.error
        ? ` - ${operation.error}`
        : operation.reason
          ? ` - ${operation.reason}`
          : '';
      console.log(
        `  ${operation.status.toUpperCase().padEnd(4)} ${operation.operation}${duration}${suffix}`
      );
    }
    console.log('');
  }

  const flat = results.flatMap(item => item.operations);
  const counts = {
    pass: flat.filter(operation => operation.status === 'pass').length,
    fail: flat.filter(operation => operation.status === 'fail').length,
    skip: flat.filter(operation => operation.status === 'skip').length,
  };
  console.log(
    `Summary: ${counts.pass} passed, ${counts.fail} failed, ${counts.skip} skipped`
  );
  if (counts.fail > 0) process.exitCode = 1;
}

const selectedCases =
  requestedLanguages.size === 0
    ? CASES
    : CASES.filter(testCase => requestedLanguages.has(testCase.id));

if (selectedCases.length === 0) {
  console.error(
    `No benchmark cases matched: ${Array.from(requestedLanguages).join(', ')}`
  );
  process.exit(1);
}

try {
  const results = [];
  for (const testCase of selectedCases) {
    results.push(await runCase(testCase));
  }
  printReport(results);
} finally {
  await releaseAllPooledClients();
}
