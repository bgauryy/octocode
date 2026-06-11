import ts from 'typescript';
import { getExtension } from '../utils/fileExtension.js';

/**
 * Skeleton (minify:"symbols") extraction — shared by localGetFileContent and
 * githubGetFileContent.
 *
 * Architecture: pluggable per-language strategies behind ONE interface
 * (`SignatureStrategy`), an extension→strategy registry, and ONE shared
 * gutter renderer. The ts-js family uses the TypeScript compiler API (pure
 * parse, no type-check); other families use lean line heuristics. Phase 2
 * can add tree-sitter (WASM) strategies for long-tail languages by simply
 * registering more strategies — no dispatch rework needed.
 *
 * Output contract (all strategies):
 *   - original 1-based line numbers in the gutter (`NNN| text`)
 *   - bodies dropped, comments dropped (zero pure-comment lines)
 *   - no blank gutter lines
 *   - whole skeleton, never paginated (callers bypass pagination)
 */

/** Concise hint emitted whenever minify:"symbols" extraction succeeds. */
export const SIGNATURES_ONLY_HINT =
  'Signatures only — bodies and comments omitted; the whole skeleton is returned in one response (never paginated). Left gutter shows original line numbers; use startLine/endLine to read a body.';

/** One kept source line, with its ORIGINAL 1-based line number. */
export type KeptLine = {
  lineNumber: number;
  text: string;
};

type CommentStyle = 'c' | 'hash' | 'c-hash' | 'html' | 'sql';

/**
 * A per-language extraction strategy. Returns the kept lines (original line
 * numbers preserved) or null when nothing structural was found.
 */
export interface SignatureStrategy {
  /** Comment syntax used to filter pure-comment lines out of the skeleton. */
  readonly commentStyle: CommentStyle;
  extract(content: string, ext: string): KeptLine[] | null;
}

// ───────────────────────────── shared helpers ─────────────────────────────

// Net `{`/`}` (and `(`/`[`) depth a line contributes, ignoring line comments.
// Best-effort — braces inside strings are not excluded, consistent with the
// tool's stated LOSSY contract.
function netDelta(line: string, open: string, close: string): number {
  const code = line.replace(/\/\/.*$/, '');
  let depth = 0;
  for (const ch of code) {
    if (ch === open) depth++;
    else if (ch === close) depth--;
  }
  return depth;
}
const braceDelta = (line: string): number => netDelta(line, '{', '}');
const roundDelta = (line: string): number =>
  netDelta(line, '(', ')') + netDelta(line, '[', ']');
// Net generic-angle depth, after stripping operators that contain `<`/`>` but
// are not generics (`=>`, `<=`, `>=`, `<<`, `->`). `>>` is intentionally NOT
// stripped — in TS/Go/Rust it almost always closes nested generics.
const angleDelta = (line: string): number => {
  const code = line.replace(/\/\/.*$/, '').replace(/=>|<=|>=|<<|->/g, '');
  return netDelta(code, '<', '>');
};

function toKeptLines(lines: string[], indices: Iterable<number>): KeptLine[] {
  return [...new Set(indices)]
    .filter(index => index >= 0 && index < lines.length)
    .sort((a, b) => a - b)
    .map(index => ({
      lineNumber: index + 1,
      text: lines[index]!.trimEnd(),
    }));
}

/** True when the line is a pure comment for the family's comment syntax. */
function isPureCommentLine(text: string, style: CommentStyle): boolean {
  const trimmed = text.trim();
  if (trimmed === '') return false;
  if (trimmed.startsWith('#!')) return false; // shebang is structural, not a comment
  if ((style === 'hash' || style === 'c-hash') && trimmed.startsWith('#')) {
    return true;
  }
  if (style === 'sql') {
    if (trimmed.startsWith('--')) return true;
    if (trimmed.startsWith('/*')) {
      const close = trimmed.indexOf('*/');
      if (close === -1 || trimmed.slice(close + 2).trim() === '') return true;
    }
    return false;
  }
  if (
    style === 'html' &&
    (trimmed.startsWith('<!--') || trimmed.startsWith('-->'))
  ) {
    return true;
  }
  if (style === 'c' || style === 'c-hash' || style === 'html') {
    if (trimmed.startsWith('//')) return true;
    // Block-comment interior/closer lines (`* foo`, `*/`).
    if (trimmed.startsWith('*')) return true;
    if (trimmed.startsWith('/*')) {
      const close = trimmed.indexOf('*/');
      if (close === -1 || trimmed.slice(close + 2).trim() === '') return true;
    }
  }
  return false;
}

/**
 * Shared gutter renderer: `NNN| text` with the ORIGINAL line numbers.
 * Blank lines and pure-comment lines never reach the output.
 */
function renderSkeleton(kept: KeptLine[], style: CommentStyle): string | null {
  const visible = kept.filter(
    entry => entry.text.trim() !== '' && !isPureCommentLine(entry.text, style)
  );
  if (visible.length === 0) return null;

  const maxLineNumber = visible.reduce(
    (max, entry) => Math.max(max, entry.lineNumber),
    1
  );
  const lineWidth = String(maxLineNumber).length;
  return visible
    .map(
      entry =>
        `${String(entry.lineNumber).padStart(lineWidth, ' ')}| ${entry.text}`
    )
    .join('\n')
    .trimEnd();
}

// ───────────────────────── ts-js family: AST strategy ─────────────────────────

