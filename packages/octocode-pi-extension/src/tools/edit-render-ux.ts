export interface EditRationaleSource {
  edits?: Array<{ reasoning: string }>;
}

/** Build compact, stable rationale rows for collapsed edit results. */
export function collapsedEditRationales(
  files: EditRationaleSource[]
): string[] {
  const counts = new Map<string, number>();
  for (const file of files) {
    for (const edit of file.edits ?? []) {
      const rationale = edit.reasoning.trim();
      if (!rationale) continue;
      counts.set(rationale, (counts.get(rationale) ?? 0) + 1);
    }
  }
  return [...counts].map(
    ([rationale, count]) =>
      `${count > 1 ? `(${count} edits) ` : ''}${rationale}`
  );
}
