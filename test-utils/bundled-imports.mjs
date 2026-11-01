import { parsers } from 'prettier/plugins/babel';

/** Find literal local dependencies in JavaScript, ignoring comments and fixture strings.
 * @param {string} source
 * @returns {string[]}
 */
export function localScriptImports(source) {
  const dependencies = new Set();
  const pending = [parsers.babel.parse(source)];
  while (pending.length) {
    const node = pending.pop();
    if (!node || typeof node !== 'object') continue;
    const literal =
      node.type === 'CallExpression' && node.callee?.name === 'require'
        ? node.arguments[0]
        : [
              'ImportDeclaration',
              'ImportExpression',
              'ExportNamedDeclaration',
              'ExportAllDeclaration',
            ].includes(node.type)
          ? node.source
          : null;
    if (literal?.type === 'StringLiteral' && /^\.\.?\//.test(literal.value)) {
      dependencies.add(literal.value);
    }
    for (const value of Object.values(node)) {
      if (Array.isArray(value)) pending.push(...value);
      else if (value && typeof value === 'object') pending.push(value);
    }
  }
  return [...dependencies];
}