const SCRIPT_KIND: Record<string, ts.ScriptKind> = {
  ts: ts.ScriptKind.TS,
  tsx: ts.ScriptKind.TSX,
  js: ts.ScriptKind.JS,
  jsx: ts.ScriptKind.JSX,
  mjs: ts.ScriptKind.JS,
  cjs: ts.ScriptKind.JS,
};

function isFunctionValue(
  node: ts.Node
): node is ts.ArrowFunction | ts.FunctionExpression {
  return ts.isArrowFunction(node) || ts.isFunctionExpression(node);
}

function unwrapExpression(expr: ts.Expression): ts.Expression {
  let current = expr;
  for (;;) {
    if (
      ts.isAsExpression(current) ||
      ts.isSatisfiesExpression(current) ||
      ts.isNonNullExpression(current) ||
      ts.isParenthesizedExpression(current) ||
      ts.isTypeAssertionExpression(current)
    ) {
      current = current.expression;
    } else if (ts.isAwaitExpression(current)) {
      current = current.expression;
    } else {
      return current;
    }
  }
}

/** Multi-line initializer kinds whose interiors are dropped (head + closer kept). */
function isDroppableLiteral(node: ts.Node): boolean {
  return (
    ts.isObjectLiteralExpression(node) ||
    ts.isArrayLiteralExpression(node) ||
    ts.isTemplateExpression(node) ||
    ts.isNoSubstitutionTemplateLiteral(node) ||
    ts.isTaggedTemplateExpression(node) ||
    ts.isClassExpression(node) ||
    ts.isJsxElement(node) ||
    ts.isJsxFragment(node) ||
    ts.isJsxSelfClosingElement(node)
  );
}

/**
 * AST-based extraction for ts/tsx/js/jsx/mjs/cjs using the TypeScript
 * compiler API — pure parse via ts.createSourceFile, no type-check.
 * Throws are handled by the strategy wrapper (regex fallback).
 */
