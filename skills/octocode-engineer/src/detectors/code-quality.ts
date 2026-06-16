import * as ts from 'typescript';

import { canAddFinding } from './shared.js';
import { isTestFile } from '../common/utils.js';

import type { FindingDraft } from './shared.js';
import type {
  DependencyState,
  DuplicateGroup,
  FileEntry,
  Finding,
  FlowMapEntry,
  RedundantFlowGroup,
} from '../types/index.js';

export function detectDuplicateFunctionBodies(
  duplicates: DuplicateGroup[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const group of duplicates) {
    const sample = group.locations[0];
    const reason =
      `Same ${group.kind} body shape appears in ${group.occurrences} places (` +
      `${group.filesCount} file${group.filesCount > 1 ? 's' : ''}).`;
    const severity: Finding['severity'] =
      group.occurrences >= 6
        ? 'high'
        : group.occurrences >= 3
          ? 'medium'
          : 'low';
    if (!canAddFinding(findings)) break;
    findings.push({
      ...sample,
      severity,
      category: 'duplicate-function-body',
      title: `Deduplicate function body: ${group.signature}`,
      reason,
      files: group.locations.map(
        loc => `${loc.file}:${loc.lineStart}-${loc.lineEnd}`
      ),
      suggestedFix: {
        strategy:
          'Create a shared helper function once and replace duplicate call sites.',
        steps: [
          'Extract one function to a dedicated utility module.',
          'Keep behavior unchanged by passing function-specific differences as params.',
          'Replace duplicated blocks with calls to the shared helper.',
          'Add/extend tests around each entry point that previously used duplicates.',
        ],
      },
      impact: `Lower maintenance cost and reduce regression risk when behavior changes.`,
      tags: ['duplication', 'maintainability', 'dryness'],
      lspHints: [
        {
          tool: 'lspGetSemantics', semanticType: 'definition',
          symbolName: group.signature,
          lineHint: sample.lineStart,
          file: sample.file,
          expectedResult: `navigate to one instance to compare implementations side-by-side`,
        },
      ],
    });
  }
  return findings;
}

export function detectDuplicateFlowStructures(
  controlDuplicates: RedundantFlowGroup[],
  flowDupThreshold: number
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const group of controlDuplicates) {
    if (group.occurrences < flowDupThreshold) continue;
    const sample = group.locations[0];
    const reason = `${group.kind} structure appears ${group.occurrences} times across ${group.filesCount} file(s).`;
    const severity: Finding['severity'] =
      group.occurrences >= 10 ? 'high' : 'medium';
    if (!canAddFinding(findings)) break;
    findings.push({
      ...sample,
      severity,
      category: 'duplicate-flow-structure',
      title: `Extract repeated flow structure: ${group.kind}`,
      reason,
      files: group.locations.map(
        loc => `${loc.file}:${loc.lineStart}-${loc.lineEnd}`
      ),
      suggestedFix: {
        strategy:
          'Extract a reusable flow helper around the repeated structure.',
        steps: [
          'Create one clear helper that accepts varying inputs as parameters.',
          'Call helper from each repeated site.',
          'Keep variable names aligned and add local adapter logic where needed.',
          'Document expected invariants for the shared flow.',
        ],
      },
      impact: `Reduces duplicate control branches and normalizes edge-case handling.`,
      tags: ['duplication', 'control-flow', 'dryness'],
    });
  }
  return findings;
}

export function detectFunctionOptimization(
  fileSummaries: FileEntry[],
  criticalComplexityThreshold: number
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const fileEntry of fileSummaries) {
    for (const fn of fileEntry.functions) {
      const alerts: string[] = [];
      if (fn.complexity >= criticalComplexityThreshold)
        alerts.push(
          `Cyclomatic-like complexity is high (>=${criticalComplexityThreshold}).`
        );
      if (fn.maxBranchDepth >= 7)
        alerts.push('Branch depth is very deep and hard to reason about.');
      if (fn.maxLoopDepth >= 4)
        alerts.push('Nested loops are high and likely expensive.');
      if (fn.statementCount >= 24)
        alerts.push(
          'Function body is large and may be doing multiple responsibilities.'
        );

      if (alerts.length === 0) continue;

      const isHigh =
        fn.complexity >= criticalComplexityThreshold ||
        fn.maxBranchDepth >= 7 ||
        fn.maxLoopDepth >= 4;
      findings.push({
        ...fn,
        severity: isHigh ? 'high' : 'medium',
        category: 'function-optimization',
        title: `Potential function refactor: ${fn.name}`,
        reason: alerts.join(' '),
        files: [`${fn.file}:${fn.lineStart}-${fn.lineEnd}`],
        suggestedFix: {
          strategy: 'Refactor for readability and testability.',
          steps: [
            'Split into smaller subroutines with single responsibilities.',
            'Convert deeply nested branches into guard clauses when safe.',
            'Replace loops with intent-specific helpers if one loop owns most lines.',
            'Add unit coverage for each extracted piece before deleting old logic.',
          ],
        },
        impact: 'Cleaner flow, easier review and safer refactors.',
        tags: ['complexity', 'readability', 'refactor'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'callers',
            symbolName: fn.name,
            lineHint: fn.lineStart,
            file: fn.file,
            expectedResult: `inspect callers and callees to plan safe decomposition of ${fn.name}`,
          },
        ],
      });
    }
  }
  return findings;
}

