import { dump } from 'js-yaml';

export type YamlConversionConfig = {
  sortKeys?: boolean;
  keysPriority?: string[];
};

function quoteYamlBlockLine(indent: string, line: string): string {
  return line.length === 0 ? indent : `${indent}  ${line}`;
}

// Single-pass decode — sequential replaces would corrupt literal `\n` text
// (dumped as `\\n`) by decoding its tail `\n` into a real newline.
function decodeEscapedYamlString(value: string): string {
  return value.replace(/\\(n|t|"|\\)/g, (_match, ch: string) =>
    ch === 'n' ? '\n' : ch === 't' ? '\t' : ch
  );
}

// Escapes the decoder above cannot reproduce (\r, \u…, \x…) — the quoted
// form is the only lossless representation for values containing them.
const UNSUPPORTED_ESCAPE = /\\(?![nt"\\])/;

function convertMultilineQuotedStringsToBlockScalars(yaml: string): string {
  return yaml.replace(
    /^(\s*)([A-Za-z0-9_-]+): "((?:[^"\\]|\\.)*\\n(?:[^"\\]|\\.)*)"$/gm,
    (match, indent: string, key: string, escapedValue: string) => {
      if (UNSUPPORTED_ESCAPE.test(escapedValue)) return match;
      const content = decodeEscapedYamlString(escapedValue);
      if (!content.includes('\n')) return match; // literal \n text only
      const lines = content
        .split('\n')
        .map(line => quoteYamlBlockLine(indent, line))
        .join('\n');
      return `${indent}${key}: |-\n${lines}`;
    }
  );
}

export function jsonToYamlString(
  jsonObject: unknown,
  config?: YamlConversionConfig
): string {
  const createSortFunction = () => {
    if (!config?.sortKeys && !config?.keysPriority) {
      return false;
    }

    if (config.keysPriority && config.keysPriority.length > 0) {
      const priorityKeys = config.keysPriority;
      return (a: string, b: string) => {
        const aPriority = priorityKeys.indexOf(a);
        const bPriority = priorityKeys.indexOf(b);

        if (aPriority !== -1 && bPriority !== -1) {
          return aPriority - bPriority;
        }

        if (aPriority !== -1 && bPriority === -1) {
          return -1;
        }

        if (aPriority === -1 && bPriority !== -1) {
          return 1;
        }

        if (config.sortKeys) {
          return a.localeCompare(b);
        }

        return 0;
      };
    }

    if (config.sortKeys) {
      return (a: string, b: string) => a.localeCompare(b);
    }

    return false;
  };

  try {
    const yaml = dump(jsonObject, {
      forceQuotes: true,
      quotingType: '"',
      lineWidth: -1,
      noRefs: true,
      sortKeys: createSortFunction(),
      indent: 2,
      noCompatMode: true,
      flowLevel: -1,
      skipInvalid: false,
    });

    return convertMultilineQuotedStringsToBlockScalars(yaml);
  } catch (error) {
    const errorMessage =
      error instanceof Error ? error.message : 'Unknown error';

    try {
      return JSON.stringify(jsonObject, null, 2);
    } catch (jsonError) {
      return `# YAML conversion failed: ${errorMessage}\n# JSON conversion also failed: ${jsonError instanceof Error ? jsonError.message : 'Unknown error'}\n# Object: [Unconvertible]`;
    }
  }
}
