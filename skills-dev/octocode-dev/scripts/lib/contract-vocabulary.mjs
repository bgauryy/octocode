// Contract vocabulary for prose checks: every field name, enum value and parent→child field relation that the
// generated tool contract (input, query and output schemas, continuation channels) and the config contract declare.
// Prose that names `parent.child` where the contract knows `parent` but not that child names a retired or invented field.

const COMBINATORS = new Set(['anyOf', 'oneOf', 'allOf', 'items', 'prefixItems', 'additionalProperties', 'not', 'if', 'then', 'else']);

function createVocabulary() {
  const names = new Set();
  const values = new Set();
  const children = new Map();
  const addChild = (parent, child) => {
    names.add(child);
    if (!parent) return;
    if (!children.has(parent)) children.set(parent, new Set());
    children.get(parent).add(child);
  };
  return { names, values, children, addChild };
}

function resolveRef(ref, root) {
  return ref.replace(/^#\//, '').split('/').reduce((node, key) => node?.[key], root);
}

/** Records each `properties` key under the property name that reached it, following refs and combinators. */
function walkSchema(vocabulary, schema, context) {
  if (Array.isArray(schema)) {
    for (const item of schema) walkSchema(vocabulary, item, context);
    return;
  }
  if (!schema || typeof schema !== 'object') return;
  const { root, globalRoot, parent, seen } = context;
  if (typeof schema.$ref === 'string') {
    const key = `${schema.$ref}|${parent}`;
    if (!seen.has(key)) {
      seen.add(key);
      const target = resolveRef(schema.$ref, root) ?? resolveRef(schema.$ref, globalRoot);
      walkSchema(vocabulary, target, context);
    }
  }
  for (const [key, value] of Object.entries(schema)) walkKeyword(vocabulary, key, value, context);
}

function walkKeyword(vocabulary, key, value, context) {
  if (key === 'properties' && value && typeof value === 'object') {
    for (const [name, child] of Object.entries(value)) {
      vocabulary.addChild(context.parent, name);
      walkSchema(vocabulary, child, { ...context, parent: name });
    }
  } else if (key === 'enum' && Array.isArray(value)) {
    for (const item of value) vocabulary.values.add(String(item));
  } else if (key === 'const') {
    vocabulary.values.add(String(value));
  } else if (COMBINATORS.has(key)) {
    walkSchema(vocabulary, value, context);
  }
}

/** Every string in the contract: descriptions and instructions name paths such as `next.nextCandidatePage` that no schema types. */
function* contractStrings(node) {
  if (typeof node === 'string') yield node;
  else if (Array.isArray(node)) for (const item of node) yield* contractStrings(item);
  else if (node && typeof node === 'object') for (const value of Object.values(node)) yield* contractStrings(value);
}

function addDescribedPaths(vocabulary, contract) {
  for (const text of contractStrings(contract)) {
    for (const [path] of text.matchAll(/\b[A-Za-z_]\w*(?:\[\])?(?:\.[A-Za-z_]\w*(?:\[\])?)+/g)) {
      const segments = path.replaceAll('[]', '').split('.');
      // Prose adds children to parents the schemas declare; it never makes a word such as `test` a parent.
      for (let index = 1; index < segments.length && vocabulary.children.has(segments[index - 1]); index++) {
        vocabulary.addChild(segments[index - 1], segments[index]);
      }
    }
  }
}

function addToolContract(vocabulary, contract) {
  for (const tool of contract.tools ?? []) {
    vocabulary.names.add(tool.name);
    for (const key of ['inputSchema', 'querySchema', 'outputSchema']) {
      if (tool[key]) walkSchema(vocabulary, tool[key], { root: tool[key], globalRoot: contract, parent: null, seen: new Set() });
    }
  }
  // Continuation channels are the `next.*` names a response may emit; collections page their own sections.
  for (const channel of Object.values(contract.continuationChannels ?? {})) {
    for (const name of [...Object.keys(channel.row ?? {}), ...(channel.collections ?? [])]) vocabulary.addChild('next', name);
  }
  // Any tool's error row may carry these (`next.retry` on a rateLimited row).
  for (const name of Object.keys(contract.errorContinuationChannels ?? {})) vocabulary.addChild('next', name);
  for (const key of ['errorCodes', 'partialReasons']) {
    for (const name of Object.keys(contract[key] ?? {})) vocabulary.values.add(name);
  }
  addDescribedPaths(vocabulary, contract);
}

function addConfigContract(vocabulary, config) {
  for (const [section, { fields = {} } = {}] of Object.entries(config?.sections ?? {})) {
    if (!section) continue;
    vocabulary.names.add(section);
    for (const field of Object.keys(fields)) vocabulary.addChild(section, field);
  }
}

/** `{names, values, children}` from the generated tool contract (schemas, channels, described paths) and the config contract. */
export function contractVocabulary(toolContract, configContract) {
  const vocabulary = createVocabulary();
  addToolContract(vocabulary, toolContract);
  addConfigContract(vocabulary, configContract);
  const { names, values, children } = vocabulary;
  return { names, values, children };
}

const DOTTED_PATH = /^[A-Za-z_]\w*(?:\[\])?(?:\.[A-Za-z_]\w*(?:\[\])?)+$/;

/** The first segment of a dotted field path the contract does not declare under its parent, or undefined. */
export function unknownPathSegment(path, vocabulary, extensions = new Set()) {
  const segments = path.replaceAll('[]', '').split('.');
  const last = segments.at(-1);
  // `data.json`, `graph.rs`, `github.com`: a file or host name, not a field path.
  if (extensions.has(last) || /^[a-z\d]{1,3}$/.test(last)) return undefined;
  let parent = segments[0];
  for (const segment of segments.slice(1)) {
    const known = vocabulary.children.get(parent);
    if (!known) return undefined;
    if (!known.has(segment)) return { parent, segment };
    parent = segment;
  }
  return undefined;
}

/** Inline code spans outside fenced blocks, with 1-based line numbers. */
export function inlineCodeSpans(text) {
  const spans = [];
  let fence;
  for (const [index, line] of text.split('\n').entries()) {
    const marker = line.match(/^\s*(`{3,}|~{3,})/)?.[1];
    if (marker && (!fence || marker[0] === fence[0] && marker.length >= fence.length)) {
      fence = fence ? undefined : marker;
      continue;
    }
    if (fence) continue;
    for (const match of line.matchAll(/(`+)([^`]+?)\1/g)) spans.push({ line: index + 1, code: match[2].trim() });
  }
  return spans;
}

/** Dotted field paths in prose that the contract contradicts: a known parent with an undeclared child. */
export function proseFindings(text, vocabulary, extensions) {
  const findings = [];
  for (const { line, code } of inlineCodeSpans(text)) {
    if (!DOTTED_PATH.test(code)) continue;
    const unknown = unknownPathSegment(code, vocabulary, extensions);
    if (unknown) findings.push({ line, token: code, message: `\`${unknown.segment}\` is not a field of \`${unknown.parent}\` in the tool or config contract` });
  }
  return findings;
}

/** String literals of Rust source (plain, byte and raw), with 1-based start lines; comments and char literals skipped. */
export function rustStringLiterals(text) {
  const literals = [];
  let line = 1;
  for (let index = 0; index < text.length; index++) {
    const char = text[index];
    if (char === '\n') line++;
    else if (char === '/' && text[index + 1] === '/') {
      while (index < text.length && text[index] !== '\n') index++;
      line++;
    } else if (char === '/' && text[index + 1] === '*') {
      const end = text.indexOf('*/', index + 2);
      const stop = end < 0 ? text.length : end + 1;
      line += (text.slice(index, stop).match(/\n/g) ?? []).length;
      index = stop;
    } else if (char === "'") {
      // A char literal ('a', '\n', '"') is skipped whole; a lifetime ('a) has no closing quote.
      const close = text[index + 1] === '\\' ? text.indexOf("'", index + 2) : index + 2;
      if (text[close] === "'" && close - index <= 12) index = close;
    } else if (char === 'r' && /^r#*"/.test(text.slice(index, index + 260)) && !/\w/.test(text[index - 1] ?? '')) {
      const hashes = text.slice(index + 1).match(/^#*/)[0];
      const start = index + 2 + hashes.length;
      const end = text.indexOf(`"${hashes}`, start);
      const stop = end < 0 ? text.length : end;
      literals.push({ line, value: text.slice(start, stop) });
      line += (text.slice(index, stop).match(/\n/g) ?? []).length;
      index = stop + hashes.length;
    } else if (char === '"') {
      let end = index + 1;
      while (end < text.length && text[end] !== '"') end += text[end] === '\\' ? 2 : 1;
      literals.push({ line, value: text.slice(index + 1, end) });
      line += (text.slice(index, end).match(/\n/g) ?? []).length;
      index = end;
    }
  }
  return literals;
}

const PATH_IN_TEXT = /(?<![\w./{])[A-Za-z_]\w*(?:\[\])?(?:\.[A-Za-z_]\w*(?:\[\])?)+(?![\w({])/g;

/** Dotted field paths that Rust hint, warning and lead strings name where the contract knows the parent but not the child. */
export function rustFindings(text, vocabulary, extensions) {
  const findings = [];
  for (const { line, value } of rustStringLiterals(text)) {
    for (const [token] of value.matchAll(PATH_IN_TEXT)) {
      const unknown = unknownPathSegment(token, vocabulary, extensions);
      if (unknown) findings.push({ line, token, message: `\`${unknown.segment}\` is not a field of \`${unknown.parent}\` in the tool or config contract` });
    }
  }
  return findings;
}
