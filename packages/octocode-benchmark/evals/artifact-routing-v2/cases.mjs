// Authored after the original diagnostic, before any v2 inference.
// References are grader-only and must never enter a model prompt.
const call = (id, prompt, tool, query, extra = {}) => ({id, prompt, expected: {tool, equals: query, ...extra}, reference: {calls: [{tool, arguments: {queries: [query]}}], answer: ''}});
export const cases = [
  call('npm-private-exact', 'Resolve npm dependency @north/stream from https://packages.example.test/npm/. Preserve that registry URL.', 'artifactSearch', {type:'npm',packageName:'@north/stream',registry:'https://packages.example.test/npm/'}),
  call('python-exact', 'This uv project depends on rich. Get its registry metadata and upstream source.', 'artifactSearch', {type:'pypi',packageName:'rich'}),
  call('rust-discovery', 'Find Cargo libraries for Unicode normalization. No crate identifier is known.', 'artifactSearch', {type:'crates',keywords:['Unicode normalization']}, {keywordGroups:[['unicode'],['normaliz']]}),
  call('composer-exact', 'Look up the Composer coordinate symfony/console in its package registry.', 'artifactSearch', {type:'packagist',packageName:'symfony/console'}),
  call('maven-exact', 'Retrieve registry metadata for Maven coordinate org.slf4j:slf4j-api.', 'artifactSearch', {type:'maven',packageName:'org.slf4j:slf4j-api'}),
  call('nuget-exact', 'Resolve the .NET NuGet dependency Dapper.', 'artifactSearch', {type:'nuget',packageName:'Dapper'}),
  call('go-exact', 'Get published package metadata for Go module github.com/spf13/cobra.', 'artifactSearch', {type:'go',packageName:'github.com/spf13/cobra'}),
  call('ruby-discovery', 'Discover Ruby gems for Markdown rendering. I do not know a gem name.', 'artifactSearch', {type:'rubygems',keywords:['Markdown rendering']}, {keywordGroups:[['markdown']]}),
  call('remote-anchored-read', 'Read docs/security.md in octokit/octokit.js at branch release-2026. Preserve the requested revision.', 'ghGetFileContent', {owner:'octokit',repo:'octokit.js',path:'docs/security.md',branch:'release-2026'}),
  call('remote-symbol-search', 'Search the known repository sindresorhus/p-queue for occurrences of onIdle.', 'ghSearchCode', {owner:'sindresorhus',repo:'p-queue',keywords:['onIdle']}),
  call('history-search', 'Find open issues mentioning hydration in vuejs/core.', 'ghSearchHistory', {operation:'issues',owner:'vuejs',repo:'core',state:'open',keywords:['hydration']}),
  call('history-read', 'Read the body of issue 1729 in rust-lang/rust. Its identity is already known.', 'ghGetHistoryItem', {operation:'issue',owner:'rust-lang',repo:'rust',number:1729,content:{body:true}}),
  call('clone-known', 'Clone the known repository tokio-rs/bytes at branch master into the managed cache so I can inspect it locally.', 'ghCloneRepo', {owner:'tokio-rs',repo:'bytes',branch:'master'}),
  call('local-text', 'Locate literal occurrences of task_queue in local source under /workspace/project/src.', 'localSearch', {path:'/workspace/project/src',searchText:'task_queue',regex:'literal'}),
  call('local-exact', 'Read lines 18 through 36 of /workspace/project/src/scheduler.ts exactly.', 'localFetch', {path:'/workspace/project/src/scheduler.ts',startLine:18,endLine:36}),
  call('structure-files', 'List the TypeScript file paths beneath /workspace/project/src without searching their text.', 'structureSearch', {operation:'files',path:'/workspace/project/src',extensions:['ts']}),
  call('lsp-identity', 'Find semantic references to the symbol enqueue declared near line 42 of /workspace/project/src/scheduler.ts.', 'lspSearch', {operation:'references',uri:'/workspace/project/src/scheduler.ts',symbolName:'enqueue',lineHint:42}),
  {id:'unsupported-python-discovery',prompt:'Using only official PyPI package APIs, discover Python packages for terminal plotting. I have no project name.',expected:{tool:'none',refusal:'pypi'},reference:{calls:[],answer:'PyPI keyword discovery is unsupported. Please provide an exact Python package name for lookup.'}},
  {id:'clone-disabled',prompt:'Clone rust-lang/rust into the managed cache. Cloning is disabled in this session.',disabledTools:['ghCloneRepo'],expected:{tool:'none',refusal:'disabled'},reference:{calls:[],answer:'Repository cloning is disabled in this session.'}},
  {id:'independent-package-batch',prompt:'In one batch, resolve npm package kleur and Python package attrs. Both exact identifiers are known and independent.',
    expected:{tool:'artifactSearch',rows:[{type:'npm',packageName:'kleur'},{type:'pypi',packageName:'attrs'}]},
    reference:{calls:[{tool:'artifactSearch',arguments:{queries:[{type:'npm',packageName:'kleur'},{type:'pypi',packageName:'attrs'}]}}],answer:''}},
];
