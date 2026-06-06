import * as ts from 'typescript';

import { getLineAndCharacter } from '../common/utils.js';

import type { CodeLocation, ConsoleLogEntry, FileEntry, SuspiciousString } from '../types/index.js';

const HIGH_CONFIDENCE_SENSITIVE_LOG_PATTERNS = [
  /password/i,
  /passwd/i,
  /\bsecret\b/i,
  /\btoken\b/i,
  /credential/i,
  /credit.?card/i,
  /\bssn\b/i,
  /social.?security/i,
  /api[_-]?key/i,
  /private[_-]?key/i,
  /access[_-]?key/i,
];

const LOW_CONFIDENCE_SENSITIVE_LOG_PATTERNS = [
  /\bauth\b/i,
  /\bsession\b/i,
];

const NON_SECRET_AUTH_SESSION_CONTEXT =
  /\b(auth|session)\b.{0,40}\b(flow|status|state|start(?:ed)?|success(?:ful|fully)?|fail(?:ed|ure)?|refresh(?:ed)?|renew(?:ed)?|expire(?:d)?|invalid|chang(?:e|ed)|required|created|destroyed)\b/i;
const AUTH_SESSION_VALUE_HINT =
  /\b(id|sid|jwt|bearer|cookie|header|authorization|credential|secret|token|key)\b|[:=]|\{|\}/i;
const NON_SECRET_USAGE_HINT =
  /\busage:\b|\boptions:\b|--[a-z0-9-]+|\bunknown\b.{0,20}\btoken\b|\bpillar names?\b|\bcategory names?\b/i;

const SECRET_CONTEXT_NAME_PATTERN =
  /(password|passwd|secret|token|api[_-]?key|private[_-]?key|access[_-]?key|credential|auth|session|jwt|bearer|ssn)/i;

const CONSOLE_LOG_METHODS = new Set([
  'log', 'debug', 'trace', 'info', 'warn', 'error', 'dir', 'table',
]);

