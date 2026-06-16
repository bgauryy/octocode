import { isTestFile } from '../common/utils.js';

import type { FileEntry, Finding } from '../types/index.js';

type FindingDraft = Omit<Finding, 'id'>;

const NESTED_QUANTIFIER_RE = /(\(.+[+*]\))[+*]|(\(.+\?\))\{/;

const toSecurityFinding = (
  draft: FindingDraft,
  ruleId: string,
  confidence: 'high' | 'medium' | 'low',
  evidence: Record<string, unknown>
): FindingDraft => ({
  ...draft,
  ruleId,
  confidence,
  evidence,
});

export function detectHardcodedSecrets(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    const secrets = (entry.suspiciousStrings || []).filter(
      s =>
        s.kind === 'hardcoded-secret' &&
        s.context !== 'regex-definition' &&
        s.context !== 'error-message'
    );
    if (secrets.length === 0) continue;
    for (const s of secrets) {
      findings.push(
        toSecurityFinding(
          {
            severity: 'high',
            category: 'hardcoded-secret',
            file: entry.file,
            lineStart: s.lineStart,
            lineEnd: s.lineEnd,
            title: `Potential hardcoded secret${s.snippet ? `: ${s.snippet.slice(0, 20)}…` : ''}`,
            reason: `String literal matches a secret pattern (password, API key, token, high-entropy string). Secrets in source code risk credential leaks. Validate: use localSearchCode to find the variable, then lspGetSemantics(type=references) to check if it is used in auth or network calls.`,
            files: [entry.file],
            suggestedFix: {
              strategy:
                'Move secret to environment variable or secrets manager.',
              steps: [
                'Replace the hardcoded value with process.env.YOUR_SECRET.',
                'Add the variable to your .env file (excluded from git).',
                'Verify the secret is not committed in git history.',
              ],
            },
            impact:
              'Credential leak in source code exposes API access, database credentials, or authentication tokens to anyone with repo access.',
            tags: ['security', 'secrets'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'references',
                symbolName: s.snippet?.split(/[=:]/)[0]?.trim() || 'secret',
                lineHint: s.lineStart,
                file: entry.file,
                expectedResult: `find all usages of this secret value — if used only in tests or as a regex pattern, it is a false positive`,
              },
            ],
          },
          'security.hardcoded-secret',
          'high',
          {
            source: s.snippet || '',
            sink: 'runtime usage',
            context: s.context || 'literal',
            sanitizerStatus: 'missing',
            propagationSteps: [`${entry.file}:${s.lineStart}`],
          }
        )
      );
    }
  }
  return findings;
}

export function detectEvalUsage(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const loc of entry.evalUsages || []) {
      findings.push(
        toSecurityFinding(
          {
            severity: 'critical',
            category: 'eval-usage',
            file: entry.file,
            lineStart: loc.lineStart,
            lineEnd: loc.lineEnd,
            title: 'Dynamic code execution (eval/Function)',
            reason:
              'eval(), new Function(), or string-based setTimeout/setInterval allows arbitrary code execution. This is a code injection vector.',
            files: [entry.file],
            suggestedFix: {
              strategy:
                'Replace dynamic code execution with safe alternatives.',
              steps: [
                'For JSON parsing: use JSON.parse() instead of eval().',
                'For dynamic dispatch: use a lookup table or switch statement.',
                'For setTimeout: pass a function reference, not a string.',
              ],
            },
            impact:
              'Arbitrary code execution enables full application takeover — the most severe class of injection vulnerability.',
            tags: ['security', 'injection', 'critical'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'callers',
                symbolName: 'eval',
                lineHint: loc.lineStart,
                file: entry.file,
                expectedResult: `trace callers to find how user input reaches the eval site`,
              },
            ],
          },
          'security.eval-usage',
          'high',
          {
            sink: `eval at ${entry.file}:${loc.lineStart}-${loc.lineEnd}`,
            sanitizerStatus: 'missing',
          }
        )
      );
    }
  }
  return findings;
}

