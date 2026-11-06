// What the recorded-answer store holds: one tool call per case, against public
// packages and repositories only. `maps` names objects keyed by data, for field
// paths; `whole` the lists a tool routes by; `also` records a real answer the
// current code does not ask for, so old or wrong code gets the provider's answer,
// not a miss.
const repo = { owner: 'octocat', repo: 'Hello-World' };
const NUGET_REG = 'https://api.nuget.org/v3';
const NUGET_WHOLE = ['resources'];
const MAVEN_REPOSITORIES = ['project.url.text', 'project.scm.url.text', 'project.scm.connection.text', 'project.scm.developerConnection.text'];
const NPM_MAPS = ['versions', 'time', 'versions.*.dependencies', 'versions.*.devDependencies', 'versions.*.scripts', 'versions.*.directories', 'versions.*.bin', 'versions.*.engines', 'dependencies', 'devDependencies', 'scripts', 'users'];

export const EXTRA_HOSTS = ['api.github.com', 'azuresearch-usnc.nuget.org', 'azuresearch-ussc.nuget.org', 'search.maven.org', 'static.crates.io', 'index.crates.io'];

export const PLAN = [
  // OSV: `advisory` shows the whole record; `package` the brief one.
  { id: 'osv/advisory-ghsa', tool: 'securityCheck', query: { operation: 'advisory', id: 'GHSA-29mw-wpgm-hmr9' } },
  { id: 'osv/advisory-ossfuzz', tool: 'securityCheck', query: { operation: 'advisory', id: 'OSV-2020-111' } },
  { id: 'osv/advisory-unknown', tool: 'securityCheck', query: { operation: 'advisory', id: 'GHSA-0000-0000-0000' }, errorCode: 'notFound' },
  { id: 'osv/package-lodash', tool: 'securityCheck', query: { operation: 'package', ecosystem: 'npm', packageName: 'lodash', version: '4.17.20' }, maps: NPM_MAPS },
  // A record with a score and no severity word: the brief row shows the vector.
  { id: 'osv/package-score-only', tool: 'securityCheck', query: { operation: 'package', ecosystem: 'pypi', packageName: 'requests', version: '2.19.0' } },
  { id: 'osv/commit', tool: 'securityCheck', query: { operation: 'commit', commit: '6879efc2c1596d11a6a6ad296f80063b558d5e0f' } },
  // NuGet: the lower hive is real too, and older code read it.
  { id: 'nuget/exact-advisory', whole: NUGET_WHOLE, tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'nuget', packageName: 'Newtonsoft.Json', version: '12.0.3' }, also: [{ method: 'GET', url: `${NUGET_REG}/registration5-semver1/newtonsoft.json/index.json`, accept: 'application/json' }, { method: 'GET', url: `${NUGET_REG}/registration5-gz-semver1/newtonsoft.json/index.json`, accept: 'application/json' }] },
  { id: 'nuget/search', whole: NUGET_WHOLE, tool: 'artifactSearch', query: { operation: 'search', ecosystem: 'nuget', keywords: ['Newtonsoft.Json.Bson'], limit: 5 } },
  { id: 'npm/exact', tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'npm', packageName: 'left-pad', version: '1.3.0' }, maps: NPM_MAPS },
  { id: 'npm/search', tool: 'artifactSearch', query: { operation: 'search', ecosystem: 'npm', keywords: ['left-pad'], limit: 5 }, maps: NPM_MAPS },
  { id: 'pypi/exact-advisory', tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'pypi', packageName: 'requests', version: '2.19.0' }, maps: ['releases', 'info.project_urls', 'urls[*].digests', 'releases.*[*].digests'] },
  { id: 'crates/exact', tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'crates', packageName: 'cfg-if', version: '1.0.0' } },
  { id: 'crates/search', tool: 'artifactSearch', query: { operation: 'search', ecosystem: 'crates', keywords: ['regex-syntax'], limit: 5 } },
  { id: 'maven/exact', repositories: MAVEN_REPOSITORIES, tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'maven', packageName: 'com.google.code.gson:gson', version: '2.8.9' } },
  { id: 'maven/search', repositories: MAVEN_REPOSITORIES, whole: ['response.docs'], tool: 'artifactSearch', query: { operation: 'search', ecosystem: 'maven', keywords: ['gson'], limit: 5 } },
  { id: 'go/exact', dates: ['Time'], tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'go', packageName: 'github.com/pkg/errors', version: 'v0.9.1' } },
  { id: 'packagist/exact', tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'packagist', packageName: 'psr/log', version: '1.1.4' }, maps: ['packages', 'packages.*[*].require', 'packages.*[*].require-dev', 'packages.*[*].autoload', 'packages.*[*].extra', 'packages.*[*].suggest'] },
  { id: 'packagist/search', tool: 'artifactSearch', query: { operation: 'search', ecosystem: 'packagist', keywords: ['psr log'], limit: 5 } },
  { id: 'rubygems/exact', tool: 'artifactSearch', query: { operation: 'resolve', ecosystem: 'rubygems', packageName: 'rack', version: '2.2.3' } },
  { id: 'rubygems/search', tool: 'artifactSearch', query: { operation: 'search', ecosystem: 'rubygems', keywords: ['rack-test'], limit: 5 } },
  // GitHub REST and GraphQL, one public repository named on every call.
  { id: 'github/repo-get', tool: 'ghSearchRepo', query: { operation: 'get', ...repo }, github: true },
  { id: 'github/structure-tree', tool: 'ghStructure', query: { operation: 'tree', ...repo, ref: 'master' }, github: true },
  { id: 'github/file', tool: 'ghGetFileContent', query: { ...repo, path: 'README', ref: 'master' }, github: true },
  { id: 'github/code-search', tool: 'ghSearchCode', query: { ...repo, keywords: ['Hello'] }, github: true },
  { id: 'github/history-commits', tool: 'ghSearchHistory', query: { operation: 'commit', ...repo, limit: 3 }, github: true },
  { id: 'github/history-pulls', tool: 'ghSearchHistory', query: { operation: 'pullRequest', ...repo, keywords: ['Create'], limit: 3 }, github: true },
  { id: 'github/item-commit', tool: 'ghGetHistoryItem', query: { operation: 'commit', ...repo, ref: '7fd1a60b01f91b314f59955a4e4d4e80d8edf11d' }, github: true },
  { id: 'github/item-pull', tool: 'ghGetHistoryItem', query: { operation: 'pullRequest', ...repo, number: 1, sections: ['files'] }, github: true },
];

export const planOf = id => PLAN.find(entry => entry.id === id);