export function computeCognitiveComplexity(node: ts.Node): number {
  let total = 0;

  const visit = (current: ts.Node, nesting: number): void => {
    let increment = 0;
    let nestable = false;

    switch (current.kind) {
      case ts.SyntaxKind.IfStatement:
      case ts.SyntaxKind.ForStatement:
      case ts.SyntaxKind.ForInStatement:
      case ts.SyntaxKind.ForOfStatement:
      case ts.SyntaxKind.WhileStatement:
      case ts.SyntaxKind.DoStatement:
      case ts.SyntaxKind.CatchClause:
      case ts.SyntaxKind.ConditionalExpression:
      case ts.SyntaxKind.SwitchStatement:
        increment = 1;
        nestable = true;
        break;
      default:
        break;
    }

    if (
      current.kind === ts.SyntaxKind.BinaryExpression &&
      ((current as ts.BinaryExpression).operatorToken.kind ===
        ts.SyntaxKind.AmpersandAmpersandToken ||
        (current as ts.BinaryExpression).operatorToken.kind ===
          ts.SyntaxKind.BarBarToken ||
        (current as ts.BinaryExpression).operatorToken.kind ===
          ts.SyntaxKind.QuestionQuestionToken)
    ) {
      increment = 1;
    }

    if (
      current.kind === ts.SyntaxKind.IfStatement &&
      current.parent &&
      ts.isIfStatement(current.parent) &&
      current.parent.elseStatement === current
    ) {
      increment = 1;
      nestable = false;
    }

    if (nestable) {
      total += increment + nesting;
      ts.forEachChild(current, child => visit(child, nesting + 1));
      return;
    }

    total += increment;
    ts.forEachChild(current, child => visit(child, nesting));
  };

  visit(node, 0);
  return total;
}

export function detectCognitiveComplexity(
  fileSummaries: FileEntry[],
  threshold: number = 15
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      if (fn.cognitiveComplexity > threshold) {
        findings.push({
          severity: fn.cognitiveComplexity > 25 ? 'high' : 'medium',
          category: 'cognitive-complexity',
          file: entry.file,
          lineStart: fn.lineStart,
          lineEnd: fn.lineEnd,
          title: `High cognitive complexity: ${fn.name} (${fn.cognitiveComplexity})`,
          reason: `Function cognitive complexity is ${fn.cognitiveComplexity} (threshold: ${threshold}). Nested branches compound reading difficulty.`,
          files: [`${entry.file}:${fn.lineStart}-${fn.lineEnd}`],
          suggestedFix: {
            strategy: 'Reduce nesting and simplify control flow.',
            steps: [
              'Convert nested branches into early returns / guard clauses.',
              'Extract deeply nested blocks into named helper functions.',
              'Replace complex boolean chains with named predicates.',
            ],
          },
          impact:
            'Lower cognitive complexity directly correlates with fewer bugs and faster code reviews.',
          tags: ['complexity', 'readability', 'nesting'],
          lspHints: [
            {
              tool: 'lspGetSemantics', semanticType: 'callers',
              symbolName: fn.name,
              lineHint: fn.lineStart,
              file: entry.file,
              expectedResult: `understand call graph before simplifying ${fn.name}`,
            },
          ],
        });
      }
    }
  }

  return findings;
}

export function detectExcessiveParameters(
  fileSummaries: FileEntry[],
  threshold: number = 5
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      if (fn.params == null || fn.params <= threshold) continue;
      findings.push({
        severity: fn.params > 7 ? 'high' : 'medium',
        category: 'excessive-parameters',
        file: entry.file,
        lineStart: fn.lineStart,
        lineEnd: fn.lineEnd,
        title: `Excessive parameters: ${fn.name} (${fn.params} params)`,
        reason: `Function has ${fn.params} parameters (threshold: ${threshold}). High parameter counts make call sites hard to read and signal the function may be doing too much.`,
        files: [`${entry.file}:${fn.lineStart}-${fn.lineEnd}`],
        suggestedFix: {
          strategy: 'Introduce a parameter object or split the function.',
          steps: [
            'Group related parameters into an options/config object.',
            'Use destructuring at the function signature for clarity.',
            'Consider splitting into smaller, focused functions if params serve different concerns.',
          ],
        },
        impact:
          'Improves call-site readability and makes the API easier to evolve.',
        tags: ['api-design', 'readability', 'refactor'],
      });
    }
  }
  return findings;
}

export function detectEmptyCatchBlocks(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (!entry.emptyCatches || entry.emptyCatches.length === 0) continue;
    for (const loc of entry.emptyCatches) {
      findings.push({
        severity: 'medium',
        category: 'empty-catch',
        file: entry.file,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        title: `Empty catch block silently swallows errors`,
        reason: `Catch block at line ${loc.lineStart} has no statements — errors are silently ignored.`,
        files: [`${entry.file}:${loc.lineStart}-${loc.lineEnd}`],
        suggestedFix: {
          strategy: 'Log, re-throw, or handle the error explicitly.',
          steps: [
            'Add error logging (console.error or a logger) at minimum.',
            'Re-throw if the caller should handle the error.',
            'Add a comment explaining why swallowing is intentional, if it truly is.',
          ],
        },
        impact:
          'Prevents silent failures that are extremely hard to debug in production.',
        tags: ['error-handling', 'reliability', 'silent-failure'],
      });
    }
  }
  return findings;
}