export function detectUnsafeHtml(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const loc of entry.unsafeHtmlAssignments || []) {
      findings.push(
        toSecurityFinding(
          {
            severity: 'high',
            category: 'unsafe-html',
            file: entry.file,
            lineStart: loc.lineStart,
            lineEnd: loc.lineEnd,
            title: 'Unsafe HTML manipulation',
            reason:
              'innerHTML, outerHTML, dangerouslySetInnerHTML, or document.write can execute unsanitized user input as HTML/script. XSS vector.',
            files: [entry.file],
            suggestedFix: {
              strategy: 'Use safe DOM APIs or sanitize input before insertion.',
              steps: [
                'Replace innerHTML with textContent for plain text.',
                'Use a sanitizer library (e.g. DOMPurify) if HTML is required.',
                'In React, avoid dangerouslySetInnerHTML — use JSX instead.',
              ],
            },
            impact:
              'Unsanitized HTML insertion enables cross-site scripting (XSS) — attackers can steal sessions, credentials, or execute actions as the victim.',
            tags: ['security', 'xss'],
          },
          'security.unsafe-html',
          'high',
          {
            sink: 'DOM assignment',
            sanitizerStatus: 'missing',
            propagationSteps: ['html assignment'],
          }
        )
      );
    }
  }
  return findings;
}

export function detectSqlInjectionRisk(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    const sqls = (entry.suspiciousStrings || []).filter(
      s => s.kind === 'sql-injection'
    );
    for (const s of sqls) {
      findings.push(
        toSecurityFinding(
          {
            severity: 'high',
            category: 'sql-injection-risk',
            file: entry.file,
            lineStart: s.lineStart,
            lineEnd: s.lineEnd,
            title: 'SQL query built with template literal interpolation',
            reason:
              'Template literals with SQL keywords and interpolated expressions risk SQL injection if user input flows into the query.',
            files: [entry.file],
            suggestedFix: {
              strategy: 'Use parameterized queries or a query builder.',
              steps: [
                'Replace template literal with parameterized query (e.g. db.query(sql, [param])).',
                'Use an ORM or query builder that handles escaping.',
                'If raw SQL is necessary, validate and sanitize all interpolated values.',
              ],
            },
            impact:
              'SQL injection can expose, modify, or destroy database contents and potentially escalate to full server compromise.',
            tags: ['security', 'injection', 'sql'],
          },
          'security.sql-injection-risk',
          'high',
          {
            sink: `sql template literal`,
            sanitizerStatus: 'missing',
            propagationSteps: [`${entry.file}:${s.lineStart}-${s.lineEnd}`],
          }
        )
      );
    }
  }
  return findings;
}

export function detectUnsafeRegex(fileSummaries: FileEntry[]): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const re of entry.regexLiterals || []) {
      if (NESTED_QUANTIFIER_RE.test(re.pattern)) {
        findings.push(
          toSecurityFinding(
            {
              severity: 'medium',
              category: 'unsafe-regex',
              file: entry.file,
              lineStart: re.lineStart,
              lineEnd: re.lineEnd,
              title: 'Regex with catastrophic backtracking risk',
              reason: `Pattern "${re.pattern.slice(0, 40)}" has nested quantifiers that can cause exponential backtracking (ReDoS).`,
              files: [entry.file],
              suggestedFix: {
                strategy:
                  'Simplify the regex or use atomic groups / possessive quantifiers.',
                steps: [
                  'Remove nested quantifiers — e.g. change (a+)+ to a+.',
                  'Use a regex linter (e.g. safe-regex) to validate patterns.',
                  'Consider using string methods instead of complex regexes.',
                ],
              },
              impact:
                'Catastrophic backtracking causes CPU exhaustion — a single crafted input string can hang the event loop (ReDoS).',
              tags: ['security', 'regex', 'performance'],
              lspHints: [
                {
                  tool: 'lspGetSemantics', semanticType: 'references',
                  symbolName: re.pattern.slice(0, 20),
                  lineHint: re.lineStart,
                  file: entry.file,
                  expectedResult: `find where this regex is used to assess if user input reaches it`,
                },
              ],
            },
            'security.unsafe-regex',
            'medium',
            {
              source: re.pattern,
              sink: `Regex execution`,
              sanitizerStatus: 'not-applicable',
              propagationSteps: [`${entry.file}:${re.lineStart}-${re.lineEnd}`],
            }
          )
        );
      }
    }
  }
  return findings;
}

