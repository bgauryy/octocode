/** Select complete Markdown sections using the reader's original-line labels. */
export function selectMarkdownSections(outline, totalLines, pattern) {
  if (!Number.isInteger(totalLines) || totalLines < 1) throw new Error('Outline needs original source totalLines');
  const headingPattern = new RegExp(pattern, 'iu');
  const headings = outline.split('\n').flatMap(line => {
    const match = /^\s*(\d+)\|\s+(#{1,6})\s+(.+)$/.exec(line);
    return match ? [{ line: Number(match[1]), level: match[2].length, title: match[3] }] : [];
  });
  for (let i = 0; i < headings.length; i++) {
    if (headings[i].line > totalLines || headings[i].line < 1 || (i && headings[i].line <= headings[i - 1].line)) throw new Error('Outline source labels are not ordered within the file');
  }
  const sections = [];
  for (let index = 0; index < headings.length; index++) {
    const heading = headings[index];
    if (!headingPattern.test(heading.title)) continue;
    const next = headings.slice(index + 1).find(candidate => candidate.level <= heading.level);
    const section = { title: heading.title, startLine: heading.line, endLine: next ? next.line - 1 : totalLines };
    if (!sections.some(parent => parent.startLine <= section.startLine && parent.endLine >= section.endLine)) sections.push(section);
  }
  return sections;
}