function extractTsJsAst(content: string, ext: string): number[] {
  const scriptKind = SCRIPT_KIND[ext] ?? ts.ScriptKind.TS;
  const sourceFile = ts.createSourceFile(
    `module.${ext}`,
    content,
    ts.ScriptTarget.Latest,
    /* setParentNodes */ true,
    scriptKind
  );

  const keep = new Set<number>();
  const lineOf = (pos: number): number =>
    sourceFile.getLineAndCharacterOfPosition(pos).line;
  // getStart(sourceFile) skips leading trivia (comments/JSDoc) but includes
  // decorators — comments before declarations are never inside kept ranges.
  const startLineOf = (node: ts.Node): number =>
    lineOf(node.getStart(sourceFile));
  const endLineOf = (node: ts.Node): number =>
    lineOf(Math.max(node.getStart(sourceFile), node.end - 1));
  const keepLines = (from: number, to: number): void => {
    for (let line = from; line <= to; line++) keep.add(line);
  };
  const dropLines = (from: number, to: number): void => {
    for (let line = from; line <= to; line++) keep.delete(line);
  };
  const keepNode = (node: ts.Node): void =>
    keepLines(startLineOf(node), endLineOf(node));
  const spansMultipleLines = (node: ts.Node): boolean =>
    endLineOf(node) > startLineOf(node);

  /** Drop the interiors of function bodies / big literals nested anywhere under `root`. */
  const dropDescendantBodies = (root: ts.Node): void => {
    const visit = (node: ts.Node): void => {
      if (isFunctionValue(node) && node.body && ts.isBlock(node.body)) {
        if (spansMultipleLines(node.body)) {
          dropLines(
            lineOf(node.body.getStart(sourceFile)) + 1,
            endLineOf(node.body)
          );
        }
        return;
      }
      if (isDroppableLiteral(node) && spansMultipleLines(node)) {
        dropLines(startLineOf(node) + 1, endLineOf(node));
        return;
      }
      node.forEachChild(visit);
    };
    visit(root);
  };

  /** Keep a function-like's signature head (params + return type); drop the body. */
  const keepFunctionHead = (node: ts.FunctionLikeDeclaration): void => {
    if (node.body && ts.isBlock(node.body)) {
      keepLines(startLineOf(node), lineOf(node.body.getStart(sourceFile)));
    } else {
      // Overload/ambient signature — no body to drop.
      keepNode(node);
    }
  };

  /** Head + per-member + closing line — interior comments are skipped naturally. */
  const keepWithMembers = (
    node: ts.Node,
    members: ts.NodeArray<ts.Node>
  ): void => {
    keepLines(startLineOf(node), lineOf(members.pos));
    for (const member of members) keepNode(member);
    keep.add(endLineOf(node));
  };

  /**
   * `<anchor> = <init>`-style statements (variable declarations, export
   * assignments, module.exports): keep the declaration head, drop initializer
   * bodies (callback/object/array/template), keep the final closing line.
   */
  const keepValueStatement = (
    anchor: ts.Node,
    rawInit: ts.Expression,
    exported: boolean
  ): void => {
    const init = unwrapExpression(rawInit);

    if (isFunctionValue(init)) {
      if (init.body && ts.isBlock(init.body)) {
        keepLines(startLineOf(anchor), lineOf(init.body.getStart(sourceFile)));
      } else if (!spansMultipleLines(anchor)) {
        keepNode(anchor);
      } else {
        // Multi-line expression-bodied arrow (e.g. JSX): head + closing line.
        keepLines(startLineOf(anchor), startLineOf(init.body));
        keep.add(endLineOf(anchor));
      }
      return;
    }

    if (ts.isCallExpression(init) || ts.isNewExpression(init)) {
      // `const X = $constructor((inst, def) => {...})`: keep the call head and
      // argument lines, drop callback/object bodies, keep the closing line.
      keepNode(anchor);
      dropDescendantBodies(init);
      keep.add(endLineOf(anchor));
      return;
    }

    if (!exported) return; // non-exported plain assignments carry no signature info

    if (spansMultipleLines(anchor) && isDroppableLiteral(init)) {
      keepLines(startLineOf(anchor), startLineOf(init));
      keep.add(endLineOf(anchor));
      return;
    }

    // Small literal / identifier / single-line initializer: keep whole.
    keepNode(anchor);
  };

  const visitClassMember = (member: ts.ClassElement): void => {
    if (
      ts.isMethodDeclaration(member) ||
      ts.isConstructorDeclaration(member) ||
      ts.isGetAccessorDeclaration(member) ||
      ts.isSetAccessorDeclaration(member)
    ) {
      keepFunctionHead(member);
      return;
    }
    if (ts.isPropertyDeclaration(member)) {
      if (member.initializer) {
        keepValueStatement(member, member.initializer, true);
      } else {
        keepNode(member);
      }
      return;
    }
    if (ts.isIndexSignatureDeclaration(member)) {
      keepNode(member);
    }
    // Static blocks / semicolons: skipped.
  };

  const visitClassLike = (node: ts.ClassLikeDeclaration): void => {
    keepLines(startLineOf(node), lineOf(node.members.pos));
    for (const member of node.members) visitClassMember(member);
    keep.add(endLineOf(node));
  };

  const hasExportModifier = (node: ts.Node): boolean => {
    const modifiers = ts.canHaveModifiers(node)
      ? ts.getModifiers(node)
      : undefined;
    return (
      modifiers?.some(
        modifier => modifier.kind === ts.SyntaxKind.ExportKeyword
      ) ?? false
    );
  };

  const visitStatements = (statements: readonly ts.Statement[]): void => {
    for (const statement of statements) {
      if (
        ts.isImportDeclaration(statement) ||
        ts.isImportEqualsDeclaration(statement) ||
        ts.isExportDeclaration(statement)
      ) {
        keepNode(statement);
        continue;
      }
      if (ts.isExportAssignment(statement)) {
        keepValueStatement(statement, statement.expression, true);
        continue;
      }
      if (ts.isInterfaceDeclaration(statement)) {
        keepWithMembers(statement, statement.members);
        continue;
      }
      if (ts.isTypeAliasDeclaration(statement)) {
        if (ts.isTypeLiteralNode(statement.type)) {
          keepWithMembers(statement, statement.type.members);
        } else {
          keepNode(statement);
        }
        continue;
      }
      if (ts.isEnumDeclaration(statement)) {
        keepWithMembers(statement, statement.members);
        continue;
      }
      if (ts.isFunctionDeclaration(statement)) {
        keepFunctionHead(statement);
        continue;
      }
      if (ts.isClassDeclaration(statement)) {
        visitClassLike(statement);
        continue;
      }
      if (ts.isModuleDeclaration(statement)) {
        if (statement.body && ts.isModuleBlock(statement.body)) {
          keepLines(
            startLineOf(statement),
            lineOf(statement.body.statements.pos)
          );
          visitStatements(statement.body.statements);
          keep.add(endLineOf(statement));
        } else {
          keepNode(statement);
        }
        continue;
      }
      if (ts.isVariableStatement(statement)) {
        const exported = hasExportModifier(statement);
        for (const declaration of statement.declarationList.declarations) {
          if (declaration.initializer) {
            keepValueStatement(statement, declaration.initializer, exported);
          } else if (exported) {
            keepNode(statement);
          }
        }
        continue;
      }
      if (ts.isExpressionStatement(statement)) {
        // CommonJS export surface: `module.exports = …` / `exports.x = …`.
        const expression = statement.expression;
        if (
          ts.isBinaryExpression(expression) &&
          expression.operatorToken.kind === ts.SyntaxKind.EqualsToken
        ) {
          const lhs = expression.left.getText(sourceFile);
          if (lhs.startsWith('module.exports') || lhs.startsWith('exports.')) {
            keepValueStatement(statement, expression.right, true);
          }
        }
        continue;
      }
      // Control flow / side-effect statements: not signatures — skipped.
    }
  };

  visitStatements(sourceFile.statements);
  return [...keep];
}

// ───────────────────── ts-js family: regex fallback ─────────────────────

