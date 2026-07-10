export type MarkdownHeading = {
  line: number;
  level: number;
  text: string;
  children: MarkdownHeading[];
};

type Fence = {
  marker: '`' | '~';
  length: number;
};

const MARKDOWN_PATH_RE = /\.(?:md|markdown|mdx)$/i;

function isMarkdownFilePath(filePath: string): boolean {
  return MARKDOWN_PATH_RE.test(filePath.split(/[?#]/, 1)[0] ?? filePath);
}

function extractMarkdownHeadingOutline(
  content: string,
  filePath: string
): MarkdownHeading[] | null {
  if (!isMarkdownFilePath(filePath)) return null;
  return buildHeadingTree(parseAtxHeadings(content));
}

export function markdownHeadingOutlineToText(
  content: string,
  filePath: string
): string | null {
  const headings = extractMarkdownHeadingOutline(content, filePath);
  if (headings === null || headings.length === 0) return null;

  const lines: string[] = [];
  appendHeadingTextLines(headings, lines);
  return lines.join('\n');
}

export function markdownHeadingOutlineToDocumentSymbols(
  content: string,
  filePath: string
): unknown[] | null {
  const headings = extractMarkdownHeadingOutline(content, filePath);
  if (headings === null) return null;
  return headings.map(toDocumentSymbol);
}

function parseAtxHeadings(content: string): MarkdownHeading[] {
  const headings: MarkdownHeading[] = [];
  const lines = content.split(/\r?\n/);
  let activeFence: Fence | undefined;

  lines.forEach((line, index) => {
    const fence = line.match(/^[ \t]{0,3}(`{3,}|~{3,})/);
    if (fence?.[1]) {
      const marker = fence[1][0] as Fence['marker'];
      const length = fence[1].length;
      if (!activeFence) {
        activeFence = { marker, length };
      } else if (
        activeFence.marker === marker &&
        length >= activeFence.length
      ) {
        activeFence = undefined;
      }
      return;
    }

    if (activeFence) return;

    const match = line.match(/^[ \t]{0,3}(#{1,6})(?:[ \t]+|$)(.*)$/);
    if (!match?.[1]) return;

    const text = (match[2] ?? '').replace(/[ \t]+#+[ \t]*$/, '').trim();
    headings.push({
      line: index + 1,
      level: match[1].length,
      text: text.length > 0 ? text : '(untitled heading)',
      children: [],
    });
  });

  return headings;
}

function buildHeadingTree(headings: MarkdownHeading[]): MarkdownHeading[] {
  const roots: MarkdownHeading[] = [];
  const stack: MarkdownHeading[] = [];

  for (const heading of headings) {
    while (
      stack.length > 0 &&
      stack[stack.length - 1]!.level >= heading.level
    ) {
      stack.pop();
    }

    const parent = stack[stack.length - 1];
    if (parent) {
      parent.children.push(heading);
    } else {
      roots.push(heading);
    }
    stack.push(heading);
  }

  return roots;
}

function appendHeadingTextLines(
  headings: readonly MarkdownHeading[],
  lines: string[]
): void {
  for (const heading of headings) {
    const gutter = String(heading.line).padStart(4, ' ');
    const indent = '  '.repeat(Math.max(0, heading.level - 1));
    lines.push(
      `${gutter}| ${indent}${'#'.repeat(heading.level)} ${heading.text}`
    );
    appendHeadingTextLines(heading.children, lines);
  }
}

function toDocumentSymbol(heading: MarkdownHeading): Record<string, unknown> {
  const name = `${'#'.repeat(heading.level)} ${heading.text}`;
  return {
    name,
    kind: 'markdownHeading',
    range: {
      start: { line: heading.line - 1, character: 0 },
      end: { line: heading.line - 1, character: name.length },
    },
    selectionRange: {
      start: { line: heading.line - 1, character: 0 },
      end: { line: heading.line - 1, character: name.length },
    },
    children: heading.children.map(toDocumentSymbol),
  };
}
