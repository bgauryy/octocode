# Links and cross-references

Load when adding or reviewing any link or pointer to other material.

## Link text

- Link the destination's title or a descriptive phrase: "see [Configure a load balancer]", not "[click here]", "[this page]", "[read more]", or a bare URL. Terms of Service and similar legal pages are the rare place a URL can be the link text.
- Put the meaningful words first and keep the text short enough to scan; it must stand alone in a screen reader's link list.
- Include the descriptor for a code element inside the link text: "[the `gcloud instances create` command]". For a series, factor the noun out: "supports the `GET`, `HEAD`, and `OPTIONS` methods".
- Include both the long form and the abbreviation inside the link: "[Google Kubernetes Engine (GKE)]", not "[Google Kubernetes Engine] (GKE)".
- No quotation marks around link text; quotation marks are for an unlinked reference to a section or short work, italics for an unlinked full-length title. Punctuation stays outside the link.

## Phrasing

- "For more information, see X." Add the topic when the destination isn't obvious: "For more information about IAM roles, see X."
- Use "see", not "on" or "at", and keep the pattern identical across the page.
- Say why the link is worth following, either in the link text or in the sentence around it.
- Same-page targets say so: "see the [Write descriptive link text] section of this document."
- IF the target's title matches a title on your page → THEN add context: "see [Install libraries] in "Building new audiences"".
- Explain surprising behavior: a download (name the file type), a different domain, or a new tab — "(opens in a new tab)" if you can't avoid it.

## Placement

- Answer a short question in place; a link is not a substitute for the one sentence the reader needs.
- Don't force `target="_blank"` and don't decorate external links with icons — name the domain in text instead.
- Avoid duplicate links to one target on a page, unless they point to different sections, sit far apart, or the page has several entry points. Never reuse one phrase as the link text for two different destinations.
- Internal links use site-root-relative URLs, so they survive a move between environments.
- Don't link outside the documentation set from navigation or a table of contents.
- Link to a specific heading on a long page rather than telling the reader to scroll.

## Anchors

- Give frequently linked headings an explicit target: lowercase, hyphenated, short, descriptive. In HTML, prefer `<section id>` or `<a name>`, and accept `<h2 id>`; in Markdown, append `{: #anchor-name }` to the heading.
- IF you rewrite a heading with an automatically generated anchor → THEN add the old anchor explicitly, or update every inbound link.
- Don't change an existing custom anchor unless it contains a term you're removing.

## Third-party targets

Summarize third-party material in your own words and link out; don't paste their text (`references/style-claims.md`).

Upstream: [Cross-references and linking](https://developers.google.com/style/cross-references) · [Headings as link targets](https://developers.google.com/style/headings-targets). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: heading rules for the target → `references/style-structure.md`; accessible link behavior → `references/style-global.md`.