const TS_JS_PATTERNS: RegExp[] = [
  /^\s*(export\s+)?(default\s+)?(async\s+)?function\s*\*?\s*\w+[^{]*/,
  /^\s*(export\s+)?(abstract\s+)?class\s+\w+[^{]*/,
  /^\s*(export\s+)?interface\s+\w+[^{]*\{[^}]*\}/,
  /^\s*(export\s+)?type\s+\w+[^;\n=]*=?[^;\n]*/,
  /^\s*(import|export)\s+[^\n]+/,
  /^\s*(export\s+)?const\s+\w+[^=]*=\s*(\([^)]*\)|[^=>\n]+)\s*=>/,
  /^\s*(export\s+)?enum\s+\w+/,
  /^\s*(public|private|protected|static|abstract|readonly|override)\s+\w+/,
  /^\s*(async\s+)?(?!(?:if|for|while|switch|catch|return|throw|await|new|typeof|delete|void|yield|do|else|case|in|of|super)\b)\w+\s*(<[^>]+>)?\s*\([^)]*\)\s*(:\s*[^\n{]+)?$/,
];

// `const|let|var X = someCall(` — a declaration whose initializer is a call
// expression (optionally behind a /*…*/ pragma comment or `await`).
const CALL_INITIALIZER_RE =
  /^\s*(?:export\s+)?(?:declare\s+)?(?:const|let|var)\s+[^=]*=\s*(?:\/\*.*?\*\/\s*)?(?:await\s+)?(?!async\b|function\b)[\w$.]+\s*(?:<[^>]*>)?\s*\(/;

/**
 * Heuristic ts-js extraction — retained as the fallback for when the
 * TypeScript parser throws. Multi-line heads/blocks are kept via
 * brace/paren/angle depth tracking.
 */
function extractTsJsRegex(content: string): number[] {
  const lines = content.split('\n');
  const kept: number[] = [];
  const keepLine = (lineIndex: number): void => {
    kept.push(lineIndex);
  };

  // Keep every line of an open `{...}` block (import names / interface /
  // type / enum body) starting from a line with positive brace depth.
  const keepBraceBlock = (start: number): number => {
    let depth = braceDelta(lines[start]!);
    let i = start + 1;
    while (i < lines.length && depth > 0) {
      keepLine(i);
      depth += braceDelta(lines[i]!);
      i++;
    }
    return i;
  };

  let i = 0;
  while (i < lines.length) {
    const line = lines[i]!;
    const matched = TS_JS_PATTERNS.some(pattern => pattern.test(line));
    if (!matched) {
      i++;
      continue;
    }

    keepLine(i);

    const opensBlock =
      /\b(interface|enum)\b/.test(line) ||
      (/[:=]\s*\{[^}]*$/.test(line) && roundDelta(line) <= 0);
    const isMultiImport =
      (/^\s*import\b/.test(line) || /^\s*export\s+(type\s+)?\{/.test(line)) &&
      braceDelta(line) > 0;

    if ((opensBlock && braceDelta(line) > 0) || isMultiImport) {
      i = keepBraceBlock(i);
      continue;
    }

    // Multi-line type alias (non-object): keep until the terminator `;`.
    const isTypeAlias = /^\s*(export\s+)?(declare\s+)?type\s+\w/.test(line);
    if (isTypeAlias && !/;\s*$/.test(line)) {
      i++;
      let guard = 0;
      while (i < lines.length && guard < 200) {
        const aliasLine = lines[i]!;
        keepLine(i);
        i++;
        guard++;
        if (/;\s*$/.test(aliasLine)) break;
      }
      continue;
    }

    // Call-expression initializer whose callback/object body opens on this
    // line: keep only the head lines and the final closing line.
    const isCallInitializer = CALL_INITIALIZER_RE.test(line);
    if (isCallInitializer && roundDelta(line) > 0) {
      let round = roundDelta(line);
      let brace = braceDelta(line);
      i++;
      while (i < lines.length && round > 0 && brace <= 0) {
        keepLine(i);
        round += roundDelta(lines[i]!);
        brace += braceDelta(lines[i]!);
        i++;
      }
      while (i < lines.length && (round > 0 || brace > 0)) {
        round += roundDelta(lines[i]!);
        brace += braceDelta(lines[i]!);
        if (round <= 0 && brace <= 0) keepLine(i);
        i++;
      }
      continue;
    }

    // Multi-line function/method signature head: keep generic params, value
    // params, and return type; the body that follows is dropped as usual.
    if (roundDelta(line) > 0 || /<\s*$/.test(line)) {
      let round = roundDelta(line);
      let angle = angleDelta(line);
      i++;
      while (i < lines.length && (round > 0 || angle > 0)) {
        keepLine(i);
        round += roundDelta(lines[i]!);
        angle += angleDelta(lines[i]!);
        i++;
      }
      continue;
    }

    i++;
  }

  return kept;
}

const tsJsStrategy: SignatureStrategy = {
  commentStyle: 'c',
  extract(content, ext) {
    const lines = content.split('\n');
    let indices: number[];
    try {
      indices = extractTsJsAst(content, ext);
    } catch {
      // Parser threw — fall back to the heuristic extractor.
      indices = extractTsJsRegex(content);
    }
    return toKeptLines(lines, indices);
  },
};

// ───────────────────────────── python strategy ─────────────────────────────

const PY_IMPORT = /^(?:import|from)\s+\S/;
const PY_DEF = /^(?:async\s+)?def\s+\w/;
const PY_CLASS = /^class\s+\w/;
const PY_DECORATOR = /^@\w/;
const PY_DUNDER = /^__\w+__\s*=/;

/**
 * Indent-tracking walker: module/class-level defs, classes, decorators and
 * imports are kept; everything inside a function body (including NESTED
 * defs) is excluded.
 */
const pythonStrategy: SignatureStrategy = {
  commentStyle: 'hash',
  extract(content) {
    const lines = content.split('\n');
    const kept: number[] = [];
    let functionBodyIndent: number | null = null;

    for (let i = 0; i < lines.length; i++) {
      const raw = lines[i]!;
      const trimmed = raw.trim();
      if (trimmed === '') continue;

      const indent = raw.length - raw.trimStart().length;
      if (functionBodyIndent !== null) {
        if (indent > functionBodyIndent) continue; // inside a function body
        functionBodyIndent = null;
      }

      if (
        PY_IMPORT.test(trimmed) ||
        PY_DUNDER.test(trimmed) ||
        PY_DECORATOR.test(trimmed) ||
        PY_CLASS.test(trimmed)
      ) {
        kept.push(i);
        continue;
      }

      if (PY_DEF.test(trimmed)) {
        kept.push(i);
        // Multi-line def head: keep until the parameter list closes.
        let depth = roundDelta(raw);
        while (depth > 0 && i + 1 < lines.length) {
          i++;
          kept.push(i);
          depth += roundDelta(lines[i]!);
        }
        functionBodyIndent = indent;
      }
    }

    return toKeptLines(lines, kept);
  },
};

// ───────────────────────────── go strategy ─────────────────────────────

// gofmt puts all top-level declarations at column 0 — anchoring there
// excludes nested closures and bodies for free.
const GO_TOP = /^(?:package|import|func|type|const|var)\b/;
const GO_PAREN_GROUP = /^(?:import|const|var)\s*\(/;
const GO_BRACE_TYPE = /^type\s+\w+\s+(?:struct|interface)\b/;

const goStrategy: SignatureStrategy = {
  commentStyle: 'c',
  extract(content) {
    const lines = content.split('\n');
    const kept: number[] = [];

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      if (!GO_TOP.test(line)) continue;
      kept.push(i);

      if (GO_PAREN_GROUP.test(line)) {
        // `import (` / `const (` / `var (` group: keep the whole block.
        let depth = roundDelta(line);
        while (depth > 0 && i + 1 < lines.length) {
          i++;
          kept.push(i);
          depth += roundDelta(lines[i]!);
        }
        continue;
      }

      if (GO_BRACE_TYPE.test(line) && braceDelta(line) > 0) {
        // struct/interface body = field/method signatures: keep whole.
        let depth = braceDelta(line);
        while (depth > 0 && i + 1 < lines.length) {
          i++;
          kept.push(i);
          depth += braceDelta(lines[i]!);
        }
        continue;
      }

      // Multi-line func signature head.
      let round = roundDelta(line);
      while (round > 0 && i + 1 < lines.length) {
        i++;
        kept.push(i);
        round += roundDelta(lines[i]!);
      }
    }

    return toKeptLines(lines, kept);
  },
};

// ───────────────────────────── c / c++ strategy ─────────────────────────────

const C_PREPROC = /^\s*#\s*(?:include|define)\b/;
const C_TYPE_BLOCK = /^(?:typedef\s+)?(?:struct|union|enum|class)\b/;
const C_EXTRA = /^(?:namespace\s+\w|template\s*<|extern\s+")/;
const C_CONTROL =
  /^(?:if|else|for|while|switch|return|do|case|goto|sizeof|break|continue)\b/;
// `<type tokens> name(` at column 0 — function definition or prototype.
const C_FUNC = /^[A-Za-z_][\w\s*&:<>,~]*\(/;

const cFamilyStrategy: SignatureStrategy = {
  commentStyle: 'c',
  extract(content) {
    const lines = content.split('\n');
    const kept: number[] = [];

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;

      if (C_PREPROC.test(line) || C_EXTRA.test(line)) {
        kept.push(i);
        continue;
      }

      if (C_TYPE_BLOCK.test(line)) {
        kept.push(i); // always keep the opening head line
        // Enums (plain `enum`, `enum class`, `enum struct`, `typedef enum`):
        // body entries are pure data values, not structural signatures — keep
        // only the head and the closing `}` so agents know the type exists
        // without inheriting a 100+ entry value table verbatim.
        // Structs/unions/classes: keep the whole body (field/method signatures).
        const isEnum = /^(?:typedef\s+)?enum\b/.test(line.trim());
        let depth = braceDelta(line);
        while (depth > 0 && i + 1 < lines.length) {
          i++;
          depth += braceDelta(lines[i]!);
          // struct/union/class: keep every member line
          // enum: keep only the closing brace line (depth just reached 0)
          if (!isEnum || depth <= 0) {
            kept.push(i);
          }
        }
        continue;
      }

      if (C_FUNC.test(line) && !C_CONTROL.test(line)) {
        kept.push(i);
        // Multi-line parameter list.
        let round = roundDelta(line);
        while (round > 0 && i + 1 < lines.length) {
          i++;
          kept.push(i);
          round += roundDelta(lines[i]!);
        }
      }
    }

    return toKeptLines(lines, kept);
  },
};

// ─────────────────────── simple line-pattern strategies ───────────────────────

/**
 * Lean shared runner for line-pattern families: keep matched lines, extending
 * across unbalanced parens so multi-line signature heads stay intact.
 */
function lineStrategy(
  commentStyle: CommentStyle,
  patterns: RegExp[]
): SignatureStrategy {
  return {
    commentStyle,
    extract(content) {
      const lines = content.split('\n');
      const kept: number[] = [];
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i]!;
        if (!patterns.some(pattern => pattern.test(line))) continue;
        kept.push(i);
        let round = roundDelta(line);
        let guard = 0;
        while (round > 0 && i + 1 < lines.length && guard < 100) {
          i++;
          guard++;
          kept.push(i);
          round += roundDelta(lines[i]!);
        }
      }
      return toKeptLines(lines, kept);
    },
  };
}

const javaCsStrategy = lineStrategy('c', [
  /^\s*(public|private|protected|static|abstract|final|override)\s+/,
  /^\s*(class|interface|enum|record)\s+\w+/,
  /^\s*(import|using|package|namespace)\s+/,
]);

const scalaStrategy = lineStrategy('c', [
  /^\s*(package|import)\s+/,
  /^\s*(sealed\s+|abstract\s+|final\s+|case\s+)*(class|object|trait|enum)\s+\w+/,
  /^\s*(override\s+|private\s+|protected\s+|implicit\s+|given\s+)*(def|val|var|type)\s+\w+/,
]);

const rustStrategy = lineStrategy('c', [
  /^\s*(pub(\s*\([^)]+\))?\s+)?(fn|struct|enum|trait|impl|type|const|use|mod)\s+\w+/,
  /^\s*(use|extern)\s+/,
]);

const rubyStrategy = lineStrategy('hash', [
  /^\s*(require|require_relative|include|extend|module_function|alias)\b/,
  /^\s*attr_(reader|writer|accessor)\b/,
  /^\s*(def|class|module)\s+\S/,
]);

const phpStrategy = lineStrategy('c-hash', [
  /^\s*(use|namespace)\s+[\w\\]/,
  /^\s*(abstract\s+|final\s+)*(class|interface|trait|enum)\s+\w+/,
  /^\s*((public|private|protected|static|abstract|final)\s+)*function\s+&?\w+\s*\(/,
  /^\s*((public|private|protected)\s+)?const\s+\w+/,
]);

const swiftStrategy = lineStrategy('c', [
  /^\s*import\s+\w/,
  /^\s*@\w+(\([^)]*\))?\s*$/, // attribute on its own line (e.g. @discardableResult)
  /^\s*((public|private|fileprivate|internal|open|final|static|override|required|convenience|indirect|mutating|class)\s+)*(func|init|class|struct|protocol|enum|extension|subscript|typealias)\b/,
  /^\s*((public|private|fileprivate|internal|open|static|final)\s+)+(var|let)\s+\w/,
]);

// ───────────────────────────── css family strategy ─────────────────────────────

/**
 * css/scss/less: selectors + at-rule heads kept (block-opening lines, plus
 * preceding selector-list lines), `{...}` declaration bodies dropped.
 * Standalone at-rules (`@import`/`@use`, less `@var:`) and top-level scss
 * `$var:` declarations are kept; single-line rules are kept whole so the
 * selector is not lost.
 */
const cssStrategy: SignatureStrategy = {
  commentStyle: 'c',
  extract(content) {
    const lines = content.split('\n');
    const kept: number[] = [];
    let depth = 0;
    let inBlockComment = false;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      const trimmed = line.trim();
      if (inBlockComment) {
        if (trimmed.includes('*/')) inBlockComment = false;
        continue;
      }
      if (trimmed.startsWith('/*') && !trimmed.includes('*/')) {
        inBlockComment = true;
        continue;
      }
      if (trimmed === '') continue;

      const delta = braceDelta(line);
      if (delta > 0) {
        // Selector / at-rule head; pull in preceding selector-list lines.
        let back = i - 1;
        while (back >= 0 && /[,(]\s*$/.test(lines[back]!.trim())) {
          kept.push(back);
          back--;
        }
        kept.push(i);
      } else if (/^@[\w-]/.test(trimmed) && /;\s*$/.test(trimmed)) {
        kept.push(i); // @import / @use / @charset / less `@var: value;`
      } else if (depth === 0 && /^\$[\w-]+\s*:/.test(trimmed)) {
        kept.push(i); // top-level scss variable
      } else if (
        depth === 0 &&
        trimmed.includes('{') &&
        trimmed.includes('}')
      ) {
        kept.push(i); // single-line rule — kept whole
      }
      depth += delta;
    }

    return toKeptLines(lines, kept);
  },
};

// ───────────────────────────── html strategy ─────────────────────────────

const HTML_KEEP: RegExp[] = [
  /^\s*<!doctype\b/i,
  /<script\b[^>]*\bsrc\s*=/i,
  /<link\b[^>]*\bhref\s*=/i,
  /<meta\b[^>]*\bname\s*=/i,
  /<h[1-6][\s>]/i,
  /<[a-z][\w-]*(?:\s[^<>]*)?\bid\s*=/i,
];

const HTML_INLINE_SCRIPT_OPEN =
  /<script\b(?![^>]*\bsrc\s*=)[^>]*>(?!.*<\/script>)/i;
const HTML_INLINE_STYLE_OPEN = /<style\b[^>]*>(?!.*<\/style>)/i;

/**
 * html/htm: doctype, external resources (`script src` / `link href`),
 * named meta tags, headings (with their text), and tags carrying id= are
 * kept; prose text, comments, and inline style/script bodies are dropped.
 */
const htmlStrategy: SignatureStrategy = {
  commentStyle: 'html',
  extract(content) {
    const lines = content.split('\n');
    const kept: number[] = [];
    let skipUntil: RegExp | null = null; // inside an inline <script>/<style>
    let inComment = false;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      if (inComment) {
        if (line.includes('-->')) inComment = false;
        continue;
      }
      if (skipUntil) {
        if (skipUntil.test(line)) skipUntil = null;
        continue;
      }
      const trimmed = line.trim();
      if (trimmed.startsWith('<!--') && !trimmed.includes('-->')) {
        inComment = true;
        continue;
      }
      if (HTML_INLINE_SCRIPT_OPEN.test(line)) {
        skipUntil = /<\/script>/i;
        continue;
      }
      if (HTML_INLINE_STYLE_OPEN.test(line)) {
        skipUntil = /<\/style>/i;
        continue;
      }
      if (HTML_KEEP.some(pattern => pattern.test(line))) kept.push(i);
    }

    return toKeptLines(lines, kept);
  },
};

