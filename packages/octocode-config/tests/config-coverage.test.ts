// The declaration is the one place Octocode configuration is known: what exists,
// its type, its default, whether it is sensitive, and whether a workspace may set it.
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { contractFromDeclaration } from '../scripts/config-contract/declaration/index.ts';
import { flattenFields, orderedEnvironment } from '../scripts/config-contract/model.ts';

const ROOT = fileURLToPath(new URL('../../..', import.meta.url));
const contract = contractFromDeclaration();
const fields = flattenFields(contract);
const declared = new Set([
  ...Object.keys(contract.environment),
  ...fields.flatMap(field => (field.env ? [field.env.name] : [])),
]);

// Names read outside the contract on purpose: each is a development or test seam,
// never something a user configures.
const NOT_CONFIGURATION = new Map([
  ['OCTOCODE_NATIVE_BIN', 'dev-only launcher override, gated by devOverridesAllowed'],
  ['OCTOCODE_LEAD_PROBE', 'tool-health harness probe'],
  ['OCTOCODE_NET_GUARD_TEST_LOOPBACK', 'lets skill tests reach a loopback server'],
  ['OCTOCODE_CLI', 'skill scripts: which local CLI to run'],
  ['SERVICE_API_KEY', 'the example in the is_secret_key doc comment'],
]);

function walk(directory: string, keep: (path: string) => boolean): string[] {
  return readdirSync(`${ROOT}/${directory}`, { recursive: true, encoding: 'utf8' })
    .map(path => `${directory}/${path}`)
    .filter(keep);
}

function sources(): string[] {
  const isTest = /tests?[./]|\/tests\/|\.test\.|config-env\.mjs$|node_modules|process_fixture\.rs$/;
  return [
    ...walk('packages/octocode-native/crates', path => /\/src\/.*\.rs$/.test(path)),
    ...walk('skills', path => /\/scripts\/(lib\/)?[^/]+\.mjs$/.test(path)),
  ].filter(path => !isTest.test(path));
}

describe('the configuration contract is complete', () => {
  it('declares every Octocode variable, API key and API endpoint the code reads', () => {
    const pattern = /\b(OCTOCODE_[A-Z][A-Z0-9_]*|[A-Z][A-Z0-9_]*_API_KEY|[A-Z][A-Z0-9_]*_BASE_URL)\b/g;
    const found = new Map<string, string>();
    for (const path of sources()) {
      for (const [name] of readFileSync(`${ROOT}/${path}`, 'utf8').matchAll(pattern)) {
        if (!found.has(name)) found.set(name, path);
      }
    }
    const missing = [...found].filter(([name]) => !declared.has(name) && !NOT_CONFIGURATION.has(name));
    expect(missing).toEqual([]);
    const stale = [...NOT_CONFIGURATION.keys()].filter(name => !found.has(name));
    expect(stale).toEqual([]);
  });

  it('never lets a workspace choose where a secret is sent', () => {
    const { homeTrusted } = orderedEnvironment(fields, contract);
    const endpoints = [...declared].filter(name => /_(BASE_URL|API_URL|API_HOST)$/.test(name));
    expect(endpoints.length).toBeGreaterThan(0);
    expect(endpoints.filter(name => !homeTrusted.includes(name))).toEqual([]);
  });

  it('marks every secret as sensitive, GitHub tokens included', () => {
    const { credentialNames } = orderedEnvironment(fields, contract);
    const secretNamed = [...declared].filter(name => /(_API_KEY|_TOKEN|SCRAPING_ANT|_API)$/.test(name));
    expect(secretNamed.length).toBeGreaterThan(0);
    expect(secretNamed.filter(name => !credentialNames.includes(name))).toEqual([]);
  });

  it('treats proxies and certificate stores as process-only, since they can intercept every call', () => {
    for (const name of ['HTTPS_PROXY', 'HTTP_PROXY', 'ALL_PROXY', 'NO_PROXY', 'SSL_CERT_FILE', 'SSL_CERT_DIR', 'GIT_SSL_CAINFO', 'GIT_SSL_CAPATH', 'CURL_CA_BUNDLE']) {
      expect(contract.environment[name]?.dotenv, name).toBe('never');
    }
  });

  it('states a type and a default for every entry a reader can look up', () => {
    // Protected or passed through to other programs, not configured.
    const system = new Set(['PATH', 'HOME', 'SHELL', 'USER', 'LOGNAME', 'PWD', 'TMPDIR', 'NODE_OPTIONS', 'PYTHON', 'GIT_SSL_CAINFO', 'GIT_SSL_CAPATH', 'CURL_CA_BUNDLE', 'OCTOCODE_CARGO']);
    const entries = Object.entries(contract.environment).filter(([name]) => !system.has(name));
    expect(entries.filter(([, entry]) => !entry.description).map(([name]) => name)).toEqual([]);
    expect(entries.filter(([, entry]) => !entry.type).map(([name]) => name)).toEqual([]);
    expect(entries.filter(([, entry]) => !entry.displayDefault).map(([name]) => name)).toEqual([]);
  });

  it('groups every provider key and links where to get it when a site issues one', () => {
    const keys = Object.entries(contract.environment).filter(([, entry]) => entry.credential);
    const provider = keys.filter(([name]) => /_API_KEY$|^SCRAPING_ANT$/.test(name));
    expect(provider.length).toBeGreaterThan(0);
    expect(provider.filter(([, entry]) => !entry.group).map(([name]) => name)).toEqual([]);
    expect(provider.filter(([, entry]) => !entry.docsUrl).map(([name]) => name)).toEqual([]);
  });
});

describe('what derives from the contract', () => {
  it('search.mjs defaults match the endpoints the contract documents', () => {
    const script = readFileSync(`${ROOT}/skills/octocode-brainstorming/scripts/search.mjs`, 'utf8');
    for (const name of ['TAVILY_BASE_URL', 'EXA_BASE_URL', 'SERPER_BASE_URL']) {
      expect(script).toContain(`process.env.${name} || '${contract.environment[name]?.displayDefault}'`);
    }
  });

  it('the settings page keeps no list of keys or providers of its own', () => {
    const assets = `${ROOT}/packages/octocode-native/crates/cli/src/cli/config_view/assets`;
    const page = readFileSync(`${assets}/app.js`, 'utf8') + readFileSync(`${assets}/core.js`, 'utf8');
    expect(page).not.toMatch(/TAVILY_API_KEY|EXA_API_KEY|SERPER_API_KEY|GH_TOKEN|https:\/\/[a-z.]+\.(ai|dev|com)/);
  });
});