export function detectSwitchNoDefault(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (
      !entry.switchesWithoutDefault ||
      entry.switchesWithoutDefault.length === 0
    )
      continue;
    for (const loc of entry.switchesWithoutDefault) {
      findings.push({
        severity: 'low',
        category: 'switch-no-default',
        file: entry.file,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        title: `Switch statement missing default case`,
        reason: `Switch at line ${loc.lineStart} has no default clause — unexpected values fall through silently.`,
        files: [`${entry.file}:${loc.lineStart}-${loc.lineEnd}`],
        suggestedFix: {
          strategy:
            'Add a default case with error handling or exhaustive check.',
          steps: [
            'Add a default clause that throws an unreachable error for exhaustiveness.',
            'Or log a warning for unexpected values.',
            'In TypeScript, use `never` type assertion for compile-time exhaustive checks.',
          ],
        },
        impact:
          'Catches unexpected values early and prevents silent logic bugs.',
        tags: ['control-flow', 'exhaustiveness', 'safety'],
      });
    }
  }
  return findings;
}

export function detectUnsafeAny(
  fileSummaries: FileEntry[],
  threshold: number = 5
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (entry.anyCount == null || entry.anyCount <= threshold) continue;
    if (!canAddFinding(findings)) break;
    findings.push({
      severity: entry.anyCount > 10 ? 'high' : 'medium',
      category: 'unsafe-any',
      file: entry.file,
      lineStart: 1,
      lineEnd: 1,
      title: `Excessive \`any\` usage: ${entry.file} (${entry.anyCount} occurrences)`,
      reason: `File uses \`any\` type ${entry.anyCount} times (threshold: ${threshold}). Each \`any\` disables type checking and allows silent runtime errors.`,
      files: [entry.file],
      suggestedFix: {
        strategy: 'Replace `any` with specific types, `unknown`, or generics.',
        steps: [
          'Replace `any` with `unknown` and add type guards where needed.',
          'Use generics for functions that operate on multiple types.',
          'Define proper interfaces for complex data shapes.',
          'Use `as const` assertions instead of `as any` where possible.',
        ],
      },
      impact:
        'Restores TypeScript safety and catches bugs at compile time instead of runtime.',
      tags: ['type-safety', 'reliability', 'typescript'],
    });
  }
  return findings;
}

export function detectHighHalsteadEffort(
  fileSummaries: FileEntry[],
  effortThreshold: number = 500_000,
  bugThreshold: number = 2.0
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      if (!fn.halstead) continue;
      const { effort, estimatedBugs, volume } = fn.halstead;
      if (effort <= effortThreshold && estimatedBugs <= bugThreshold) continue;
      const reasons: string[] = [];
      if (effort > effortThreshold)
        reasons.push(
          `effort=${Math.round(effort)} (threshold: ${effortThreshold})`
        );
      if (estimatedBugs > bugThreshold)
        reasons.push(
          `estimatedBugs=${estimatedBugs.toFixed(2)} (threshold: ${bugThreshold})`
        );
      findings.push({
        severity:
          effort > effortThreshold * 2 || estimatedBugs > 5 ? 'high' : 'medium',
        category: 'halstead-effort',
        file: entry.file,
        lineStart: fn.lineStart,
        lineEnd: fn.lineEnd,
        title: `High Halstead complexity: ${fn.name}`,
        reason: `Function has high implementation complexity: ${reasons.join('; ')}. Volume=${Math.round(volume)}.`,
        files: [`${entry.file}:${fn.lineStart}-${fn.lineEnd}`],
        suggestedFix: {
          strategy:
            'Reduce operator/operand count by extracting helpers and simplifying expressions.',
          steps: [
            'Extract complex sub-expressions into named intermediate variables.',
            'Split into smaller functions with fewer unique operators/operands.',
            'Replace imperative loops with declarative array methods where clearer.',
          ],
        },
        impact:
          'Lower Halstead effort correlates with fewer bugs and faster comprehension.',
        tags: ['complexity', 'maintainability', 'effort'],
      });
    }
  }
  return findings;
}

export function detectLowMaintainability(
  fileSummaries: FileEntry[],
  threshold: number = 20
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      if (
        fn.maintainabilityIndex == null ||
        fn.maintainabilityIndex >= threshold
      )
        continue;
      findings.push({
        severity: fn.maintainabilityIndex < 10 ? 'critical' : 'high',
        category: 'low-maintainability',
        file: entry.file,
        lineStart: fn.lineStart,
        lineEnd: fn.lineEnd,
        title: `Low maintainability: ${fn.name} (MI=${fn.maintainabilityIndex.toFixed(1)})`,
        reason: `Maintainability Index is ${fn.maintainabilityIndex.toFixed(1)} (threshold: ${threshold}, scale 0-100). Combines Halstead volume, cyclomatic complexity, and lines of code.`,
        files: [`${entry.file}:${fn.lineStart}-${fn.lineEnd}`],
        suggestedFix: {
          strategy:
            'Reduce complexity, shorten the function, and simplify expressions.',
          steps: [
            'Split into smaller functions to reduce LOC and cyclomatic complexity.',
            'Extract complex expressions to reduce Halstead volume.',
            'Convert nested logic to early returns and guard clauses.',
            'Consider if parts of the function belong in separate modules.',
          ],
        },
        impact:
          'Higher MI directly predicts lower maintenance cost and defect rate.',
        tags: ['maintainability', 'complexity', 'technical-debt'],
      });
    }
  }
  return findings;
}