// ───────────────────────────── vue / svelte strategy ─────────────────────────────

/**
 * vue/svelte SFCs: each `<script>` block is extracted and run through the
 * ts-js strategy with its line numbers OFFSET back to the original file, so
 * the gutter matches the SFC source. In markup, the `<template>` root line
 * and elements carrying id= are kept; prose, comments, and `<style>` bodies
 * are dropped.
 */
const vueSvelteStrategy: SignatureStrategy = {
  commentStyle: 'html',
  extract(content) {
    const lines = content.split('\n');
    const kept: KeptLine[] = [];
    const keepIndex = (index: number): void => {
      kept.push({ lineNumber: index + 1, text: lines[index]!.trimEnd() });
    };
    let inStyle = false;
    let inComment = false;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      const trimmed = line.trim();
      if (inComment) {
        if (trimmed.includes('-->')) inComment = false;
        continue;
      }
      if (inStyle) {
        if (/<\/style>/i.test(line)) inStyle = false;
        continue;
      }
      if (trimmed.startsWith('<!--') && !trimmed.includes('-->')) {
        inComment = true;
        continue;
      }

      const scriptOpen = trimmed.match(/^<script\b([^>]*)>/i);
      if (scriptOpen) {
        keepIndex(i); // the opener carries setup/lang/context info
        if (/\bsrc\s*=/i.test(scriptOpen[1]!)) continue;
        let end = i + 1;
        while (end < lines.length && !/<\/script>/i.test(lines[end]!)) end++;
        const block = lines.slice(i + 1, end).join('\n');
        const lang = /\blang\s*=\s*["']?ts/i.test(scriptOpen[1]!) ? 'ts' : 'js';
        const inner = tsJsStrategy.extract(block, lang);
        if (inner) {
          for (const entry of inner) {
            // Block line 1 is original line i+2 → offset by i+1.
            kept.push({
              lineNumber: entry.lineNumber + i + 1,
              text: entry.text,
            });
          }
        }
        i = end; // `</script>` line itself is not kept
        continue;
      }
      if (/^<style\b/i.test(trimmed) && !/<\/style>/i.test(trimmed)) {
        inStyle = true;
        continue;
      }
      if (/^<template\b/.test(line)) {
        keepIndex(i);
        continue;
      }
      if (/<[a-z][\w-]*(?:\s[^<>]*)?\bid\s*=/i.test(line)) keepIndex(i);
    }

    return kept.length > 0 ? kept : null;
  },
};