export function detectPrototypePollutionRisk(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    if (
      !entry.prototypePollutionSites ||
      entry.prototypePollutionSites.length === 0
    )
      continue;
    for (const site of entry.prototypePollutionSites) {
      let severity: Finding['severity'];
      let confidence: 'high' | 'medium' | 'low';
      if (site.kind === 'computed-property-write') {
        if (site.guarded) {
          severity = 'low';
          confidence = 'low';
        } else {
          severity = 'high';
          confidence = 'medium';
        }
      } else {
        severity = 'medium';
        confidence = 'medium';
      }
      findings.push(
        toSecurityFinding(
          {
            severity,
            category: 'prototype-pollution-risk',
            file: entry.file,
            lineStart: site.lineStart,
            lineEnd: site.lineEnd,
            title: `Prototype pollution risk: ${site.kind}${site.guarded ? ' (guarded)' : ''}`,
            reason: `${site.detail}${site.guarded ? ' — guards detected (internal iteration or key check), likely false positive. Verify the key variable does not trace to external input.' : ''}`,
            files: [entry.file],
            suggestedFix: {
              strategy:
                'Guard against __proto__, constructor, and prototype keys before merging.',
              steps: [
                'Validate keys: reject "__proto__", "constructor", "prototype" before assignment.',
                'Use Object.create(null) as the target for merges when possible.',
                'Replace custom deep-merge with a hardened library (e.g. lodash.merge with prototype guard).',
                'For Object.assign, ensure the source is sanitized or use structuredClone().',
              ],
            },
            impact:
              'Prototype pollution can override built-in methods, bypass security checks, or achieve remote code execution.',
            tags: ['security', 'prototype-pollution', 'injection'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'callers',
                symbolName:
                  site.kind === 'computed-property-write'
                    ? 'bracket-assignment'
                    : site.detail.split('(')[0],
                lineHint: site.lineStart,
                file: entry.file,
                expectedResult: `trace callers to determine if user-controlled data reaches this site — if key comes from Object.keys() on internal object, dismiss as false positive`,
              },
            ],
          },
          'security.prototype-pollution-risk',
          confidence,
          {
            source: site.kind,
            sink: site.detail,
            guarded: site.guarded,
            sanitizerStatus: site.guarded ? 'present' : 'missing',
            propagationSteps: [`${entry.file}:${site.lineStart}`],
          }
        )
      );
    }
  }
  return findings;
}