export function detectTypeAssertionEscape(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    const esc = entry.typeAssertionEscapes;
    if (!esc) continue;

    const total =
      esc.asAny.length + esc.doubleAssertion.length + esc.nonNull.length;
    if (total === 0) continue;

    const parts: string[] = [];
    if (esc.asAny.length > 0) parts.push(`${esc.asAny.length} \`as any\``);
    if (esc.doubleAssertion.length > 0)
      parts.push(`${esc.doubleAssertion.length} double-assertion`);
    if (esc.nonNull.length > 0)
      parts.push(`${esc.nonNull.length} non-null \`!\``);
    const allLines = [...esc.asAny, ...esc.doubleAssertion, ...esc.nonNull].map(
      l => l.lineStart
    );
    const firstLine = Math.min(...allLines);

    if (!canAddFinding(findings)) break;
    findings.push({
      severity:
        esc.asAny.length + esc.doubleAssertion.length > 3 ? 'high' : 'medium',
      category: 'type-assertion-escape',
      file: entry.file,
      lineStart: firstLine,
      lineEnd: firstLine,
      title: `Type-safety escapes in ${entry.file} (${total})`,
      reason: `Found ${parts.join(', ')}. Each assertion bypasses TypeScript's type checker.`,
      files: [entry.file],
      suggestedFix: {
        strategy:
          'Replace type assertions with proper type guards or narrow types.',
        steps: [
          'Replace `as any` with `unknown` and add runtime type checks.',
          'Replace `as unknown as T` with proper generic constraints.',
          'Replace `!` assertions with explicit null checks.',
        ],
      },
      impact:
        'Type assertions silence the compiler — runtime errors go undetected.',
      tags: ['type-safety', 'assertions', 'code-quality'],
    });
  }

  return findings;
}

export function detectMissingErrorBoundary(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (!entry.unprotectedAsync) continue;

    for (const fn of entry.unprotectedAsync) {
      const severity =
        fn.awaitCount >= 4 ? 'high' : fn.awaitCount >= 2 ? 'medium' : 'low';
      findings.push({
        severity,
        category: 'missing-error-boundary',
        file: entry.file,
        lineStart: fn.lineStart,
        lineEnd: fn.lineEnd,
        title: `Missing error boundary: ${fn.name} (${fn.awaitCount} awaits, no try-catch)`,
        reason: `Async function "${fn.name}" has ${fn.awaitCount} await(s) but no try-catch. Rejected promises propagate as unhandled rejections.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Wrap await calls in try-catch or add a .catch() handler.',
          steps: [
            'Add a try-catch block around the await expressions.',
            'Handle errors appropriately (log, return default, re-throw with context).',
            'If the caller handles errors, document it with a comment.',
          ],
        },
        impact:
          'Unhandled promise rejections crash Node.js processes and cause silent failures in browsers.',
        tags: ['error-handling', 'async', 'reliability'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'callers',
            symbolName: fn.name,
            lineHint: fn.lineStart,
            file: entry.file,
            expectedResult: `check if callers wrap this in try-catch or .catch() — if so, the boundary may exist upstream`,
          },
        ],
      });
    }
  }

  return findings;
}

export function detectPromiseMisuse(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (!entry.asyncWithoutAwait) continue;

    for (const fn of entry.asyncWithoutAwait) {
      findings.push({
        severity: 'medium',
        category: 'promise-misuse',
        file: entry.file,
        lineStart: fn.lineStart,
        lineEnd: fn.lineEnd,
        title: `Unnecessary async: ${fn.name} has no await`,
        reason: `Function "${fn.name}" is declared \`async\` but never uses \`await\`. The \`async\` keyword adds unnecessary Promise wrapping.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Remove the async keyword or add the missing await.',
          steps: [
            'If the function does not need to be async, remove the `async` keyword.',
            'If an `await` was forgotten, add it to the appropriate call.',
            'Verify callers handle the return value correctly after the change.',
          ],
        },
        impact:
          'Unnecessary async wrapping adds microtask overhead and misleads readers.',
        tags: ['async', 'performance', 'clarity'],
      });
    }
  }

  return findings;
}

export function detectAwaitInLoop(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const loc of entry.awaitInLoopLocations || []) {
      findings.push({
        severity: 'high',
        category: 'await-in-loop',
        file: entry.file,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        title: 'await inside loop — sequential async execution',
        reason:
          'Each await runs serially. For N iterations this takes N * latency instead of max(latency). Use Promise.all() or Promise.allSettled() for parallel execution.',
        files: [entry.file],
        suggestedFix: {
          strategy:
            'Collect promises and await them in parallel with Promise.all().',
          steps: [
            'Collect all async operations into an array of promises.',
            'Use await Promise.all(promises) or Promise.allSettled(promises).',
            'If order matters or rate limiting is needed, use a batching utility.',
          ],
        },
        impact:
          'Sequential awaits multiply latency by N iterations — parallelizing can reduce total time to max(single-latency).',
        tags: ['performance', 'async', 'n-plus-one'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'definition',
            symbolName: 'await',
            lineHint: loc.lineStart,
            file: entry.file,
            expectedResult: `navigate to the awaited call to check if parallelization is safe`,
          },
        ],
      });
    }
  }
  return findings;
}

export function detectSyncIo(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const call of entry.syncIoCalls || []) {
      findings.push({
        severity: 'medium',
        category: 'sync-io',
        file: entry.file,
        lineStart: call.lineStart,
        lineEnd: call.lineEnd,
        title: `Synchronous I/O: ${call.name}`,
        reason: `${call.name} blocks the event loop. In server or UI code this degrades responsiveness for all concurrent operations.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Replace with async equivalent.',
          steps: [
            `Replace ${call.name} with its async counterpart (e.g. fs.promises.readFile).`,
            'Sync I/O is acceptable in CLI scripts, build tools, or one-time init code.',
          ],
        },
        impact:
          'Synchronous I/O blocks the event loop, stalling all concurrent requests until the operation completes.',
        tags: ['performance', 'blocking', 'io'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'callers',
            symbolName: call.name,
            lineHint: call.lineStart,
            file: entry.file,
            expectedResult: `find callers to assess if this sync I/O is in a hot path`,
          },
        ],
      });
    }
  }
  return findings;
}

