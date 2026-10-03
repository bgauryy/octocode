# Claims, names, example values, and links

Load when text talks about time, roadmap, competitors, performance, names, or other people's content; when a doc needs a sample domain, address, person, phone number, or project name; or when adding or reviewing any link or pointer to other material.

## Timeless wording

- Cut time anchors: `currently`, `now`, `new`, `soon`, `latest`, `eventually`, `presently`, `existing`, `old`, `older`, `newer`, `does not yet`, `in the future`, `as of this writing`, `at present`.
- Readers assume the docs describe the product as it is, so `currently supports` means no more than `supports`.
- IF a change needs a date → THEN name the release: "The January 14, 2021 release adds…".
- Describe what the product does, not how it differs from a previous version.
- Exceptions: procedural and time-stamped content — press releases, blog posts, release notes. `soon` is also fine in a procedure describing a state change: "The VM goes offline `soon` after you send the shutdown command."

## Future features and excessive claims

- Don't document, promise, or hint at unreleased features, prices, or dates. No `coming soon`, `in a future release`, `we plan to`. Pre-announcing anything requires approval from your legal counsel.
- No superlatives or absolutes: `best`, `fastest`, `simplest`, `never`, `always`. Reserve `ensure` and `guarantee` for a promise the system genuinely keeps.
- Performance, cost, and capacity claims need a citable source, or they get cut.
- Security phrasing stays honest: "helps prevent account takeover as part of a broader strategy", not "prevents phishing".
- Don't disparage or benchmark competitors; describe your own mechanism and the scenario where it helps.
- Test a claim against what stays true later, not only what is true today.

## Product names, trademarks, third-party content

- Use the full official name with the owner's capitalization. Don't invent abbreviations and don't shorten an official one — matching a UI label is the only exception, and the text around it must still make clear which product it names. Don't use a product name or feature name as a verb, and don't make one plural or possessive.
- Follow the capitalization a project publishes for its own concepts — in a Kubernetes context, "a Job creates one or more Pods" — which outranks the general caution about case carrying meaning.
- Feature names are lowercase unless the product capitalizes them. "the" goes before tool and API names, not before a product name.
- IF an official name begins lowercase → THEN keep it lowercase even at the start of a sentence, or better, rewrite the sentence.
- Use "service" when referring to several products at once; IF "services" is ambiguous → THEN name the products.
- Trademarks: follow the owner's usage guidelines and use a trademark as a modifier of a noun: "a Chromebook notebook computer", not "a Chromebook". Never as a verb, a plural, or a possessive, and never altered.
- Don't copy third-party docs, blogs, reference works, or open source documentation — licenses vary and attribution isn't permission. The same goes for images, logos, code, and speech. Summarize in your own words and link out.
- Write the definition yourself, then link the source: "a [recovery point objective (RPO)](…)". Don't document another company's product; link to their documentation.

## Example values

Never use real or personally identifiable data in an example. `Alice` and `Bob` belong to cryptographic and protocol specifications — use them only when documenting a specification that uses them, and then stay within that cast; everywhere else, take a name from the following list.

| Kind | Use |
|---|---|
| Domains | `example.com`, `example.org`, `example.net`; documentation domains `altostrat.com`, `examplepetstore.com`, `example-pet-store.com`, `cymbalgroup.com`, `myownpersonaldomain.com` |
| Email | an example domain plus a first name: `dana@example.com`; generic `support@example.net` is fine |
| Person names | Alex, Amal, Ariel, Bola, Charlie, Cruz, Dana, Dani, Hao, Ira, Izumi, Jie, Kai, Kalani, Kim, Kiran, Lee, Lucian, Luka, Mahan, Noam, Nur, Quinn, Raha, Rosario, Sasha, Tal, Taylor, Tristan, Yuri — add an initial for a surname (`Quinn N.`) |
| Companies | "Example Organization", "Enterprise Example Organization" |
| Phone | `800-555-0100` through `800-555-0199` (`references/style-punctuation.md` for format) |
| IPv4 | `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24` |
| IPv6 | `2001:db8::/32` |
| Street address | `1800 Amphibious Blvd., Mountain View, CA 94045`; `8 Rue du Nom Fictif 341, Paris`; `Avenida da Pastelaria 1903, Lisbon` |
| Service account ID | `123456789012345678901` |
| Project names | descriptive, with numbering when needed: `staging`, `frontend-development`, `production-1` |
| Internationalized domains | one of the IDN test TLDs |

