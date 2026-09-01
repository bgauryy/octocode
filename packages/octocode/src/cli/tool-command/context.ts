// The full agent-context dump (`context` command / `--full`): protocol,
// system prompt, and per-tool descriptions grouped by category.
import {
  getDirectToolCategory,
  getDirectToolDescription,
  sortDirectToolNames,
} from '@octocodeai/octocode-tools-core/schema';
import {
  TOOL_DEFINITIONS,
  getToolEnableInstruction,
  getToolAvailability,
  loadToolMetadata,
} from './registry.js';
import { formatConciseToolDescription } from './formatting.js';
import {
  AGENT_TOOL_COMMANDS,
  BATCH_ERROR_GUIDANCE,
  CHEAP_VIEW_GUIDANCE,
  CONTINUATION_GUIDANCE,
} from './agent-contract.js';

export async function getToolsContextString(
  options: { full?: boolean; minimal?: boolean } = {}
): Promise<string> {
  const full = options.full === true;
  const minimal = options.minimal === true;
  const metadata = await loadToolMetadata();
  const toolNames = sortDirectToolNames(
    TOOL_DEFINITIONS.map(tool => tool.name)
  );

  if (minimal && !full) {
    const byCategory = new Map<string, string[]>();
    for (const toolName of toolNames) {
      const category = getDirectToolCategory(toolName);
      byCategory.set(category, [...(byCategory.get(category) ?? []), toolName]);
    }
    const lines = [
      'Octocode CLI — Minimal Context',
      'Protocol: schema first → orient → search → read exact → prove → decide.',
      `Run: ${AGENT_TOOL_COMMANDS.catalog} | ${AGENT_TOOL_COMMANDS.schema} | ${AGENT_TOOL_COMMANDS.run}`,
      `Output: YAML default; --compact structured JSON. ${BATCH_ERROR_GUIDANCE}`,
      `Tools (${toolNames.length}):`,
    ];
    for (const [category, names] of byCategory) {
      lines.push(
        `  ${category}: ${names
          .map(name => {
            const availability = getToolAvailability(name);
            return availability.enabled
              ? name
              : `${name} [disabled: ${availability.envVar}]`;
          })
          .join(', ')}`
      );
    }
    return lines.join('\n');
  }

  const protocol = full
    ? [
        'Full MCP prompt + tool descriptions. Schemas stay on demand.',
        '  *** TOOL CALLS ***',
        '  tools --json --compact | tools <name> --scheme --json --compact | tools <name> --scheme',
        "  tools <name> --queries '<json>' [--compact|--json]",
        '  YAML is default; --compact is typed JSON; --json is the full envelope.',
        '  *** RESEARCH LOOP ***  orient → search → read exact → prove → decide.',
        `  ${CHEAP_VIEW_GUIDANCE}`,
        `  ${CONTINUATION_GUIDANCE}`,
        '  Exit: 0 command completed (inspect row statuses) · 2 input · 3 not-found · 4 auth · 5 tool · 7 rate-limit.',
        '  `cache fetch` materializes content locally; every other capability (files, trees, content, repos, packages, PRs, history, diffs) runs through `tools <name>` — read its schema first.',
      ]
    : [
        'Compact context; use `context --full` for MCP instructions + long descriptions.',
        'Protocol: schema first → orient → search → read exact → prove → decide.',
        "Commands: tools --json --compact | tools <name> --scheme --json --compact | tools <name> --queries '<json>' --compact",
        CHEAP_VIEW_GUIDANCE,
        CONTINUATION_GUIDANCE,
        'Proof: snippets are discovery, not proof; use exact reads, PR/commit evidence, or LSP.',
      ];

  const sections: string[] = [
    'Octocode CLI — Agent Context',
    protocol.join('\n'),
    '',
    full
      ? [
          'Agent System Prompt (Octocode MCP Instructions):',
          metadata.systemPrompt.trim(),
        ].join('\n')
      : 'MCP instructions omitted; use `context --full` when needed.',
    '',
    'Output contract (all tools):',
    (full
      ? [
          '  CLI default: YAML from content[].text. --compact: structuredContent JSON. --json: full CallToolResult.',
          '  MCP returns complete YAML text in content[].text plus full structuredContent.',
          '',
          '  --json envelope:',
          '    isError: boolean                       true = call/all rows failed; inspect each row status for mixed batches',
          '    content[].text: string                 YAML string (same as default output)',
          '    structuredContent.results[]: array     ordered rows: index, optional status/meta, and data',
          '    structuredContent.results[].data: object tool payload; continuations stay with their row',
          '    structuredContent.base: string         cwd / workspace root used for the query',
          '    structuredContent.responsePagination: object content[].text char window; structured data remains complete',
          '    structuredContent.results[].data.pagination: object page state; advance only while hasMore',
          '    structuredContent.results[].data.next: object typed follow-up calls for that row',
          '    structuredContent.results[].data.location: object where fetched or cloned content was saved',
        ]
      : [
          '  Default: YAML. --compact: lean structuredContent JSON. --json: full CallToolResult.',
          `  ${BATCH_ERROR_GUIDANCE}`,
        ]
    ).join('\n'),
    '',
    `Tools (${toolNames.length}, grouped by source):`,
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
      const availability = getToolAvailability(toolName);
      const availabilitySuffix = availability.enabled
        ? ''
        : ` [disabled: ${getToolEnableInstruction(toolName) ?? availability.envVar}]`;
      if (full) {
        sections.push(`  ${toolIndex}. ${toolName}${availabilitySuffix}`);
        sections.push(description.trim());
      } else {
        sections.push(
          `  ${toolIndex}. ${toolName}${availabilitySuffix} — ${formatConciseToolDescription(toolName, metadata, 96)}`
        );
      }
    }
    sections.push('');
  }

  sections.push(
    'Schemas are not shown here — inspect before an unfamiliar or hand-authored call:'
  );
  sections.push(
    '  tools <name> --scheme --brief    # lean signatures + example',
    '  tools <n1> <n2> ... --scheme     # several tools at once'
  );

  return sections.join('\n').trim();
}

export async function printToolsContext(
  options: { full?: boolean; minimal?: boolean } = {}
): Promise<void> {
  console.log(await getToolsContextString(options));
}