export function detectUnclearedTimers(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const timer of entry.timerCalls || []) {
      if (timer.kind === 'setInterval' && !timer.hasCleanup) {
        findings.push({
          severity: 'medium',
          category: 'uncleared-timer',
          file: entry.file,
          lineStart: timer.lineStart,
          lineEnd: timer.lineEnd,
          title: 'setInterval without clearInterval in scope',
          reason:
            'setInterval without cleanup runs indefinitely, causing memory leaks and unexpected behavior after component unmount or scope exit.',
          files: [entry.file],
          suggestedFix: {
            strategy: 'Store the timer ID and call clearInterval in cleanup.',
            steps: [
              'Assign the return value: const id = setInterval(...).',
              'Call clearInterval(id) in cleanup (useEffect return, componentWillUnmount, or scope exit).',
            ],
          },
          impact:
            'Uncleared intervals run indefinitely, leaking memory and CPU cycles after their scope is no longer relevant.',
          tags: ['performance', 'memory-leak', 'timer'],
        });
      }
    }
  }
  return findings;
}

export function detectListenerLeakRisk(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    const regs = entry.listenerRegistrations || [];
    const removals = entry.listenerRemovals || [];
    if (regs.length > 0 && removals.length === 0) {
      findings.push({
        severity: 'medium',
        category: 'listener-leak-risk',
        file: entry.file,
        lineStart: regs[0].lineStart,
        lineEnd: regs[regs.length - 1].lineEnd,
        title: `${regs.length} event listener(s) added without any removal`,
        reason:
          'addEventListener/on without corresponding removeEventListener/off risks memory leaks if the target outlives the subscriber.',
        files: [entry.file],
        suggestedFix: {
          strategy: 'Add corresponding listener removal in cleanup.',
          steps: [
            'Store the handler reference in a variable.',
            'Call removeEventListener/off in cleanup (unmount, dispose, close).',
            'Or use AbortController signal for automatic cleanup.',
          ],
        },
        impact:
          'Listener references prevent garbage collection of the subscriber, causing memory growth proportional to event-target lifetime.',
        tags: ['performance', 'memory-leak', 'events'],
      });
    }
  }
  return findings;
}

export function detectUnboundedCollection(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      if (fn.loops >= 2 && fn.calls >= 5 && fn.maxLoopDepth >= 2) {
        findings.push({
          severity: 'low',
          category: 'unbounded-collection',
          file: entry.file,
          lineStart: fn.lineStart,
          lineEnd: fn.lineEnd,
          title: `Potential unbounded collection growth in ${fn.name}`,
          reason: `Function "${fn.name}" has ${fn.loops} loops nested ${fn.maxLoopDepth} levels deep with ${fn.calls} calls — structural signal for unbounded growth. Validate with tools: read the function body and check for collection mutations (.push, .add, .set) inside loops.`,
          files: [entry.file],
          suggestedFix: {
            strategy: 'Add size limits, pagination, or streaming.',
            steps: [
              'Add a maximum size check before adding to collections.',
              'Use pagination or streaming for large datasets.',
              'Consider using generators for lazy evaluation.',
            ],
          },
          impact:
            'Unbounded collection growth inside nested loops can cause out-of-memory crashes under large input.',
          tags: ['performance', 'memory', 'collection'],
        });
      }
    }
  }
  return findings;
}

export function detectSimilarFunctionBodies(
  flowMap: Map<string, import('../types/index.js').FlowMapEntry[]>,
  similarityThreshold: number = 0.85
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  const allEntries: import('../types/index.js').FlowMapEntry[] = [];
  for (const entries of flowMap.values()) {
    for (const e of entries) {
      if (!isTestFile(e.file)) allEntries.push(e);
    }
  }

  const buckets = new Map<string, import('../types/index.js').FlowMapEntry[]>();
  for (const entry of allEntries) {
    const key = `${entry.kind}|${Math.round(entry.statementCount / 3)}`;
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push(entry);
  }

  for (const [, bucket] of buckets) {
    if (bucket.length < 2 || bucket.length > 50) continue;

    for (let i = 0; i < bucket.length; i++) {
      for (let j = i + 1; j < bucket.length; j++) {
        const a = bucket[i];
        const b = bucket[j];
        if (a.hash === b.hash) continue;
        if (a.file === b.file && a.lineStart === b.lineStart) continue;

        const stmtRatio =
          Math.min(a.statementCount, b.statementCount) /
          Math.max(a.statementCount, b.statementCount);
        if (stmtRatio < 0.8) continue;

        const similarity = computeMetricSimilarity(a, b);
        if (similarity >= similarityThreshold) {
          findings.push({
            severity: similarity >= 0.95 ? 'high' : 'medium',
            category: 'similar-function-body',
            file: a.file,
            lineStart: a.lineStart,
            lineEnd: a.lineEnd,
            title: `Similar function: ${a.name} (${(similarity * 100).toFixed(0)}% similar to ${b.name} in ${b.file})`,
            reason: `"${a.name}" and "${b.name}" have ${(similarity * 100).toFixed(0)}% structural similarity. Near-duplicates diverge over time and should be consolidated.`,
            files: [a.file, b.file],
            suggestedFix: {
              strategy: 'Extract shared logic into a parameterized helper.',
              steps: [
                `Compare ${a.file}:${a.lineStart} with ${b.file}:${b.lineStart}.`,
                'Identify the varying parts and extract them as parameters.',
                'Create a shared function and call it from both locations.',
              ],
            },
            impact:
              'Near-clone functions diverge over time, causing inconsistent behavior and multiplied maintenance cost.',
            tags: ['duplication', 'maintainability', 'near-clone'],
          });
        }
      }
    }
  }

  return findings;
}

