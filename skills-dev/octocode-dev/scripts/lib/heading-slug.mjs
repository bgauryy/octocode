// GitHub heading anchors: the rendered heading text, lowercased, with everything but letters, marks, numbers,
// `_`, `-` and spaces removed and spaces turned into `-`.
export function headingSlug(heading) {
  let text = heading
    .replace(/\s+#+\s*$/, '')
    .replace(/!?\[([^\]]*)]\([^)]*\)/g, '$1');
  // Drop every tag until none is left, so no removal can join the pieces of a new one.
  for (let previous; text !== previous;) {
    previous = text;
    text = text.replace(/<[^>]+>/g, '');
  }
  return text
    .replace(/`|\*\*|__/g, '')
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{M}\p{N}\p{Pc} -]/gu, '')
    .replace(/ /g, '-');
}
