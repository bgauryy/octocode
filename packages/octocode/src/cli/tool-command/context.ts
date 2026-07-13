// The full agent-context dump (`context` command / `--full`): protocol,
// system prompt, and per-tool descriptions grouped by category.
import {
  getDirectToolCategory,
  getDirectToolDescription,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import { loadToolMetadata } from './registry.js';
import { extractShortDescription } from './formatting.js';

export async function getToolsContextString(
  options: { full?: boolean } = {}
): Promise<string> {
  const full = options.full === true;
  const metadata = await loadToolMetadata();
  const toolNames = sortDirectToolNames(Object.keys(metadata.tools));

  const sections: string[] = [
    'Octocode CLI — Agent Context',
    [
      full
        ? 'Agent context: protocol, system prompt, full tool descriptions. Schemas are read separately, on demand.'
        : 'Agent context: protocol, system prompt, short tool descriptions. Use --full for complete descriptions; read schemas separately.',
      'Follow this protocol:',
      '',
      '  *** SCHEMA CHECK — REQUIRED BEFORE EVERY RAW TOOL CALL ***',
      '  This context lists what each tool is for. It does NOT include schemas —',
      "  read a tool's schema before calling it:",
      '    tools --json                   # lean machine catalog, no full schemas',
      '    tools <name> --scheme           # schema: fields, types, bounds, defaults',
      '    tools <name> --scheme --json    # one machine-readable schema',
      '    tools <name>                    # same schema/help shortcut',
      '    tools <n1> <n2> ... --scheme    # batch: read multiple schemas at once',
      '    tools --json --full             # full all-tool schema dump; expensive, rare',
      '',
      '  *** RESEARCH LOOP ***',
      '  1. Orient: localViewStructure / ghViewRepoStructure / npmSearch.',
      '  2. Search: localSearchCode / ghSearchCode. Use localSearchCode mode:"structural" for AST/code-shape anchors.',
      '  3. Read: localGetFileContent / ghGetFileContent — smallest slice, choose minify standard|symbols|none.',
      '  4. Prove: lspGetSemantics or ghHistoryResearch; LSP consumes the file/line anchors from text or structural search.',
      '',
      '  *** ORIENT CHEAP — BEFORE READING ***',
      '  concise:true         flat string lists — ghSearchRepos→"owner/repo", ghSearchCode→"owner/repo:path", ghHistoryResearch list→"#number title"',
      '  mode:"discovery"     localSearchCode paths only, no snippets (~80% cheaper than paginated)',
      '  minify:"symbols"     skeleton+line-gutter — orient any unknown file first; never paginated',
      '  minify:"standard"    strips comments/blanks — default read mode',
      '  minify:"none"        exact raw text — for quotes, diffs, exact matching',
      '',
      '  *** PAGINATION ***',
      '  Read the typed fields — pagination (nextPage/nextCharOffset) and next carry the exact follow-up params.',
      '  Page only when pagination.hasMore or contentPagination.*.hasMore is true; narrow scope before paging.',
      '  responseCharLength/responseCharOffset (root params, siblings of queries) cap the whole envelope.',
      '',
      '  *** TOOL CALLS ***',
      '  tools --json                                  # lean catalog; choose one tool',
      '  tools <name> --scheme --json                  # one tool schema; avoid all-schema dumps',
      "  tools <name> --queries '<json>'           # run tool, YAML output",
      "  tools <name> --queries '<json>' --json    # run tool, full CallToolResult JSON",
      "  tools <name> --queries '<json>' --compact # run tool, lean structuredContent JSON",
      "  search --query '<oql-json>' --json        # native OQL envelope JSON (results are OQL rows, not CallToolResult)",
      '',
      '  Output: clean YAML by default; use --compact for lean structuredContent JSON, --json for the full CallToolResult envelope.',
      '',
      '  Exit codes: 0=ok  2=bad-input  3=not-found  4=auth  5=tool-error  7=rate-limited',
      '',
      '  *** REFERENCES ***',
      '  Docs:  https://github.com/bgauryy/octocode/tree/main/docs',
      '  Quick commands (search/clone/cache fetch) are the fastest path; use search for files, trees, content, repos, packages, PRs, history, and diffs. Raw `tools` need a schema read first.',
      '  Do not hallucinate paths, lines, or fields — verify with the tools; snippets are discovery, not proof.',
      '',
    ].join('\n'),
    '',
    'Agent System Prompt (Octocode MCP Instructions):',
    metadata.systemPrompt.trim(),
    '',
    'Output contract (all tools):',
    [
      '  Default output: clean YAML — read it directly. No parsing needed.',
      '  Add --compact for lean structuredContent JSON. Add --json for the full CallToolResult envelope below.',
      '',
      '  --json envelope:',
      '    isError: boolean                       true = tool failed',
      '    content[].text: string                 YAML string (same as default output)',
      '    structuredContent.results[]: array     tool result objects; most tools use id + data',
      '    structuredContent.results[].files[]     ghGetFileContent grouped fetch entries; data aliases the same group',
      '    structuredContent.base: string         cwd / workspace root used for the query',
      '    structuredContent.pagination: object   nextPage / nextCharOffset — page only when present',
      '    structuredContent.next: object         typed follow-up params for the next call',
      '    structuredContent.location: object     where remote content was saved (kind, localPath, repoRoot, ...)',
      '    structuredContent.warnings[]: string[] non-fatal issues to account for',
      '    structuredContent.error: object        failure detail when isError is true',
    ].join('\n'),
    '',
    'Tools (grouped by source):',
  ];

  const CATEGORY_ORDER: Array<{
    cat: ReturnType<typeof getDirectToolCategory>;
    label: string;
  }> = [
    { cat: 'GitHub', label: 'GitHub' },
    { cat: 'Local Code', label: 'Local Code' },
    { cat: 'Package', label: 'npm' },
    { cat: 'Other', label: 'Other' },
  ];

  let toolIndex = 0;
  for (const { cat, label } of CATEGORY_ORDER) {
    const inCategory = toolNames.filter(
      toolName => getDirectToolCategory(toolName) === cat
    );
    if (inCategory.length === 0) continue;

    sections.push(`${label}:`);
    for (const toolName of inCategory) {
      toolIndex += 1;
      const description = getDirectToolDescription(toolName, metadata);
      if (full) {
        sections.push(`  ${toolIndex}. ${toolName}`);
        sections.push(description.trim());
      } else {
        sections.push(
          `  ${toolIndex}. ${toolName} — ${extractShortDescription(description)}`
        );
      }
    }
    sections.push('');
  }

  sections.push(
    'Schemas are not shown here — read them on demand (required before any call):'
  );
  sections.push(
    '  tools <name> --scheme            # one tool',
    '  tools <n1> <n2> ... --scheme     # several tools at once'
  );

  return sections.join('\n').trim();
}

export async function printToolsContext(
  options: { full?: boolean } = {}
): Promise<void> {
  console.log(await getToolsContextString(options));
}