- Keep people generic: singular "they" unless gender is the point, and no example that ties a job, a skill, or a behavior to an ethnicity, a gender, or an age.
- Don't put person names, product names, or invented names inside email addresses.
- Vary names, genders, ages, and locations across examples, avoid US-centric defaults, and check that a chosen name doesn't carry a gender connotation that conflicts with the example. Don't assign roles along gender or ethnic lines.
- Avoid `foo`, `bar`, and `baz`; a meaningful placeholder name teaches something. <!-- style-lint: ignore-line metasyntactic-name -->

## Link text and phrasing

- Link the destination's title or a descriptive phrase: "see [Configure a load balancer]", not "[click here]", "[this page]", "[read more]", or a bare URL. Terms of Service and similar legal pages are the rare place a URL can be the link text.
- Put the meaningful words first and keep the text short enough to scan; it must stand alone in a screen reader's link list.
- Include the descriptor for a code element inside the link text: "[the `gcloud instances create` command]". For a series, factor the noun out: "supports the `GET`, `HEAD`, and `OPTIONS` methods".
- Include both the long form and the abbreviation inside the link: "[Google Kubernetes Engine (GKE)]", not "[Google Kubernetes Engine] (GKE)".
- No quotation marks around link text; quotation marks are for an unlinked reference to a section or short work, italics for an unlinked full-length title. Punctuation stays outside the link.
- "For more information, see X." Add the topic when the destination isn't obvious: "For more information about IAM roles, see X." Use "see", not "on" or "at", and keep the pattern identical across the page.
- Say why the link is worth following, either in the link text or in the sentence around it.
- Same-page targets say so: "see the [Write descriptive link text] section of this document."
- IF the target's title matches a title on your page → THEN add context: "see [Install libraries] in "Building new audiences"".
- Explain surprising behavior: a download (name the file type), a different domain, or a new tab — "(opens in a new tab)" if you can't avoid it.

## Link placement and anchors

- Answer a short question in place; a link is not a substitute for the one sentence the reader needs.
- Don't force `target="_blank"` and don't decorate external links with icons — name the domain in text instead.
- Avoid duplicate links to one target on a page, unless they point to different sections, sit far apart, or the page has several entry points. Never reuse one phrase as the link text for two different destinations.
- Internal links use site-root-relative URLs, so they survive a move between environments. Don't link outside the documentation set from navigation or a table of contents.
- Link to a specific heading on a long page rather than telling the reader to scroll.
- Give frequently linked headings an explicit target: lowercase, hyphenated, short, descriptive. In HTML, prefer `<section id>` or `<a name>`, and accept `<h2 id>`; in Markdown, append `{: #anchor-name }` to the heading.
- IF you rewrite a heading with an automatically generated anchor → THEN add the old anchor explicitly, or update every inbound link. Don't change an existing custom anchor unless it contains a term you're removing.

Upstream: [Timeless documentation](https://developers.google.com/style/timeless-documentation) · [Future features](https://developers.google.com/style/future) · [Excessive claims](https://developers.google.com/style/excessive-claims) · [Product names](https://developers.google.com/style/product-names) · [Trademarks](https://developers.google.com/style/trademarks) · [Third-party content](https://developers.google.com/style/third-party-content) · [Example domains and names](https://developers.google.com/style/examples) · [Cross-references and linking](https://developers.google.com/style/cross-references) · [Headings as link targets](https://developers.google.com/style/headings-targets). Verify a disputed or missing rule against the live page → `references/style-pass.md`.
