// A value as a JavaScript literal for an expression the page evaluates.

const ESCAPES: Record<string, string> = {
  '<': '\\u003C',
  '>': '\\u003E',
  '\b': '\\b',
  '\f': '\\f',
  '\n': '\\n',
  '\r': '\\r',
  '\t': '\\t',
  '\0': '\\0',
  '\u2028': '\\u2028',
  '\u2029': '\\u2029',
};

/**
 * JSON text of `value` that evaluates back to it, with `<`, `>` and the line separators
 * U+2028/U+2029 written as escapes: wherever it is spliced into code, the literal cannot
 * close a script element or end a line.
 */
export const scriptLiteral = (value: unknown): string =>
  JSON.stringify(value).replace(
    /[<>\b\f\n\r\t\0\u2028\u2029]/g,
    char => ESCAPES[char]
  );