export function detectUnvalidatedInputSink(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const src of entry.inputSources || []) {
      if (!src.hasSinkInBody || src.hasValidation) continue;
      const sinkLabel = src.sinkKinds.join(', ');
      const severity = src.paramConfidence === 'low' ? 'medium' : 'high';
      findings.push(
        toSecurityFinding(
          {
            severity,
            category: 'unvalidated-input-sink',
            file: entry.file,
            lineStart: src.lineStart,
            lineEnd: src.lineEnd,
            title: `Unvalidated input reaches ${sinkLabel} sink in ${src.functionName}(${src.sourceParams.join(', ')})`,
            reason: `Parameter${src.sourceParams.length > 1 ? 's' : ''} '${src.sourceParams.join("', '")}' (external input) flow${src.sourceParams.length === 1 ? 's' : ''} into ${sinkLabel} without validation (no type guard, schema call, or conditional check).`,
            files: [entry.file],
            suggestedFix: {
              strategy: 'Add input validation before the sink operation.',
              steps: [
                'Add schema validation (e.g. zod, joi) for input parameters.',
                'Use parameterized APIs instead of template interpolation for SQL/exec.',
                `Trace data flow: lspGetSemantics(type=callees) on ${src.functionName}.`,
              ],
            },
            impact:
              'Unvalidated external input reaching a dangerous sink (eval, SQL, exec, innerHTML, file write) enables injection attacks.',
            tags: ['security', 'input-validation', 'injection'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'callers',
                symbolName: src.functionName,
                lineHint: src.lineStart,
                file: entry.file,
                expectedResult: `trace outgoing calls to see where ${src.sourceParams.join(', ')} data flows`,
              },
              {
                tool: 'lspGetSemantics', semanticType: 'references',
                symbolName: src.sourceParams[0],
                lineHint: src.lineStart,
                file: entry.file,
                expectedResult: `check all usages of ${src.sourceParams[0]} parameter within function`,
              },
            ],
          },
          'security.unvalidated-input-sink',
          severity === 'high' ? 'high' : 'medium',
          {
            sourceParameters: src.sourceParams,
            sink: sinkLabel,
            sanitizerStatus: src.hasValidation ? 'present' : 'missing',
            propagationSteps: src.callsWithInputArgs.map(
              call => `${call.callee}:${call.lineStart}`
            ),
          }
        )
      );
    }
  }
  return findings;
}

export function detectInputPassthroughRisk(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const src of entry.inputSources || []) {
      if (src.callsWithInputArgs.length === 0 || src.hasValidation) continue;
      if (src.hasSinkInBody) continue;
      if (src.paramConfidence === 'low') continue;
      const callees = src.callsWithInputArgs.map(c => c.callee);
      const uniqueCallees = [...new Set(callees)];
      const severity = src.paramConfidence === 'high' ? 'medium' : 'low';
      findings.push(
        toSecurityFinding(
          {
            severity,
            category: 'input-passthrough-risk',
            file: entry.file,
            lineStart: src.lineStart,
            lineEnd: src.lineEnd,
            title: `Input passthrough without validation in ${src.functionName}(${src.sourceParams.join(', ')})`,
            reason: `Parameter${src.sourceParams.length > 1 ? 's' : ''} '${src.sourceParams.join("', '")}' (external input) ${src.sourceParams.length === 1 ? 'is' : 'are'} passed to ${uniqueCallees.join(', ')} without validation. Downstream callees may not validate either.`,
            files: [entry.file],
            suggestedFix: {
              strategy:
                'Validate input before passing to downstream functions.',
              steps: [
                'Add schema validation (e.g. zod, joi) at the entry point.',
                `Trace downstream: lspGetSemantics(type=callees) on ${src.functionName} to verify callees validate.`,
                'Search for validation middleware: localSearchCode for guard/validate/sanitize patterns.',
              ],
            },
            impact:
              'Unchecked input passed downstream can reach sinks in callees — validation gaps compound across the call chain.',
            tags: ['security', 'input-validation', 'passthrough'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'callers',
                symbolName: src.functionName,
                lineHint: src.lineStart,
                file: entry.file,
                expectedResult: `trace outgoing calls to verify downstream validation of ${src.sourceParams.join(', ')}`,
              },
              {
                tool: 'lspGetSemantics', semanticType: 'references',
                symbolName: src.sourceParams[0],
                lineHint: src.lineStart,
                file: entry.file,
                expectedResult: `find all usages of ${src.sourceParams[0]} to check if validation occurs upstream`,
              },
            ],
          },
          'security.input-passthrough-risk',
          severity,
          {
            sourceParameters: src.sourceParams,
            sink: uniqueCallees.join(', '),
            sanitizerStatus: src.hasValidation ? 'present' : 'missing',
            propagationSteps: src.callsWithInputArgs.map(
              call => `${call.callee}:${call.lineStart}`
            ),
          }
        )
      );
    }
  }
  return findings;
}