function computeMetricSimilarity(
  a: import('../types/index.js').FlowMapEntry,
  b: import('../types/index.js').FlowMapEntry
): number {
  const features = [
    [a.metrics.complexity, b.metrics.complexity],
    [a.metrics.maxBranchDepth, b.metrics.maxBranchDepth],
    [a.metrics.maxLoopDepth, b.metrics.maxLoopDepth],
    [a.metrics.returns, b.metrics.returns],
    [a.metrics.awaits, b.metrics.awaits],
    [a.metrics.calls, b.metrics.calls],
    [a.metrics.loops, b.metrics.loops],
    [a.statementCount, b.statementCount],
  ];

  let totalSimilarity = 0;
  for (const [va, vb] of features) {
    const max = Math.max(va, vb, 1);
    totalSimilarity += 1 - Math.abs(va - vb) / max;
  }
  return totalSimilarity / features.length;
}

export function detectMessageChains(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (!entry.messageChains || entry.messageChains.length === 0) continue;
    const byLine = new Map<number, typeof entry.messageChains[0]>();
    for (const chain of entry.messageChains) {
      const existing = byLine.get(chain.lineStart);
      if (!existing || chain.depth > existing.depth) {
        byLine.set(chain.lineStart, chain);
      }
    }
    for (const chain of byLine.values()) {
      const severity = chain.depth >= 6 ? 'high' : 'medium';
      findings.push({
        severity,
        category: 'message-chain',
        file: entry.file,
        lineStart: chain.lineStart,
        lineEnd: chain.lineEnd,
        title: `Message chain of depth ${chain.depth}: ${chain.chain.slice(0, 50)}`,
        reason: `A property-access chain of ${chain.depth} steps violates the Law of Demeter — the caller navigates through ${chain.depth - 1} intermediate objects to reach its target. Deep chains tightly couple the caller to internal object structure, making refactoring brittle.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Apply the Law of Demeter — talk only to immediate friends.',
          steps: [
            'Identify the root object and the final method/property being used.',
            'Add a delegating method to the root object (Tell, Don\'t Ask).',
            'Replace the chain with a single call on the immediate object.',
            'If the chain crosses module boundaries, consider whether the intermediate objects should be passed directly.',
          ],
        },
        impact:
          'Deep property chains tightly couple code to internal object structure. When intermediate objects change, every chain accessing them must be updated.',
        tags: ['coupling', 'law-of-demeter', 'maintainability'],
        lspHints: [
          {
            tool: 'lspGetSemantics', semanticType: 'definition',
            symbolName: chain.chain.split('.')[0],
            lineHint: chain.lineStart,
            file: entry.file,
            expectedResult: `find the type of the root object to understand what intermediate types the chain traverses`,
          },
        ],
      });
    }
  }
  return findings;
}

export function detectDeepNesting(
  fileSummaries: FileEntry[],
  threshold: number
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      const maxDepth = Math.max(fn.maxBranchDepth, fn.maxLoopDepth);
      if (maxDepth < threshold) continue;
      if (!canAddFinding(findings)) return findings;
      const severity: Finding['severity'] = maxDepth >= threshold + 3 ? 'high' : maxDepth >= threshold + 1 ? 'medium' : 'low';
      findings.push({
        severity,
        category: 'deep-nesting',
        file: entry.file,
        lineStart: fn.lineStart,
        lineEnd: fn.lineEnd,
        title: `Deep nesting ${maxDepth} levels in ${fn.name || '<anon>'}`,
        reason: `Function has ${maxDepth}-level nesting (branch=${fn.maxBranchDepth}, loop=${fn.maxLoopDepth}), exceeding the ${threshold}-level threshold. Each nesting level multiplies the reader's cognitive load and increases the likelihood of logic errors.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Flatten nesting with guard clauses, early returns, or extraction.',
          steps: [
            'Convert nested if-blocks to guard clauses with early returns.',
            'Extract deeply nested logic into named helper functions.',
            'Replace nested loops with array methods (map/filter/reduce).',
          ],
        },
        impact: 'Deeply nested code is hard to read, test, and modify. Each nesting level compounds the number of control-flow paths.',
        tags: ['nesting', 'readability', 'complexity'],
      });
    }
  }
  return findings;
}

