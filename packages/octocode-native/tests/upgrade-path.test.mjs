import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import {
  binPath, fallbackVerdict, findTarballs, graphResultIds, graphVerdict, isolatedEnv, parseArgs, parseDaemons, parseFrom,
  quoteWin,
} from '../scripts/upgrade-path-lib.mjs';

const answer = ids => JSON.stringify({ results: [{ index: 0, data: { results: ids.map(id => ({ id })) } }] });

const NAMES = [
  'octocode-20.2.0.tgz',
  'octocode-mcp-20.2.0.tgz',
  'octocodeai-octocode-native-darwin-arm64-20.2.0.tgz',
  'octocodeai-octocode-native-linux-x64-gnu-20.2.0.tgz',
  'notes.txt',
];

test('parseArgs keeps the CI interface and rejects unknown flags', () => {
  const options = parseArgs(['--packages', 'pkgs', '--from', 'octocode@20.1.0,octocode-mcp@19.1.0', '--json', 'out.json']);
  assert.deepEqual(options, { packages: 'pkgs', from: { octocode: '20.1.0', 'octocode-mcp': '19.1.0' }, json: 'out.json', keep: false, help: false });
  assert.deepEqual(parseArgs(['--packages', 'p', '--json', 'o']).from, {});
  assert.throws(() => parseArgs(['--packages', 'p']), /--json are required/);
  assert.throws(() => parseArgs(['--packages', 'p', '--json', 'o', '--publish']), /unknown argument '--publish'/);
  assert.throws(() => parseArgs(['--packages', '--json', 'o']), /--packages needs a value/);
});

test('parseFrom accepts either launcher in any order and rejects other names', () => {
  assert.deepEqual(parseFrom('octocode-mcp@19.1.0'), { 'octocode-mcp': '19.1.0' });
  assert.deepEqual(parseFrom(' octocode-mcp@1.0.0-rc.1 , octocode@2.0.0 '), { 'octocode-mcp': '1.0.0-rc.1', octocode: '2.0.0' });
  assert.throws(() => parseFrom('octocode'), /must be/);
  assert.throws(() => parseFrom('left-pad@1.0.0'), /must be/);
});

test('findTarballs picks both launchers and the host platform at one version', () => {
  assert.deepEqual(findTarballs(NAMES, 'darwin-arm64'), {
    version: '20.2.0',
    cli: 'octocode-20.2.0.tgz',
    mcp: 'octocode-mcp-20.2.0.tgz',
    platform: 'octocodeai-octocode-native-darwin-arm64-20.2.0.tgz',
  });
  assert.equal(findTarballs([...NAMES, 'octocodeai-octocode-native-linux-arm64-gnu-20.2.0.tgz'], 'linux-x64-gnu').platform,
    'octocodeai-octocode-native-linux-x64-gnu-20.2.0.tgz');
  assert.throws(() => findTarballs(NAMES, 'win32-x64-msvc'), /win32-x64-msvc platform tarball, found none/);
  assert.throws(() => findTarballs(['octocode-20.2.0.tgz', 'octocode-mcp-20.2.1.tgz', NAMES[2]], 'darwin-arm64'), /versions differ/);
  assert.throws(() => findTarballs([...NAMES, 'octocode-20.1.0.tgz'], 'darwin-arm64'), /one octocode tarball, found octocode-20.2.0.tgz, octocode-20.1.0.tgz/);
});

test('isolatedEnv replaces homes, caches, prefix and temp, and drops tokens and Octocode settings', () => {
  const dirs = { home: '/r/home', tmp: '/r/t', npmCache: '/r/cache', globalPrefix: '/r/global' };
  const env = isolatedEnv({
    PATH: '/bin', HOME: '/Users/me', GITHUB_TOKEN: 'real', gh_token: 'real', OCTOCODE_HOME: '/Users/me/.octocode',
    OCTOCODE_BETA: '1', npm_config_prefix: '/usr/local', NODE_OPTIONS: '--inspect', TOOLS_TO_RUN: 'x',
  }, dirs, 'linux');
  assert.equal(env.PATH, '/bin');
  assert.equal(env.HOME, '/r/home');
  assert.equal(env.USERPROFILE, '/r/home');
  assert.equal(env.OCTOCODE_HOME, join('/r/home', '.octocode'));
  assert.equal(env.TMPDIR, '/r/t');
  assert.equal(env.npm_config_prefix, '/r/global');
  assert.equal(env.npm_config_cache, '/r/cache');
  for (const name of ['GITHUB_TOKEN', 'gh_token', 'OCTOCODE_BETA', 'NODE_OPTIONS', 'TOOLS_TO_RUN']) assert.equal(env[name], undefined, name);
  assert.equal(env.APPDATA, undefined);
  assert.equal(isolatedEnv({}, dirs, 'win32').LOCALAPPDATA, join('/r/home', 'AppData', 'Local'));
});