export function detectPathTraversalRisk(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const src of entry.inputSources || []) {
      const fsReadSinks = src.sinkKinds.filter(
        k => k === 'fs-read' || k === 'path-resolve'
      );
      if (fsReadSinks.length === 0) continue;
      if (src.paramConfidence === 'low') continue;

      const hasValidation = src.hasValidation;
      const severity: Finding['severity'] = hasValidation ? 'medium' : 'high';
      const sinkLabel = fsReadSinks.join(', ');

      findings.push(
        toSecurityFinding(
          {
            severity,
            category: 'path-traversal-risk',
            file: entry.file,
            lineStart: src.lineStart,
            lineEnd: src.lineEnd,
            title: `Path traversal risk: ${src.functionName}(${src.sourceParams.join(', ')}) → ${sinkLabel}`,
            reason: `Parameter${src.sourceParams.length > 1 ? 's' : ''} '${src.sourceParams.join("', '")}' (external input) flow${src.sourceParams.length === 1 ? 's' : ''} into ${sinkLabel} ${hasValidation ? 'with partial validation — verify path normalization + prefix check + realpath resolution' : 'without validation. Path traversal (e.g. ../../etc/passwd) can read or write arbitrary files'}.`,
            files: [entry.file],
            suggestedFix: {
              strategy:
                'Add multi-layer path validation before file system operations.',
              steps: [
                'Normalize the path: path.resolve(basePath, userInput).',
                'Prefix check: resolvedPath.startsWith(basePath + path.sep).',
                'Resolve symlinks: fs.realpathSync() to prevent symlink escape.',
                'Re-validate after symlink resolution.',
              ],
            },
            impact:
              'Path traversal enables reading sensitive files (credentials, configs, source code) or writing to arbitrary locations (code injection via file overwrite).',
            tags: ['security', 'path-traversal', 'agentic'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'callers',
                symbolName: src.functionName,
                lineHint: src.lineStart,
                file: entry.file,
                expectedResult: `trace incoming callers to determine if path parameter comes from user input — then trace outgoing to the fs/path call`,
              },
            ],
          },
          'security.path-traversal-risk',
          src.paramConfidence === 'high' ? 'high' : 'medium',
          {
            sourceParameters: src.sourceParams,
            sink: sinkLabel,
            sanitizerStatus: hasValidation ? 'partial' : 'missing',
            propagationSteps: src.callsWithInputArgs.map(
              call => `${call.callee}:${call.lineStart}`
            ),
          }
        )
      );
    }
  }
  return findings;
}

