import {
  MINIFY_CONFIG,
  type CommentPatternGroup,
  type FileTypeMinifyConfig,
} from '@octocodeai/octocode-minifier';

export const KEEP_MARKER = 'KEEP_MARKER';
export const DROP_MARKER = 'DROP_MARKER';

export type LanguageBenchmarkCase = {
  readonly ext: string;
  readonly filePath: string;
  readonly content: string;
  readonly config: FileTypeMinifyConfig;
  readonly protectedMarkers: readonly string[];
  readonly droppedMarkers: readonly string[];
};

type Fixture = {
  readonly content: string;
  readonly protectedMarkers: readonly string[];
  readonly droppedMarkers: readonly string[];
};

const CSS_EXTENSIONS: readonly string[] = [
  'css',
  'less',
  'scss',
  'sass',
  'styl',
];
const HTML_EXTENSIONS: readonly string[] = [
  'html',
  'htm',
  'xml',
  'svg',
  'vue',
  'svelte',
];
const TEMPLATE_EXTENSIONS: readonly string[] = [
  'hbs',
  'handlebars',
  'ejs',
  'mustache',
  'twig',
  'jinja',
  'jinja2',
  'erb',
];

function commentGroups(
  config: FileTypeMinifyConfig
): readonly CommentPatternGroup[] {
  if (!config.comments) return [];
  return Array.isArray(config.comments) ? config.comments : [config.comments];
}

function hasCommentGroup(
  config: FileTypeMinifyConfig,
  group: CommentPatternGroup
): boolean {
  return commentGroups(config).includes(group);
}

function fixture(
  content: string,
  protectedMarkers: readonly string[] = [KEEP_MARKER],
  droppedMarkers: readonly string[] = [DROP_MARKER]
): Fixture {
  return { content, protectedMarkers, droppedMarkers };
}

function jsonFixture(ext: string): Fixture {
  if (ext === 'json') {
    return fixture(
      `{
  "name": "${KEEP_MARKER}",
  "items": [
    "alpha",
    "beta"
  ]
}`,
      [KEEP_MARKER],
      []
    );
  }

  return fixture(`// ${DROP_MARKER} json comment
{
  "name": "${KEEP_MARKER}",
  "items": [
    "alpha",
  ],
}`);
}

function markdownFixture(): Fixture {
  return fixture(`# ${KEEP_MARKER}

<!-- ${DROP_MARKER} html comment -->

> ${KEEP_MARKER} quoted note

Body text for ${KEEP_MARKER}.
`);
}

function cssFixture(): Fixture {
  return fixture(`/* ${DROP_MARKER} css comment */
.${KEEP_MARKER.toLowerCase()}::before {
  content: "/* ${KEEP_MARKER} */";
  color: red;
}
`);
}

function htmlFixture(ext: string): Fixture {
  if (ext === 'vue' || ext === 'svelte') {
    return fixture(`<!-- ${DROP_MARKER} component comment -->
<template>
  <div data-note="<!-- ${KEEP_MARKER} -->">${KEEP_MARKER}</div>
</template>
<script>
export function ${KEEP_MARKER.toLowerCase()}() {
  return "${KEEP_MARKER}";
}
</script>
`);
  }

  return fixture(`<!-- ${DROP_MARKER} html comment -->
<main data-note="<!-- ${KEEP_MARKER} -->">
  <h1>${KEEP_MARKER}</h1>
</main>
`);
}

function cStyleFixture(ext: string): Fixture {
  if (CSS_EXTENSIONS.includes(ext)) return cssFixture();

  return fixture(`/* ${DROP_MARKER} block comment */
export function ${KEEP_MARKER.toLowerCase()}() {
  const url = "https://example.com//${KEEP_MARKER}";
  const regex = /[/*]${KEEP_MARKER}/g;
  return url + regex.source;
} // ${DROP_MARKER} inline comment
`);
}

function hashFixture(ext: string): Fixture {
  const shebang = ext === 'sh' || ext === 'bash' ? '#!/bin/bash\n' : '';

  return fixture(`${shebang}# ${DROP_MARKER} hash comment
value = "# ${KEEP_MARKER}"
name = "${KEEP_MARKER}"
`);
}

function sqlFixture(): Fixture {
  return fixture(`-- ${DROP_MARKER} sql line comment
SELECT '-- ${KEEP_MARKER}' AS keep_value;
/* ${DROP_MARKER} sql block comment */
CREATE TABLE ${KEEP_MARKER.toLowerCase()} (id INT);
`);
}