export function detectMultipleReturnPaths(
  fileSummaries: FileEntry[],
  threshold: number
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const fn of entry.functions) {
      if (fn.returns < threshold) continue;
      if (!canAddFinding(findings)) return findings;
      const severity: Finding['severity'] = fn.returns >= threshold + 4 ? 'high' : fn.returns >= threshold + 2 ? 'medium' : 'low';
      findings.push({
        severity,
        category: 'multiple-return-paths',
        file: entry.file,
        lineStart: fn.lineStart,
        lineEnd: fn.lineEnd,
        title: `${fn.returns} return paths in ${fn.name || '<anon>'}`,
        reason: `Function has ${fn.returns} return/throw points — the reader must track every exit path to understand the function's behavior. This exceeds the ${threshold} threshold.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Consolidate return points to reduce exit-path tracking.',
          steps: [
            'Replace scattered returns with a single result variable assigned conditionally.',
            'Use early guard clauses for error cases only.',
            'Consider splitting into smaller functions with clear single-responsibility.',
          ],
        },
        impact: 'Many return paths make it harder to reason about what a function returns and to add post-processing.',
        tags: ['returns', 'readability', 'flow'],
      });
    }
  }
  return findings;
}

export function detectCatchRethrow(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (!entry.catchRethrows || entry.catchRethrows.length === 0) continue;
    for (const loc of entry.catchRethrows) {
      if (!canAddFinding(findings)) return findings;
      findings.push({
        severity: 'low',
        category: 'catch-rethrow',
        file: entry.file,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        title: 'Catch-rethrow without transformation',
        reason: 'A catch block that only re-throws the caught error is a no-op — it adds indentation and obscures the stack trace without adding value.',
        files: [entry.file],
        suggestedFix: {
          strategy: 'Remove the try-catch or add meaningful error handling.',
          steps: [
            'If no transformation is needed, remove the try-catch entirely.',
            'If logging is intended, add a log statement before re-throwing.',
            'If wrapping, throw a new error with the original as cause.',
          ],
        },
        impact: 'Pointless catch blocks add noise and can accidentally swallow stack-trace context.',
        tags: ['error-handling', 'noise', 'cleanup'],
      });
    }
  }
  return findings;
}

export function detectMagicStrings(
  fileSummaries: FileEntry[],
  minOccurrences: number
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  const globalCounts = new Map<string, Array<{ file: string; lineStart: number; lineEnd: number }>>();

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (!entry.magicStrings) continue;
    for (const ms of entry.magicStrings) {
      const list = globalCounts.get(ms.value) || [];
      list.push({ file: ms.file, lineStart: ms.lineStart, lineEnd: ms.lineEnd });
      globalCounts.set(ms.value, list);
    }
  }

  for (const [value, locs] of globalCounts) {
    if (locs.length < minOccurrences) continue;
    if (!canAddFinding(findings)) return findings;
    const uniqueFiles = [...new Set(locs.map(l => l.file))];
    const severity: Finding['severity'] = locs.length >= 8 ? 'high' : locs.length >= 5 ? 'medium' : 'low';
    findings.push({
      severity,
      category: 'magic-string',
      file: locs[0].file,
      lineStart: locs[0].lineStart,
      lineEnd: locs[0].lineEnd,
      title: `Magic string "${value.length > 30 ? value.slice(0, 27) + '...' : value}" appears ${locs.length} times`,
      reason: `The string literal "${value}" is used in ${locs.length} comparisons across ${uniqueFiles.length} file(s). If the value changes, every occurrence must be updated — a classic source of silent bugs.`,
      files: uniqueFiles,
      suggestedFix: {
        strategy: 'Extract to a named constant or enum.',
        steps: [
          `Create a const (e.g. const ${value.toUpperCase().replace(/[^A-Z0-9]/g, '_')} = '${value}').`,
          'Replace all usages with the constant reference.',
          'Consider an enum if there are multiple related string values.',
        ],
      },
      impact: 'Magic strings scatter domain knowledge across the codebase and are invisible to refactoring tools.',
      tags: ['magic-value', 'maintainability', 'duplication'],
    });
  }
  return findings;
}

export function detectBooleanParameterCluster(
  fileSummaries: FileEntry[],
  threshold: number
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (!entry.booleanParamClusters) continue;
    for (const cluster of entry.booleanParamClusters) {
      if (cluster.booleanCount < threshold) continue;
      if (!canAddFinding(findings)) return findings;
      findings.push({
        severity: 'medium',
        category: 'boolean-parameter-cluster',
        file: entry.file,
        lineStart: cluster.lineStart,
        lineEnd: cluster.lineEnd,
        title: `${cluster.booleanCount} boolean params in ${cluster.name || '<anon>'}`,
        reason: `Function has ${cluster.booleanCount} boolean parameters out of ${cluster.totalParams} total. Boolean flags are opaque at call sites (e.g. doThing(true, false, true)) and each flag doubles the function's behavior space.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Replace boolean clusters with an options object or separate functions.',
          steps: [
            'Create an options/config object type with named fields.',
            'Replace boolean parameters with the options object.',
            'Consider splitting into distinct functions for each behavior variant.',
          ],
        },
        impact: 'Boolean parameter clusters make call sites unreadable and the function hard to test — 2^N behavior combinations.',
        tags: ['api-design', 'readability', 'parameters'],
      });
    }
  }
  return findings;
}