export function detectCommandInjectionRisk(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const src of entry.inputSources || []) {
      const execSinks = src.sinkKinds.filter(k => k === 'exec');
      if (execSinks.length === 0) continue;
      if (src.paramConfidence === 'low') continue;

      const execCallees = src.callsWithInputArgs.filter(c =>
        /\.exec\b|^exec$|^execSync$|child_process\.exec/.test(c.callee)
      );
      const spawnCallees = src.callsWithInputArgs.filter(c =>
        /\.spawn\b|^spawn$|^spawnSync$|child_process\.spawn/.test(c.callee)
      );

      if (execCallees.length > 0) {
        const severity: Finding['severity'] =
          src.paramConfidence === 'high' ? 'critical' : 'high';
        findings.push(
          toSecurityFinding(
            {
              severity,
              category: 'command-injection-risk',
              file: entry.file,
              lineStart: src.lineStart,
              lineEnd: src.lineEnd,
              title: `Command injection risk: ${src.functionName}(${src.sourceParams.join(', ')}) → exec`,
              reason: `Parameter${src.sourceParams.length > 1 ? 's' : ''} '${src.sourceParams.join("', '")}' (external input) flow${src.sourceParams.length === 1 ? 's' : ''} into exec/execSync. exec() runs commands through a shell — string interpolation enables command injection.`,
              files: [entry.file],
              suggestedFix: {
                strategy:
                  'Replace exec with spawn using array arguments (no shell interpretation).',
                steps: [
                  'Replace child_process.exec(cmd) with child_process.spawn(binary, [args]).',
                  'Never interpolate user input into command strings.',
                  'Use an allowlist for permitted commands if dynamic dispatch is needed.',
                  'If shell features are required, validate input against a strict allowlist.',
                ],
              },
              impact:
                'Command injection enables arbitrary OS command execution — full server compromise, data exfiltration, or lateral movement.',
              tags: ['security', 'command-injection', 'critical', 'agentic'],
              lspHints: [
                {
                  tool: 'lspGetSemantics', semanticType: 'callers',
                  symbolName: src.functionName,
                  lineHint: src.lineStart,
                  file: entry.file,
                  expectedResult: `trace incoming callers to verify if user input reaches the exec call — check for allowlist or sanitization`,
                },
              ],
            },
            'security.command-injection-risk',
            src.paramConfidence === 'high' ? 'high' : 'medium',
            {
              sourceParameters: src.sourceParams,
              sink: 'exec',
              sanitizerStatus: src.hasValidation ? 'partial' : 'missing',
              propagationSteps: execCallees.map(
                call => `${call.callee}:${call.lineStart}`
              ),
            }
          )
        );
      }

      if (spawnCallees.length > 0 && execCallees.length === 0) {
        findings.push(
          toSecurityFinding(
            {
              severity: 'high',
              category: 'command-injection-risk',
              file: entry.file,
              lineStart: src.lineStart,
              lineEnd: src.lineEnd,
              title: `Potential command injection: ${src.functionName}(${src.sourceParams.join(', ')}) → spawn`,
              reason: `Parameter${src.sourceParams.length > 1 ? 's' : ''} '${src.sourceParams.join("', '")}' (external input) flow${src.sourceParams.length === 1 ? 's' : ''} into spawn. If shell:true is set, this is equivalent to exec. Verify spawn uses array args without shell option.`,
              files: [entry.file],
              suggestedFix: {
                strategy:
                  'Ensure spawn uses array arguments without shell: true.',
                steps: [
                  'Verify spawn is called as spawn(binary, [arg1, arg2]) — NOT spawn(cmd, { shell: true }).',
                  'Remove shell: true if present.',
                  'Validate command arguments against an allowlist.',
                ],
              },
              impact:
                'spawn with shell:true enables the same command injection as exec. Without shell:true, spawn with array args is safe from injection.',
              tags: ['security', 'command-injection', 'agentic'],
              lspHints: [
                {
                  tool: 'lspGetSemantics', semanticType: 'callers',
                  symbolName: src.functionName,
                  lineHint: src.lineStart,
                  file: entry.file,
                  expectedResult: `trace incoming callers — check if spawn uses shell:true option`,
                },
              ],
            },
            'security.command-injection-risk',
            'medium',
            {
              sourceParameters: src.sourceParams,
              sink: 'spawn',
              sanitizerStatus: src.hasValidation ? 'partial' : 'missing',
              propagationSteps: spawnCallees.map(
                call => `${call.callee}:${call.lineStart}`
              ),
            }
          )
        );
      }
    }
  }
  return findings;
}
export function detectDebugLogLeakage(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const log of entry.consoleLogs || []) {
      if (log.method === 'debugger') {
        findings.push(
          toSecurityFinding(
            {
              severity: 'high',
              category: 'debug-log-leakage',
              file: entry.file,
              lineStart: log.lineStart,
              lineEnd: log.lineEnd,
              title: 'Debugger statement in production code',
              reason:
                'A `debugger` statement pauses execution when DevTools are open. In production it can expose internal state and halt the application.',
              files: [entry.file],
              suggestedFix: {
                strategy: 'Remove the debugger statement before shipping.',
                steps: [
                  'Delete the `debugger;` line.',
                  'Use structured logging (pino, winston) or feature-flagged debug helpers instead.',
                ],
              },
              impact:
                'Debugger statements in production can halt request processing and expose internal runtime state to anyone with browser DevTools open.',
              tags: ['security', 'debug', 'production-safety'],
            },
            'security.debug-log-leakage',
            'high',
            { method: 'debugger', line: log.lineStart }
          )
        );
      } else if (log.method === 'debug' || log.method === 'trace') {
        findings.push(
          toSecurityFinding(
            {
              severity: 'medium',
              category: 'debug-log-leakage',
              file: entry.file,
              lineStart: log.lineStart,
              lineEnd: log.lineEnd,
              title: `console.${log.method}() in production code`,
              reason: `console.${log.method}() is a development-only call. Left in production it leaks internal state, variable values, and execution paths — all useful to attackers.`,
              files: [entry.file],
              suggestedFix: {
                strategy:
                  'Replace with a structured logger that respects log-level configuration.',
                steps: [
                  `Remove or gate the console.${log.method}() call behind a LOG_LEVEL check.`,
                  'Use a structured logger (pino, winston) with level filtering instead.',
                  'Ensure debug/trace levels are disabled in production config.',
                ],
              },
              impact:
                'Debug/trace logs expose internal object state and execution flow, making reconnaissance easier for attackers and violating minimal disclosure.',
              tags: ['security', 'debug', 'information-disclosure'],
              lspHints: [
                {
                  tool: 'lspGetSemantics', semanticType: 'references',
                  symbolName: `console.${log.method}`,
                  lineHint: log.lineStart,
                  file: entry.file,
                  expectedResult: 'find all debug/trace log calls in this file to assess total leakage surface',
                },
              ],
            },
            'security.debug-log-leakage',
            'medium',
            { method: log.method, snippet: log.argSnippet, line: log.lineStart }
          )
        );
      }
    }
  }
  return findings;
}