const SECRET_PATTERNS = [
  /password\s*[:=]\s*['"`]/i,
  /api[_-]?key\s*[:=]\s*['"`]/i,
  /secret\s*[:=]\s*['"`]/i,
  /token\s*[:=]\s*['"`]/i,
  /-----BEGIN.*KEY/,
  /private[_-]?key\s*[:=]\s*['"`]/i,
  /auth[_-]?token\s*[:=]\s*['"`]/i,
];

const SQL_KEYWORDS =
  /\b(SELECT|INSERT|UPDATE|DELETE|DROP|ALTER|CREATE|TRUNCATE)\b/i;


const PLACEHOLDER_PATTERN = /^(YOUR_|REPLACE_ME|<[a-z_-]+>|\$\{|{{)/i;

const UUID_PATTERN =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function isInsideRegexLiteral(node: ts.Node): boolean {
  let current: ts.Node | undefined = node.parent;
  while (current) {
    if (ts.isRegularExpressionLiteral(current)) return true;
    if (
      ts.isNewExpression(current) &&
      current.expression.getText(node.getSourceFile()) === 'RegExp'
    )
      return true;
    current = current.parent;
  }
  return false;
}

function isPlaceholderOrUuid(value: string): boolean {
  return PLACEHOLDER_PATTERN.test(value) || UUID_PATTERN.test(value);
}


const METADATA_PROP_NAMES = new Set([
  'suggestedFix',
  'strategy',
  'steps',
  'reason',
  'impact',
  'expectedResult',
  'title',
]);

function isInsideMetadataProperty(node: ts.Node): boolean {
  let current: ts.Node | undefined = node.parent;
  while (current) {
    if (ts.isPropertyAssignment(current) && ts.isIdentifier(current.name)) {
      if (METADATA_PROP_NAMES.has(current.name.text)) return true;
    }
    current = current.parent;
  }
  return false;
}
function computeShannonEntropy(s: string): number {
  const freq = new Map<string, number>();
  for (const ch of s) freq.set(ch, (freq.get(ch) || 0) + 1);
  let entropy = 0;
  for (const count of freq.values()) {
    const p = count / s.length;
    if (p > 0) entropy -= p * Math.log2(p);
  }
  return entropy;
}

function hasSecretLikeIdentifierContext(
  node: ts.Node,
  sourceFile: ts.SourceFile
): boolean {
  const parent = node.parent;
  if (ts.isVariableDeclaration(parent)) {
    if (ts.isIdentifier(parent.name)) {
      return SECRET_CONTEXT_NAME_PATTERN.test(parent.name.text);
    }
    return false;
  }
  if (ts.isPropertyAssignment(parent)) {
    if (ts.isIdentifier(parent.name)) {
      return SECRET_CONTEXT_NAME_PATTERN.test(parent.name.text);
    }
    if (ts.isStringLiteral(parent.name) || ts.isNumericLiteral(parent.name)) {
      return SECRET_CONTEXT_NAME_PATTERN.test(parent.name.text);
    }
  }
  if (ts.isBinaryExpression(parent) && ts.isPropertyAccessExpression(parent.left)) {
    return SECRET_CONTEXT_NAME_PATTERN.test(parent.left.name.getText(sourceFile));
  }
  return false;
}

function hasSensitiveLogArgument(argText: string): boolean {
  if (NON_SECRET_USAGE_HINT.test(argText)) return false;
  if (HIGH_CONFIDENCE_SENSITIVE_LOG_PATTERNS.some(p => p.test(argText))) {
    return true;
  }
  const hasLowConfidenceTerm = LOW_CONFIDENCE_SENSITIVE_LOG_PATTERNS.some(p =>
    p.test(argText)
  );
  if (!hasLowConfidenceTerm) return false;
  if (NON_SECRET_AUTH_SESSION_CONTEXT.test(argText)) return false;
  return AUTH_SESSION_VALUE_HINT.test(argText);
}

export function collectSecurityData(
  sourceFile: ts.SourceFile,
  fileRelative: string,
  fileEntry: FileEntry
): void {
  const evalUsages: CodeLocation[] = [];
  const unsafeHtmlAssignments: CodeLocation[] = [];
  const suspiciousStrings: SuspiciousString[] = [];
  const consoleLogs: ConsoleLogEntry[] = [];
  const regexLiterals: Array<{
    lineStart: number;
    lineEnd: number;
    pattern: string;
  }> = [];

  const visit = (node: ts.Node): void => {
    if (ts.isDebuggerStatement(node)) {
      const loc = getLineAndCharacter(sourceFile, node);
      consoleLogs.push({
        method: 'debugger',
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        hasSensitiveArg: false,
      });
    }

    if (ts.isCallExpression(node)) {
      const expr = node.expression;
      if (ts.isPropertyAccessExpression(expr)) {
        const obj = expr.expression.getText(sourceFile);
        const method = expr.name.getText(sourceFile);
        if (obj === 'console' && CONSOLE_LOG_METHODS.has(method)) {
          const loc = getLineAndCharacter(sourceFile, node);
          const argText = node.arguments.map(a => a.getText(sourceFile)).join(' ');
          const hasSensitiveArg = hasSensitiveLogArgument(argText);
          consoleLogs.push({
            method,
            lineStart: loc.lineStart,
            lineEnd: loc.lineEnd,
            hasSensitiveArg,
            argSnippet: argText.slice(0, 80),
          });
        }
      }
    }

    if (ts.isCallExpression(node)) {
      const text = node.expression.getText(sourceFile);
      if (text === 'eval' || text === 'Function') {
        const loc = getLineAndCharacter(sourceFile, node);
        evalUsages.push({
          file: fileRelative,
          lineStart: loc.lineStart,
          lineEnd: loc.lineEnd,
        });
      }
      if (text === 'new Function') {
        const loc = getLineAndCharacter(sourceFile, node);
        evalUsages.push({
          file: fileRelative,
          lineStart: loc.lineStart,
          lineEnd: loc.lineEnd,
        });
      }
      if (
        (text === 'setTimeout' || text === 'setInterval') &&
        node.arguments.length > 0
      ) {
        const firstArg = node.arguments[0];
        if (
          ts.isStringLiteral(firstArg) ||
          ts.isNoSubstitutionTemplateLiteral(firstArg)
        ) {
          const loc = getLineAndCharacter(sourceFile, node);
          evalUsages.push({
            file: fileRelative,
            lineStart: loc.lineStart,
            lineEnd: loc.lineEnd,
          });
        }
      }
      if (text === 'document.write' || text === 'document.writeln') {
        const loc = getLineAndCharacter(sourceFile, node);
        unsafeHtmlAssignments.push({
          file: fileRelative,
          lineStart: loc.lineStart,
          lineEnd: loc.lineEnd,
        });
      }
    }

    if (
      ts.isNewExpression(node) &&
      node.expression.getText(sourceFile) === 'Function'
    ) {
      const loc = getLineAndCharacter(sourceFile, node);
      evalUsages.push({
        file: fileRelative,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
      });
    }

    if (
      ts.isBinaryExpression(node) &&
      node.operatorToken.kind === ts.SyntaxKind.EqualsToken
    ) {
      if (ts.isPropertyAccessExpression(node.left)) {
        const prop = node.left.name.getText(sourceFile);
        if (prop === 'innerHTML' || prop === 'outerHTML') {
          const loc = getLineAndCharacter(sourceFile, node);
          unsafeHtmlAssignments.push({
            file: fileRelative,
            lineStart: loc.lineStart,
            lineEnd: loc.lineEnd,
          });
        }
      }
    }

    if (
      ts.isJsxAttribute(node) &&
      node.name.getText(sourceFile) === 'dangerouslySetInnerHTML'
    ) {
      const loc = getLineAndCharacter(sourceFile, node);
      unsafeHtmlAssignments.push({
        file: fileRelative,
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
      });
    }

    if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      if (!isInsideMetadataProperty(node) && !isInsideRegexLiteral(node)) {
        const value = node.text;
        if (!isPlaceholderOrUuid(value)) {
          let matchedSecretPattern = false;
          for (const pattern of SECRET_PATTERNS) {
            if (pattern.test(value)) {
              const loc = getLineAndCharacter(sourceFile, node);
              suspiciousStrings.push({
                lineStart: loc.lineStart,
                lineEnd: loc.lineEnd,
                kind: 'hardcoded-secret',
                snippet: value.slice(0, 40),
                context: 'literal',
              });
              matchedSecretPattern = true;
              break;
            }
          }
          if (
            !matchedSecretPattern &&
            value.length >= 20 &&
            computeShannonEntropy(value) > 4.5 &&
            hasSecretLikeIdentifierContext(node, sourceFile)
          ) {
            const loc = getLineAndCharacter(sourceFile, node);
            suspiciousStrings.push({
              lineStart: loc.lineStart,
              lineEnd: loc.lineEnd,
              kind: 'hardcoded-secret',
              context: 'literal',
            });
          }
        }
      }
    }

    if (ts.isRegularExpressionLiteral(node)) {
      const regexText = node.getText(sourceFile);
      for (const pattern of SECRET_PATTERNS) {
        if (pattern.test(regexText)) {
          const loc = getLineAndCharacter(sourceFile, node);
          suspiciousStrings.push({
            lineStart: loc.lineStart,
            lineEnd: loc.lineEnd,
            kind: 'hardcoded-secret',
            snippet: regexText.slice(0, 40),
            context: 'regex-definition',
          });
          break;
        }
      }
    }

    if (ts.isTemplateExpression(node)) {
      if (!isInsideMetadataProperty(node)) {
        const fullText = node.getText(sourceFile);
        if (SQL_KEYWORDS.test(fullText) && node.templateSpans.length > 0) {
          const loc = getLineAndCharacter(sourceFile, node);
          suspiciousStrings.push({
            lineStart: loc.lineStart,
            lineEnd: loc.lineEnd,
            kind: 'sql-injection',
            snippet: fullText.slice(0, 60),
          });
        }
      }
    }

    if (ts.isRegularExpressionLiteral(node)) {
      const pattern = node.text;
      const loc = getLineAndCharacter(sourceFile, node);
      regexLiterals.push({
        lineStart: loc.lineStart,
        lineEnd: loc.lineEnd,
        pattern,
      });
    }

    ts.forEachChild(node, visit);
  };
  ts.forEachChild(sourceFile, visit);

  fileEntry.evalUsages = evalUsages;
  fileEntry.unsafeHtmlAssignments = unsafeHtmlAssignments;
  fileEntry.suspiciousStrings = suspiciousStrings;
  fileEntry.consoleLogs = consoleLogs;
  fileEntry.regexLiterals = regexLiterals;
}