export function detectPromiseAllUnhandled(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (!entry.promiseAllUnhandled) continue;
    for (const loc of entry.promiseAllUnhandled) {
      if (!canAddFinding(findings)) return findings;
      findings.push({
        severity: 'medium',
        category: 'promise-all-unhandled',
        file: entry.file,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        title: `${loc.kind} without error handling`,
        reason: `${loc.kind} is called without a surrounding try-catch or .catch() chain. If any of the composed promises reject, the rejection will propagate unhandled.`,
        files: [entry.file],
        suggestedFix: {
          strategy: 'Wrap in try-catch or add .catch() to the promise chain.',
          steps: [
            'Add a try-catch around the await expression.',
            'Or chain a .catch() handler onto the Promise combinator.',
            'Consider Promise.allSettled if partial failure is acceptable.',
          ],
        },
        impact: 'Unhandled rejections from promise combinators crash Node.js processes and cause silent failures in browsers.',
        tags: ['error-handling', 'async', 'reliability'],
      });
    }
  }
  return findings;
}

export function detectExportSurfaceDensity(
  fileSummaries: FileEntry[],
  dependencyState?: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  if (!dependencyState) return findings;

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    const depProfile = entry.dependencyProfile;
    if (!depProfile) continue;
    const totalStatements = entry.functions.reduce((s, f) => s + f.statementCount, 0)
      + entry.flows.length;
    if (totalStatements < 20) continue;
    const exportCount = depProfile.declaredExports?.length || 0;
    if (exportCount === 0) continue;
    const ratio = exportCount / totalStatements;
    if (ratio < 0.5) continue;
    if (!canAddFinding(findings)) return findings;
    const severity: Finding['severity'] = ratio >= 0.8 ? 'high' : ratio >= 0.6 ? 'medium' : 'low';
    findings.push({
      severity,
      category: 'export-surface-density',
      file: entry.file,
      lineStart: 1,
      lineEnd: 1,
      title: `${Math.round(ratio * 100)}% export density (${exportCount} exports / ~${totalStatements} statements)`,
      reason: `This module exports ${exportCount} symbols from ~${totalStatements} total statements — a ${Math.round(ratio * 100)}% export surface. High export density means nearly everything is public API, increasing coupling and reducing the ability to refactor internals.`,
      files: [entry.file],
      suggestedFix: {
        strategy: 'Reduce the public API by making non-essential symbols internal.',
        steps: [
          'Audit each export — does it need to be consumed externally?',
          'Convert unnecessary exports to module-private functions.',
          'Consider splitting into a public facade and private implementation module.',
        ],
      },
      impact: 'High export density couples consumers to internal implementation, making any change a potential breaking change.',
      tags: ['encapsulation', 'api-surface', 'coupling'],
    });
  }
  return findings;
}

export function detectChangeRisk(
  fileSummaries: FileEntry[],
  _flowMap: Map<string, FlowMapEntry[]>,
  _dependencyState?: DependencyState
): FindingDraft[] {
  const findings: FindingDraft[] = [];

  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    let riskScore = 0;
    const signals: string[] = [];

    const avgComplexity = entry.functions.length > 0
      ? entry.functions.reduce((s, f) => s + f.complexity, 0) / entry.functions.length
      : 0;
    if (avgComplexity > 15) {
      riskScore += 2;
      signals.push(`high avg complexity (${avgComplexity.toFixed(1)})`);
    }

    const maxCognitive = entry.functions.reduce((m, f) => Math.max(m, f.cognitiveComplexity), 0);
    if (maxCognitive > 20) {
      riskScore += 2;
      signals.push(`high cognitive complexity (${maxCognitive})`);
    }

    const lowMiCount = entry.functions.filter(f => f.maintainabilityIndex !== undefined && f.maintainabilityIndex < 20).length;
    if (lowMiCount > 0) {
      riskScore += lowMiCount;
      signals.push(`${lowMiCount} function(s) with low MI`);
    }

    if (entry.emptyCatches && entry.emptyCatches.length > 0) {
      riskScore += 1;
      signals.push(`${entry.emptyCatches.length} empty catches`);
    }
    if (entry.promiseAllUnhandled && entry.promiseAllUnhandled.length > 0) {
      riskScore += 1;
      signals.push(`${entry.promiseAllUnhandled.length} unhandled promise combinators`);
    }

    const depProfile = entry.dependencyProfile;
    if (depProfile && depProfile.declaredExports) {
      const exportCount = depProfile.declaredExports.length;
      if (exportCount > 15) {
        riskScore += 1;
        signals.push(`${exportCount} exports`);
      }
    }

    if (riskScore < 4) continue;
    if (!canAddFinding(findings)) return findings;
    const severity: Finding['severity'] = riskScore >= 8 ? 'critical' : riskScore >= 6 ? 'high' : 'medium';
    findings.push({
      severity,
      category: 'change-risk',
      file: entry.file,
      lineStart: 1,
      lineEnd: 1,
      title: `Change-risk score ${riskScore}: ${signals.slice(0, 3).join(', ')}`,
      reason: `This file has a composite change-risk score of ${riskScore}, derived from: ${signals.join('; ')}. Files with multiple overlapping quality signals are the most likely to introduce regressions when modified.`,
      files: [entry.file],
      suggestedFix: {
        strategy: 'Reduce risk incrementally — address the highest-impact signal first.',
        steps: [
          'Check test coverage for this file — add tests if missing.',
          'Address the highest-severity individual finding first.',
          'Consider splitting the module to isolate high-risk logic.',
        ],
      },
      impact: 'Files with multiple quality issues compound risk — each change is likely to trigger regressions in hard-to-predict ways.',
      tags: ['risk', 'composite', 'priority'],
    });
  }
  return findings;
}