// ───────────────────────────── sql strategy ─────────────────────────────

const SQL_CREATE =
  /^\s*CREATE\s+(?:OR\s+REPLACE\s+)?(?:GLOBAL\s+|LOCAL\s+|TEMP(?:ORARY)?\s+|UNLOGGED\s+|UNIQUE\s+|MATERIALIZED\s+|DEFINER\s*=\s*\S+\s+)*(TABLE|VIEW|FUNCTION|PROCEDURE|INDEX|TRIGGER)\b/i;

/**
 * sql: CREATE TABLE/VIEW/FUNCTION/PROCEDURE/INDEX/TRIGGER heads kept (table
 * column lists and function parameter lists included); `$$...$$` and
 * BEGIN...END bodies dropped; `--` line comments and block comments dropped.
 */
const sqlStrategy: SignatureStrategy = {
  commentStyle: 'sql',
  extract(content) {
    const lines = content.split('\n');
    const kept: number[] = [];
    let inBlockComment = false;
    let inDollar = false;
    let beginDepth = 0;

    const dollarToggles = (line: string): number =>
      (line.match(/\$\$/g) ?? []).length;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      const trimmed = line.trim();

      if (inBlockComment) {
        if (trimmed.includes('*/')) inBlockComment = false;
        continue;
      }
      if (inDollar) {
        if (dollarToggles(line) % 2 === 1) inDollar = false;
        continue;
      }
      if (beginDepth > 0) {
        if (/\bBEGIN\b/i.test(trimmed)) beginDepth++;
        if (/\bEND\b/i.test(trimmed)) beginDepth--;
        continue;
      }
      if (trimmed.startsWith('/*') && !trimmed.includes('*/')) {
        inBlockComment = true;
        continue;
      }
      if (trimmed.startsWith('--')) continue;

      const created = trimmed.match(SQL_CREATE);
      if (!created) continue;
      kept.push(i);
      const kind = created[1]!.toUpperCase();

      // Parenthesized column/parameter list (TABLE columns, FUNCTION args).
      let round = roundDelta(line);
      while (round > 0 && i + 1 < lines.length) {
        i++;
        kept.push(i);
        round += roundDelta(lines[i]!);
      }

      if (kind === 'FUNCTION' || kind === 'PROCEDURE' || kind === 'TRIGGER') {
        // Keep RETURNS/qualifier head lines until the body opens or the
        // statement ends; then skip the $$-quoted or BEGIN…END body.
        let guard = 0;
        let headLine = lines[i]!;
        while (guard < 50) {
          if (dollarToggles(headLine) % 2 === 1) {
            inDollar = true;
            break;
          }
          if (/\bBEGIN\b/i.test(headLine)) {
            beginDepth = 1;
            break;
          }
          if (/;\s*$/.test(headLine)) break;
          if (i + 1 >= lines.length) break;
          i++;
          guard++;
          headLine = lines[i]!;
          kept.push(i);
        }
      }
      // TABLE/INDEX/VIEW: head (and any column list) only.
    }

    return toKeptLines(lines, kept);
  },
};

