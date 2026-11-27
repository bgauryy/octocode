import { test } from 'node:test';
import assert from 'node:assert/strict';
import { normalize, pick, pinFor, resolveRegistry, toRange } from '../lib/registries.mjs';
import { satisfies } from '../lib/semver.mjs';

const fits = (ecosystem, requirement, version) => {
  const { range, exclude } = toRange(ecosystem, requirement);
  return !exclude.includes(normalize(version)) && satisfies(normalize(version), range);
};

test('each ecosystem requirement syntax selects the versions its own tool would', () => {
  const cases = [
    ['crates', '0.7', '0.7.9', true], ['crates', '0.7', '0.8.0', false], ['crates', '>=1.2, <1.5', '1.4.9', true], ['crates', '=1.2.3', '1.2.4', false],
    ['pypi', '>=5.2,<6', '5.2.19', true], ['pypi', '>=5.2,<6', '6.0.0', false], ['pypi', '~=2.10', '2.14.4', true], ['pypi', '~=2.10', '3.0', false],
    ['pypi', '~=1.4.2', '1.4.9', true], ['pypi', '~=1.4.2', '1.5.0', false], ['pypi', '==1.2.*', '1.2.7', true], ['pypi', '>=1,!=1.3.0', '1.3.0', false],
    ['rubygems', '~> 7.2', '7.9.0', true], ['rubygems', '~> 7.2', '8.0.0', false], ['rubygems', '~> 7.2.1', '7.3.0', false], ['rubygems', '>= 6, < 7', '6.1.7', true],
    ['hex', '~> 1.7.0', '1.7.25', true], ['hex', '~> 1.7.0', '1.8.0', false], ['hex', '>= 1.0.0 and < 2.0.0', '1.9.0', true], ['hex', '~> 1.0 or ~> 2.0', '2.3.0', true],
    ['packagist', '^12.0', 'v12.74.0', true], ['packagist', '^12.0', 'v13.0.0', false], ['packagist', '~1.2', '1.9.0', true], ['packagist', '~1.2', '2.0.0', false],
    ['packagist', '^1.0 || ^2.0', '2.4.0', true], ['packagist', '>=1.0 <1.5', '1.4.0', true],
    ['nuget', '[12.0,13.0)', '12.0.3', true], ['nuget', '[12.0,13.0)', '13.0.0', false], ['nuget', '12.0', '13.0.4', true], ['nuget', '(,2.0]', '2.0.0', true],
    ['maven', '[3.5,4.0)', '3.5.16', true], ['maven', '3.5.0', '3.5.1', false], ['pub', '^1.2.0', '1.6.0', true], ['pub', 'any', '9.0.0', true],
    ['go', '>=1.9 <2', 'v1.10.2', true], ['jsr', '^1.0', '1.1.6', true],
  ];
  for (const [ecosystem, requirement, version, expected] of cases) assert.equal(fits(ecosystem, requirement, version), expected, `${ecosystem} ${requirement} vs ${version}`);
});

test('four-part releases and Maven release qualifiers match ranges and order after their three-part base', () => {
  const rails = [{ version: '8.0.3' }, { version: '8.1.4' }, { version: '8.1.4.1' }, { version: '8.1.5.beta1' }];
  assert.equal(pick('rubygems', rails, '8.1.4.1', '~> 8.0').version, '8.1.4.1', 'the security release 8.1.4.1 wins over 8.1.4');
  assert.equal(pick('rubygems', rails, '8.1.4.1', '~> 8.1.4').version, '8.1.4.1');
  const hibernate = [{ version: '6.5.0.Final' }, { version: '6.6.1.Final' }, { version: '7.0.0.CR1' }];
  assert.equal(pick('maven', hibernate, '6.6.1.Final', '[6.0,7.0)').version, '6.6.1.Final');
  assert.equal(pick('nuget', [{ version: '4.0.0' }, { version: '4.0.0.1' }], null, '[4.0,5.0)').version, '4.0.0.1');
});

test('pick takes latest, an exact version or the newest stable match, and skips yanked versions', () => {
  const candidates = [{ version: '1.0.0' }, { version: '1.2.0' }, { version: '1.3.0', yanked: true }, { version: '2.0.0-rc.1' }, { version: '2.0.0' }];
  assert.equal(pick('crates', candidates, '2.0.0', 'latest').version, '2.0.0');
  assert.equal(pick('crates', candidates, '2.0.0', '1').version, '1.2.0', 'the yanked 1.3.0 and the release candidate are skipped');
  assert.equal(pick('crates', candidates, '2.0.0', '1.0.0').version, '1.0.0');
  assert.equal(pick('crates', candidates, '2.0.0', '>=3'), null);
  assert.equal(pick('pypi', [{ version: '1.0', yanked: true }], '1.0', 'latest').deprecated, 'yanked');
});

