Load when writing, reviewing or cleaning text a reader must spend time on: docs, comments, reports, commit and PR text, agent instructions. Source: `skills/common`; installed copies are generated.

# Writing tells

Slop is text that cost less effort to produce than it costs the reader to understand ([No AI Slop manifesto](https://www.slopmanifesto.com/)). The tells below are symptoms. Fix the cause, a vague, unsupported or padded claim, by writing the specific claim or deleting the sentence. Swapping a tell for a synonym fixes nothing.

## Judge before flagging

- Single-instance defects, in any text in scope: chat leftovers, notes for the prompter, placeholders, tool residue and phantom references. Each one is a finding.
- Stylistic families (framing, padding, rhythm, inflation, hedges, formatting): judge density in context. One instance is coincidence; several in one passage is the signal. Judge text the change added, or the text a cleanup request names.
- Skip quotations, fixtures, transcripts, test data and documented examples.
- Detectors and readers both misjudge AI text: a stylistic finding is a rewrite candidate, never an accusation.
- Keep what a parser, policy or reader needs, including required disclosure and attribution.

## Families

| Family | Examples | Repair |
|---|---|---|
| Chat leftovers | "I hope this helps", "Great question", "Certainly!", "Let me know if", "As an AI", knowledge-cutoff notes | Delete |
| Reasoning leaks and narration | "Let me fix", "Now I'll", "Actually, wait", "I want to be careful here", `// changed from X to Y` | Delete; keep the result |
| Notes for the prompter | `<!-- Delete before submission -->`, "Replace with real data", "expand once the body exists" | Act on it or delete it |
| Placeholders | `[Your Name]`, `INSERT_X_HERE`, `example.com/source1`, `2025-XX-XX`, "(Add your URL here)" | Fill with the real value or remove; bare `example.com` and template placeholders are fine |
| Tool residue | `utm_source=chatgpt.com`, `utm_source=openai`, `utm_source=copilot.com`, `contentReference[oaicite:N]`, `turn0search`, `[cite: N]` | Strip the parameter or marker; keep a verified link |
| Framing | "It's not X, it's Y", "Not X. Not Y. Just Z.", "Here's the kicker", "The X? A Y.", restating the question, announcing structure ("three things to know") | State the claim directly |
| Rhythm and emphasis | Triads chosen for rhythm, emphasis words (crucial, critically, truly), the same sentence opening repeated, emphatic repetition of a rule | Keep the items the content has; state a rule once |
| Padding | Signposted conclusions ("In summary"), tie-backs, recaps at every level, counting before a list, belaboring an uncontested point, one point restated in new words | Delete the repeat |
| Inflation | Stakes inflation, promotional words (seamless, robust, groundbreaking, transformative), magic adverbs (quietly, fundamentally), "serves as" for "is", ornate nouns (tapestry, landscape, realm) | Plain words and the measured fact |
| Unsupported precision | "40% faster", "fully tested", "production-ready", "secure", "experts agree" with no measurement, check or named source | Cite the measurement or source, or drop the claim |
| Phantom references | Links, papers, RFC numbers, flags, commands or APIs that do not exist | Open each link; run `--help`; check the API at the pinned version |
| Hedges | "based on available information", "does not by itself establish", "should be treated as X rather than Y" with nothing specific behind them | Keep a hedge only when it names the specific unknown |
| Formatting | Several dashed asides per sentence, bold lead-in on every bullet, emoji headings, title-case headings, Markdown where it does not render (docstrings, commit messages, YAML) | Follow the project's style guide |
| Synonym cycling | One referent called dashboard, portal, then analytics hub | Pick one name and keep it |
| Generic text (swap test) | A comment or paragraph that would read the same in another file or project | Write the specific contract, or delete it |
| Compliance assurance (commit, PR, edit summary) | "ensured", "while preserving", "improved clarity, robustness and maintainability", lists of what was not changed | Name the concrete change and its check |

## Make it not recur

Automate only deterministic residue (tool markers, tracking parameters, placeholder patterns) with a search in the docs or lint gate. For stylistic tells, propose a prose-linter rule such as [Vale](https://vale.sh/) to the owner; the owner decides, since a phrase ban turns into a keyword list. The general rule for recurring defects is in [evidence](evidence.md#verify-a-change).

Sources: [No AI Slop manifesto](https://www.slopmanifesto.com/), [Wikipedia: Signs of AI writing](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing), [tropes.fyi](https://tropes.fyi/), [claudisms.ai](https://claudisms.ai/), [Please stop the slop](https://tej.as/blog/please-stop-the-slop), [Ghostty AI policy](https://github.com/ghostty-org/ghostty/blob/main/AI_POLICY.md).
