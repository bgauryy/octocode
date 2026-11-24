// A rules run as SARIF 2.1.0 (https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html), the log code scanning
// reads: one result per violation, its rule as ruleId and its severity as level, located at the source module's import,
// with a codeFlows thread through the chain that proves it (CodeQL's path-problem shape), one step per hop with its
// line and the import it makes. Paths are relative to the repository that holds the graph root (%SRCROOT%), so an
// upload names the files as the repository does.
import { existsSync } from 'node:fs';
import { dirname, join, relative, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { violationKey, type Severity, type Violation } from './rules.ts';

/** A rule as the log describes it. */
export interface RuleInfo {
  id: string;
  severity: Severity;
  type: Violation['type'];
  comment?: string;
}

const LEVEL: Record<Severity, string> = {
  error: 'error',
  warn: 'warning',
  info: 'note',
};
const WHAT: Record<Violation['kind'], string> = {
  dependency: 'depends on',
  reachable: 'reaches',
  unreachable: 'is reached by no from-module:',
  circular: 'closes a cycle through',
  required: 'does not depend on',
};

/** The repository holding `root`: the nearest directory above it (or itself) with a `.git` entry; else null. */
export function repositoryOf(root: string) {
  for (let dir = root; ; dir = dirname(dir)) {
    if (existsSync(join(dir, '.git'))) return dir;
    if (dirname(dir) === dir) return null;
  }
}

/** The SARIF log of these violations (known ones are left out: the baseline accepts them). */
export function sarifLog(
  violations: readonly Violation[],
  rules: readonly RuleInfo[],
  { root, isFile }: { root: string; isFile: (id: string) => boolean }
) {
  const top = root ? repositoryOf(root) : null;
  const base = top ?? root;
  const uri = (id: string) =>
    base ? relative(base, join(root, id)).split(sep).join('/') : id;
  const at = (id: string, line: number | undefined) =>
    isFile(id)
      ? {
          physicalLocation: {
            artifactLocation: { uri: uri(id), uriBaseId: '%SRCROOT%' },
            region: { startLine: line ?? 1 },
          },
        }
      : { logicalLocations: [{ name: id, kind: 'module' }] };
  const index = new Map(rules.map((r, i) => [r.id, i]));
  const results = violations
    .filter(v => !v.known)
    .map(v => {
      const steps = v.chain.flatMap((hop, i) => {
        if (hop.nextFile)
          return [
            {
              location: {
                ...at(hop.id, hop.line),
                message: {
                  text: `imports ${hop.nextFile}${hop.kinds?.length ? ` [${hop.kinds.join(',')}${hop.load ? ` → ${hop.load}` : ''}]` : ''}`,
                },
              },
              executionOrder: i + 1,
            },
          ];
        // The last step: a file is a place to land on; an installed package has none (the step before names it).
        return isFile(hop.id)
          ? [
              {
                location: {
                  ...at(hop.id, undefined),
                  message: {
                    text:
                      v.kind === 'circular'
                        ? 'back at the start of the cycle'
                        : 'reached',
                  },
                },
                executionOrder: i + 1,
              },
            ]
          : [];
      });
      return {
        ruleId: v.rule,
        ruleIndex: index.get(v.rule) ?? 0,
        level: LEVEL[v.severity],
        message: {
          text: `${v.from} ${WHAT[v.kind]} ${v.to}${v.comment ? `: ${v.comment}` : ''}`,
        },
        locations: [at(v.from, v.chain[0]?.line)],
        ...(steps.length > 1 && {
          codeFlows: [{ threadFlows: [{ locations: steps }] }],
        }),
        partialFingerprints: { 'octocodeGraphViolation/v1': violationKey(v) },
        properties: {
          kind: v.kind,
          from: v.from,
          to: v.to,
          ...(v.cycle && { cycle: v.cycle }),
        },
      };
    });
  return {
    $schema: 'https://json.schemastore.org/sarif-2.1.0.json',
    version: '2.1.0',
    runs: [
      {
        tool: {
          driver: {
            name: 'octocode-graph',
            informationUri:
              'https://github.com/bgauryy/octocode/tree/main/packages/octocode-graph',
            rules: rules.map(r => ({
              id: r.id,
              shortDescription: { text: r.comment ?? `${r.type} rule ${r.id}` },
              defaultConfiguration: { level: LEVEL[r.severity] },
              properties: { type: r.type },
            })),
          },
        },
        ...(base && {
          originalUriBaseIds: {
            '%SRCROOT%': { uri: pathToFileURL(`${base}/`).href },
          },
        }),
        results,
      },
    ],
  };
}