// A fake registry: URL → body (JSON object or text); absent URLs are 404.
const fakeGet = pages => async url => url in pages ? { body: pages[url] } : { missing: true };

test('registries report deprecation, abandonment, discontinuation, retirement and runtime requirements', async () => {
  const get = fakeGet({
    'https://proxy.golang.org/github.com/!burnt!sushi/toml/@v/list': 'v1.4.0\nv1.5.0\n',
    'https://proxy.golang.org/github.com/!burnt!sushi/toml/@latest': { Version: 'v1.5.0' },
    'https://proxy.golang.org/github.com/!burnt!sushi/toml/@v/v1.5.0.mod': '// Deprecated: use example.com/toml/v2\nmodule github.com/BurntSushi/toml\n\ngo 1.18\n',
    'https://repo.packagist.org/p2/acme/old.json': { packages: { 'acme/old': [{ version: 'v2.1.0', require: { php: '^8.1' }, abandoned: 'acme/new' }, { version: 'v2.0.0', abandoned: '__unset' }] } },
    'https://pub.dev/api/packages/pedantic': { latest: { version: '1.11.1' }, versions: [{ version: '1.11.1', pubspec: { environment: { sdk: '>=2.12.0 <3.0.0' } } }] },
    'https://pub.dev/api/packages/pedantic/options': { isDiscontinued: true, replacedBy: 'lints' },
    'https://hex.pm/api/packages/plug': { latest_stable_version: '1.0.1', releases: [{ version: '1.0.1' }, { version: '1.0.0' }], retirements: { '1.0.0': { reason: 'security', message: 'CVE' } } },
    'https://api.nuget.org/v3-flatcontainer/old.lib/index.json': { versions: ['1.0.0', '2.0.0'] },
    'https://api.nuget.org/v3/registration5-gz-semver2/old.lib/index.json': { items: [{ '@id': 'https://api.nuget.org/page/1' }] },
    'https://api.nuget.org/page/1': { items: [{ catalogEntry: { version: '2.0.0', listed: true, deprecation: { reasons: ['Legacy'] } } }] },
    'https://repo1.maven.org/maven2/org/acme/core/maven-metadata.xml': '<metadata><versioning><release>2.0.0</release><versions><version>1.0.0</version><version>2.0.0</version></versions></versioning></metadata>',
    'https://rubygems.org/api/v1/versions/nokogiri.json': [{ number: '1.18.0', platform: 'x86_64-linux', prerelease: false }, { number: '1.18.0', platform: 'ruby', prerelease: false, ruby_version: '>= 3.1' }],
    'https://jsr.io/@std/path/meta.json': { latest: '1.1.6', versions: { '1.1.6': {}, '1.1.5': { yanked: true } } },
  });
  const resolve = async (ecosystem, name, requested = 'latest') => (await resolveRegistry(get, { ecosystem, name, requested })).package;
  const go = await resolve('go', 'github.com/BurntSushi/toml');
  assert.deepEqual([go.version, go.deprecated, go.engines], ['v1.5.0', 'use example.com/toml/v2', { go: '1.18' }]);
  const composer = await resolve('packagist', 'acme/old');
  assert.deepEqual([composer.deprecated, composer.engines, composer.pin], ['abandoned; use acme/new', { php: '^8.1' }, 'acme/old:2.1.0']);
  assert.equal((await resolve('packagist', 'acme/old', '2.0.0')).deprecated, null, 'the minified diff unsets abandoned for 2.0.0');
  assert.equal((await resolve('pub', 'pedantic')).deprecated, 'discontinued; use lints');
  assert.match((await resolve('hex', 'plug', '1.0.0')).deprecated, /^retired: security/);
  assert.equal((await resolve('nuget', 'Old.Lib')).deprecated, 'deprecated: Legacy');
  assert.equal((await resolve('maven', 'org.acme:core')).pin, 'org.acme:core:2.0.0');
  const gem = await resolve('rubygems', 'nokogiri');
  assert.deepEqual([gem.engines, gem.pin], [{ ruby: '>= 3.1' }, 'gem "nokogiri", "1.18.0"']);
  assert.equal((await resolve('jsr', '@std/path', '^1')).version, '1.1.6');
  assert.deepEqual(await resolveRegistry(get, { ecosystem: 'hex', name: 'nope', requested: 'latest' }), { error: 'not-found', message: 'nope is not on the hex registry' });
});

test('pins use each tool\'s own syntax', () => {
  assert.deepEqual(['crates', 'pypi', 'go', 'nuget', 'pub', 'hex', 'jsr'].map(ecosystem => pinFor(ecosystem, ecosystem === 'jsr' ? '@std/path' : 'x', '1.0.0')),
    ['x@1.0.0', 'x==1.0.0', 'x@1.0.0', 'x@1.0.0', 'x:1.0.0', '{:x, "1.0.0"}', 'jsr:@std/path@1.0.0']);
});