// ───────────────────────────── shell strategy ─────────────────────────────

// `name() {` / `name ()` / `function name` — both POSIX and bash/zsh styles.
const SH_FUNC = /^(function\s+[\w.:-]+|[\w.:-]+\s*\(\s*\))/;
const SH_KEEP = [/^(source|\.)\s+\S/, /^export\s+\w+=/];

// Raw brace counter — shell has no `//` comments, so the c-style stripper in
// netDelta would corrupt counts on URL-bearing lines.
function shellBraceDelta(line: string): number {
  let depth = 0;
  for (const ch of line) {
    if (ch === '{') depth++;
    else if (ch === '}') depth--;
  }
  return depth;
}

/**
 * sh/bash/zsh: shebang, `source`/`.` lines, top-level `export NAME=`, and
 * function definition heads kept; function bodies (brace-tracked) and
 * comments dropped.
 */
const shellStrategy: SignatureStrategy = {
  commentStyle: 'hash',
  extract(content) {
    const lines = content.split('\n');
    const kept: number[] = [];

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      const trimmed = line.trim();
      if (i === 0 && trimmed.startsWith('#!')) {
        kept.push(i);
        continue;
      }
      if (trimmed.startsWith('#')) continue;
      if (SH_KEEP.some(pattern => pattern.test(trimmed))) {
        kept.push(i);
        continue;
      }
      if (!SH_FUNC.test(trimmed)) continue;
      kept.push(i);

      // Skip the function body: find the opening `{` (same or next line),
      // then advance until the braces balance out.
      let depth = shellBraceDelta(line);
      if (depth === 0) {
        let next = i + 1;
        while (next < lines.length && lines[next]!.trim() === '') next++;
        if (next < lines.length && lines[next]!.trim().startsWith('{')) {
          i = next;
          depth = shellBraceDelta(lines[i]!);
        }
      }
      while (depth > 0 && i + 1 < lines.length) {
        i++;
        depth += shellBraceDelta(lines[i]!);
      }
    }

    return toKeptLines(lines, kept);
  },
};