function luaFixture(): Fixture {
  return fixture(`-- ${DROP_MARKER} lua line comment
local line = "-- ${KEEP_MARKER}"
local block = "--[[ ${KEEP_MARKER} ]]"
local value = "${KEEP_MARKER}" -- ${DROP_MARKER} inline comment
--[[ ${DROP_MARKER} lua block comment ]]
`);
}

function templateFixture(): Fixture {
  return fixture(`{{!-- ${DROP_MARKER} hbs comment --}}
<div data-note="{{! ${KEEP_MARKER} }}">
  ${KEEP_MARKER}
</div>
<%# ${DROP_MARKER} ejs comment %>
{# ${DROP_MARKER} twig comment #}
`);
}

function haskellFixture(): Fixture {
  return fixture(`-- ${DROP_MARKER} haskell comment
main = putStrLn "-- ${KEEP_MARKER}"
name = "{- ${KEEP_MARKER} -}"
{- ${DROP_MARKER} haskell block -}
`);
}

function semicolonFixture(): Fixture {
  return fixture(`; ${DROP_MARKER} semicolon comment
value = "; ${KEEP_MARKER}" ; ${DROP_MARKER} inline comment
name = "${KEEP_MARKER}"
`);
}

function percentFixture(): Fixture {
  return fixture(`% ${DROP_MARKER} percent comment
value() -> "% ${KEEP_MARKER}". % ${DROP_MARKER} inline comment
name() -> "${KEEP_MARKER}".
`);
}

function powerShellFixture(): Fixture {
  return fixture(`# ${DROP_MARKER} powershell comment
$value = "# ${KEEP_MARKER}"
$here = @"
# ${KEEP_MARKER}
"@
Write-Output "${KEEP_MARKER}" # ${DROP_MARKER} inline comment
<# ${DROP_MARKER} powershell block #>
`);
}

function bangFixture(): Fixture {
  return fixture(`! ${DROP_MARKER} bang comment
program ${KEEP_MARKER.toLowerCase()}
  print *, "! ${KEEP_MARKER}"
end program ${KEEP_MARKER.toLowerCase()} ! ${DROP_MARKER} inline comment
`);
}

function apostropheFixture(): Fixture {
  return fixture(`' ${DROP_MARKER} apostrophe comment
Dim value As String = "' ${KEEP_MARKER}"
Dim name As String = "${KEEP_MARKER}" ' ${DROP_MARKER} inline comment
`);
}

function doubleDashFixture(): Fixture {
  return fixture(`-- ${DROP_MARKER} double dash comment
entity ${KEEP_MARKER} is
end ${KEEP_MARKER};
signal note : string := "-- ${KEEP_MARKER}";
`);
}

function fsharpBlockFixture(): Fixture {
  return fixture(`(* ${DROP_MARKER} outer (* ${DROP_MARKER} nested *) comment *)
let ${KEEP_MARKER.toLowerCase()} = "(* ${KEEP_MARKER} *)"
`);
}

function pascalFixture(): Fixture {
  return fixture(`{ ${DROP_MARKER} pascal brace comment }
(* ${DROP_MARKER} pascal paren comment *)
procedure ${KEEP_MARKER};
begin
  WriteLn('{ ${KEEP_MARKER} }');
end;
`);
}

function wasmTextFixture(): Fixture {
  return fixture(`;; ${DROP_MARKER} wat line comment
(module
  (data ";; ${KEEP_MARKER}")
  (; ${DROP_MARKER} wat block comment ;)
  (func (export "${KEEP_MARKER.toLowerCase()}"))
)
`);
}

function objcPlusPlusFixture(): Fixture {
  return fixture(`// ${DROP_MARKER} objective-c++ comment
@interface ${KEEP_MARKER}View : NSObject
- (void)run;
@end
@implementation ${KEEP_MARKER}View
- (void)run {
  NSString *value = @"// ${KEEP_MARKER}";
}
@end
`);
}

function zigFixture(): Fixture {
  return fixture(`// ${DROP_MARKER} zig comment
pub fn ${KEEP_MARKER.toLowerCase()}() void {
    const message = "// ${KEEP_MARKER}";
    std.debug.print("{s}", .{message});
}
`);
}

function vFixture(): Fixture {
  return fixture(`// ${DROP_MARKER} v comment
fn ${KEEP_MARKER.toLowerCase()}() string {
    value := "// ${KEEP_MARKER}"
    return value
}
`);
}