test('quoteWin leaves plain words alone and quotes cmd.exe metacharacters', () => {
  assert.equal(quoteWin('--version'), '--version');
  assert.equal(quoteWin('C:\\a b\\x.tgz'), '"C:\\a b\\x.tgz"');
  assert.equal(quoteWin('a&b'), '"a&b"');
  assert.equal(quoteWin('say "hi"'), '"say \\"hi\\""');
  assert.equal(quoteWin('C:\\dir with space\\'), '"C:\\dir with space\\\\"');
  assert.equal(quoteWin(''), '""');
});

test('binPath follows npm project and global layouts per platform', () => {
  assert.equal(binPath('/p', 'octocode', 'project', 'linux'), join('/p', 'node_modules', '.bin', 'octocode'));
  assert.equal(binPath('/p', 'octocode', 'project', 'win32'), join('/p', 'node_modules', '.bin', 'octocode.cmd'));
  assert.equal(binPath('/g', 'octocode', 'global', 'darwin'), join('/g', 'bin', 'octocode'));
  assert.equal(binPath('/g', 'octocode', 'global', 'win32'), join('/g', 'octocode.cmd'));
});

test('parseDaemons keeps only serve processes whose socket is under the run root', () => {
  const listing = [
    '  101 /x/octocode serve --socket /run/t/octocode-501/serve-aaaa.sock',
    '  102 /x/octocode serve --socket /real/T/octocode-501/serve-bbbb.sock',
    '  103 /x/octocode localSearch --input q.json',
  ].join('\n');
  assert.deepEqual(parseDaemons(listing, '/run/t'), [
    { pid: 101, command: '/x/octocode serve --socket /run/t/octocode-501/serve-aaaa.sock', socket: '/run/t/octocode-501/serve-aaaa.sock' },
  ]);
});

test('graphVerdict accepts the same answer or a clear rebuild error, never a crash or other data', () => {
  assert.deepEqual(graphResultIds(`noise\n${answer(['b', 'a'])}\n`), ['a', 'b']);
  assert.equal(graphResultIds('not json'), null);
  assert.equal(graphVerdict({ status: 0, stdout: answer(['m']), stderr: '' }, ['m']).kind, 'same-data');
  assert.equal(graphVerdict({ status: 0, stdout: answer(['x']), stderr: '' }, ['m']).kind, 'wrong-data');
  assert.equal(graphVerdict({ status: 5, stdout: '', stderr: 'graph format v16 is not supported; rebuild it with graphBuild' }, ['m']).kind, 'clear-error');
  assert.equal(graphVerdict({ status: 5, stdout: '', stderr: 'boom' }, ['m']).kind, 'unclear-error');
  assert.equal(graphVerdict({ status: null, signal: 'SIGSEGV', stdout: '', stderr: '' }, ['m']).kind, 'crash');
  assert.equal(graphVerdict({ status: 139, stdout: '', stderr: 'rebuild' }, ['m']).kind, 'crash');
  assert.equal(graphVerdict({ timedOut: true, status: null }, ['m']).kind, 'timeout');
});

test('fallbackVerdict accepts a fetched binary or a fast error naming the package', () => {
  const name = '@octocodeai/octocode-native-linux-x64-gnu';
  const base = { stdout: '', stderr: '', ms: 5, signal: null };
  assert.equal(fallbackVerdict({ ...base, status: 0, stdout: 'octocode 1.2.3\n' }, '1.2.3', name).kind, 'fetched');
  assert.equal(fallbackVerdict({ ...base, status: 5, stderr: `platform package '${name}' is not installed. HTTP 404` }, '1.2.3', name).kind, 'clear-error');
  assert.equal(fallbackVerdict({ ...base, status: 5, stderr: 'failed' }, '1.2.3', name).kind, 'unclear');
  assert.equal(fallbackVerdict({ ...base, status: 0, stdout: 'octocode 1.0.0' }, '1.2.3', name).ok, false);
  assert.equal(fallbackVerdict({ ...base, timedOut: true, status: null }, '1.2.3', name).kind, 'hang');
});