export function detectSensitiveDataLogging(
  fileSummaries: FileEntry[]
): FindingDraft[] {
  const findings: FindingDraft[] = [];
  for (const entry of fileSummaries) {
    if (isTestFile(entry.file)) continue;
    for (const log of entry.consoleLogs || []) {
      if (log.method === 'debugger' || !log.hasSensitiveArg) continue;
      findings.push(
        toSecurityFinding(
          {
            severity: 'high',
            category: 'sensitive-data-logging',
            file: entry.file,
            lineStart: log.lineStart,
            lineEnd: log.lineEnd,
            title: `Sensitive data logged via console.${log.method}()${log.argSnippet ? `: ${log.argSnippet.slice(0, 40)}` : ''}`,
            reason: `console.${log.method}() argument matches a sensitive-data pattern (password, token, secret, credential, API key, session, SSN). Logging secrets writes them to stdout/stderr, log aggregators, error monitoring services, and persistent log files.`,
            files: [entry.file],
            suggestedFix: {
              strategy:
                'Remove or redact sensitive values before logging.',
              steps: [
                'Never log raw passwords, tokens, API keys, or session identifiers.',
                'If logging for debugging, redact: log({ ...user, password: "[REDACTED]" }).',
                'Use a structured logger with field-level redaction hooks (e.g. pino redact option).',
                'Audit all log aggregation pipelines (Datadog, Splunk, CloudWatch) for secret exposure.',
              ],
            },
            impact:
              'Sensitive data in logs is written to stdout/stderr, forwarded to log aggregators (Splunk, Datadog, CloudWatch), and often stored long-term — creating a persistent credential leak accessible to anyone with log access.',
            tags: ['security', 'sensitive-data', 'credential-leak', 'compliance'],
            lspHints: [
              {
                tool: 'lspGetSemantics', semanticType: 'callers',
                symbolName: log.method,
                lineHint: log.lineStart,
                file: entry.file,
                expectedResult: `trace incoming callers to understand where sensitive data originates before reaching console.${log.method}`,
              },
            ],
          },
          'security.sensitive-data-logging',
          'high',
          {
            method: log.method,
            snippet: log.argSnippet,
            line: log.lineStart,
            sanitizerStatus: 'missing',
          }
        )
      );
    }
  }
  return findings;
}