function juliaFixture(): Fixture {
  return fixture(`# ${DROP_MARKER} julia comment
function ${KEEP_MARKER.toLowerCase()}()
    value = "# ${KEEP_MARKER}"
    println(value)
end
`);
}

function nixFixture(): Fixture {
  return fixture(`# ${DROP_MARKER} nix line comment
/* ${DROP_MARKER} nix block comment */
{ pkgs }:
let
  name = "# ${KEEP_MARKER}";
in
pkgs.writeShellScriptBin "${KEEP_MARKER.toLowerCase()}" ''
  echo "$name"
''
`);
}

function groovyFixture(): Fixture {
  return fixture(`// ${DROP_MARKER} groovy comment
class ${KEEP_MARKER}App {
  String run() {
    return "// ${KEEP_MARKER}"
  }
}
`);
}

function gradleFixture(): Fixture {
  return fixture(`// ${DROP_MARKER} gradle comment
plugins {
  id "java"
}

tasks.register("${KEEP_MARKER.toLowerCase()}") {
  description = "// ${KEEP_MARKER}"
}
`);
}

function hamlFixture(): Fixture {
  return fixture(`-# ${DROP_MARKER} haml comment
%div{title: "-# ${KEEP_MARKER}"} ${KEEP_MARKER}
`);
}

function slimFixture(): Fixture {
  return fixture(`/ ${DROP_MARKER} slim comment
a href="/${KEEP_MARKER}" ${KEEP_MARKER}
`);
}

function noCommentFixture(ext: string): Fixture {
  if (ext === 'csv') {
    return fixture(
      `name,value
${KEEP_MARKER},1
other,2
`,
      [KEEP_MARKER],
      []
    );
  }

  return fixture(
    `${KEEP_MARKER}


plain content for ${ext}
`,
    [KEEP_MARKER],
    []
  );
}

function fixtureFor(ext: string, config: FileTypeMinifyConfig): Fixture {
  if (ext === 'mm') return objcPlusPlusFixture();
  if (ext === 'zig') return zigFixture();
  if (ext === 'v') return vFixture();
  if (ext === 'jl') return juliaFixture();
  if (ext === 'nix') return nixFixture();
  if (ext === 'groovy') return groovyFixture();
  if (ext === 'gradle') return gradleFixture();
  if (ext === 'wat' || ext === 'wast') return wasmTextFixture();
  if (config.strategy === 'json') return jsonFixture(ext);
  if (config.strategy === 'markdown') return markdownFixture();
  if (HTML_EXTENSIONS.includes(ext) || hasCommentGroup(config, 'html')) {
    return htmlFixture(ext);
  }
  if (
    TEMPLATE_EXTENSIONS.includes(ext) ||
    hasCommentGroup(config, 'template')
  ) {
    return templateFixture();
  }
  if (hasCommentGroup(config, 'haml')) return hamlFixture();
  if (hasCommentGroup(config, 'slim')) return slimFixture();
  if (hasCommentGroup(config, 'sql')) return sqlFixture();
  if (hasCommentGroup(config, 'lua')) return luaFixture();
  if (hasCommentGroup(config, 'haskell')) return haskellFixture();
  if (hasCommentGroup(config, 'wasm-text')) return wasmTextFixture();
  if (hasCommentGroup(config, 'semicolon')) return semicolonFixture();
  if (hasCommentGroup(config, 'percent')) return percentFixture();
  if (hasCommentGroup(config, 'powershell')) return powerShellFixture();
  if (hasCommentGroup(config, 'bang')) return bangFixture();
  if (hasCommentGroup(config, 'apostrophe')) return apostropheFixture();
  if (hasCommentGroup(config, 'double-dash')) return doubleDashFixture();
  if (hasCommentGroup(config, 'fsharp-block')) return fsharpBlockFixture();
  if (hasCommentGroup(config, 'pascal')) return pascalFixture();
  if (hasCommentGroup(config, 'hash')) return hashFixture(ext);
  if (hasCommentGroup(config, 'c-style')) return cStyleFixture(ext);

  return noCommentFixture(ext);
}

export function buildLanguageBenchmarkCases(): readonly LanguageBenchmarkCase[] {
  return Object.entries(MINIFY_CONFIG.fileTypes)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([ext, config]) => {
      const currentFixture = fixtureFor(ext, config);
      return {
        ext,
        filePath: `benchmark.${ext}`,
        config,
        content: currentFixture.content,
        protectedMarkers: currentFixture.protectedMarkers,
        droppedMarkers: currentFixture.droppedMarkers,
      };
    });
}