// ───────────────────────────── registry + dispatch ─────────────────────────────

const STRATEGY_REGISTRY: Record<string, SignatureStrategy> = {
  ts: tsJsStrategy,
  tsx: tsJsStrategy,
  js: tsJsStrategy,
  jsx: tsJsStrategy,
  mjs: tsJsStrategy,
  cjs: tsJsStrategy,
  py: pythonStrategy,
  go: goStrategy,
  java: javaCsStrategy,
  cs: javaCsStrategy,
  kt: javaCsStrategy,
  kotlin: javaCsStrategy,
  scala: scalaStrategy,
  rs: rustStrategy,
  rust: rustStrategy,
  c: cFamilyStrategy,
  h: cFamilyStrategy,
  cpp: cFamilyStrategy,
  hpp: cFamilyStrategy,
  cc: cFamilyStrategy,
  rb: rubyStrategy,
  php: phpStrategy,
  swift: swiftStrategy,
  css: cssStrategy,
  scss: cssStrategy,
  less: cssStrategy,
  html: htmlStrategy,
  htm: htmlStrategy,
  vue: vueSvelteStrategy,
  svelte: vueSvelteStrategy,
  sql: sqlStrategy,
  sh: shellStrategy,
  bash: shellStrategy,
  zsh: shellStrategy,
};

export const SUPPORTED_SIGNATURE_EXTENSIONS = Object.freeze(
  Object.keys(STRATEGY_REGISTRY).sort()
);

/**
 * Extract only the structural skeleton of a source file: imports,
 * function/class/interface/type signatures — bodies and comments dropped.
 *
 * ts/tsx/js/jsx/mjs/cjs are parsed with the TypeScript compiler API (regex
 * fallback when the parser throws) — vue/svelte script blocks reuse the same
 * AST path with original-line offsets; py/go/java/cs/kt/kotlin/rs/rust/c/cpp/
 * rb/php/swift/css/scss/less/html/sql/sh use lean per-family heuristics.
 *
 * Returns null when the file extension is not recognised or when extraction
 * produces an empty result (caller should use the original content).
 *
 * Savings: typically 60-95% on code files. LOSSY — agents should follow up
 * with startLine/endLine reads to get specific function bodies.
 */
export function extractSignatures(
  content: string,
  filePath: string
): string | null {
  try {
    const ext = getExtension(filePath, { lowercase: true, fallback: 'txt' });
    const strategy = STRATEGY_REGISTRY[ext];
    if (!strategy) return null;

    const kept = strategy.extract(content, ext);
    if (!kept || kept.length === 0) return null;

    return renderSkeleton(kept, strategy.commentStyle);
  } catch {
    return null;
  }
}
