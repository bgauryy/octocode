# Claims, names, example values, and links

Load for time words, roadmap, competitors, performance, names, others' content, example values, or links.

## Timeless wording

- Cut time anchors: `currently`, `now`, `new`, `soon`, `latest`, `eventually`, `presently`, `existing`, `old`, `older`, `newer`, `does not yet`, `in the future`, `as of this writing`, `at present`.
- A change that needs a date names the release: "The January 14, 2021 release adds…".
- Describe what the product does, not how it differs from a previous version.
- Exceptions: press releases, blog posts, release notes; `soon` in a procedure for a state change ("The VM goes offline `soon` after…").

## Future features and excessive claims

- Don't document or hint at unreleased features, prices, or dates (`coming soon`, `in a future release`, `we plan to`). Pre-announcement needs legal approval.
- No superlatives or absolutes (`best`, `fastest`, `simplest`, `never`, `always`). `ensure` and `guarantee` only for a promise the system keeps.
- Performance, cost, and capacity claims need a citable source, or get cut.
- Honest security phrasing: "helps prevent account takeover as part of a broader strategy", not "prevents phishing".
- Don't disparage or benchmark competitors; describe your mechanism and where it helps.

## Product names, trademarks, third-party content

- Full official name, owner's case. Don't invent or shorten abbreviations, except to match a UI label when context still names the product. No product or feature name as a verb, plural, or possessive.
- A project's published case for its concepts wins ("a Job creates one or more Pods" in Kubernetes).
- Feature names lowercase unless the product capitalizes them. "the" before tool and API names, not before a product name.
- An official lowercase name stays lowercase at sentence start; better, rewrite.
- "service" for several products at once; name them if ambiguous.
- Trademarks: follow the owner's guidelines; use as a modifier ("a Chromebook notebook computer"); never a verb, plural, possessive, or altered.
- Don't copy third-party docs, blogs, reference works, open source docs, images, logos, code, or speech; attribution isn't permission. Summarize and link.
- Write the definition yourself, then link the source ("a [recovery point objective (RPO)](…)"). Don't document another company's product; link theirs.

## Example values

No real or personally identifiable data. `Alice` and `Bob` only when documenting a specification that uses them, within its cast; otherwise use this list.

| Kind | Use |
|---|---|
| Domains | `example.com`, `example.org`, `example.net`; documentation domains `altostrat.com`, `examplepetstore.com`, `example-pet-store.com`, `cymbalgroup.com`, `myownpersonaldomain.com` |
| Email | example domain plus a first name: `dana@example.com`; generic `support@example.net` is fine |
| Person names | Alex, Amal, Ariel, Bola, Charlie, Cruz, Dana, Dani, Hao, Ira, Izumi, Jie, Kai, Kalani, Kim, Kiran, Lee, Lucian, Luka, Mahan, Noam, Nur, Quinn, Raha, Rosario, Sasha, Tal, Taylor, Tristan, Yuri; surname as an initial (`Quinn N.`) |
| Companies | "Example Organization", "Enterprise Example Organization" |
| Phone | `800-555-0100` through `800-555-0199` (format: `references/style-punctuation.md`) |
| IPv4 | `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24` |
| IPv6 | `2001:db8::/32` |
| Street address | `1800 Amphibious Blvd., Mountain View, CA 94045`; `8 Rue du Nom Fictif 341, Paris`; `Avenida da Pastelaria 1903, Lisbon` |
| Service account ID | `123456789012345678901` |
| Project names | descriptive, numbered when needed: `staging`, `frontend-development`, `production-1` |
| Internationalized domains | one of the IDN test TLDs |

- Singular "they" unless gender is the point. No example that ties a job, skill, or behavior to ethnicity, gender, or age.
- No person, product, or invented names inside email addresses.
- Vary names, genders, ages, and locations; avoid US-centric defaults; check that a name's gender connotation fits.
- Meaningful placeholder names, not `foo`, `bar`, `baz`. <!-- style-lint: ignore-line metasyntactic-name -->

## Link text and phrasing

- Link the destination title or a descriptive phrase, never "[click here]", "[this page]", "[read more]", or a bare URL (legal pages are the rare exception).
- Meaningful words first; short; it must stand alone in a screen reader's link list.
- Code element descriptor inside the link ("[the `gcloud instances create` command]"). A series factors out the noun ("the `GET`, `HEAD`, and `OPTIONS` methods").
- Long form and abbreviation both inside the link: "[Google Kubernetes Engine (GKE)]".
- No quotes around link text; quotes for an unlinked section or short work, italics for an unlinked full-length title. Punctuation outside the link.
- "For more information, see X."; add the topic when unclear ("For more information about IAM roles, see X."). "see", not "on" or "at"; same pattern across the page.
- Same-page targets: "see the [Write descriptive link text] section of this document."
- Target title matches one on your page: add context ("see [Install libraries] in "Building new audiences"").
- Say why the link is worth following, in the link or around it.
- Flag surprises: a download (file type), another domain, a new tab ("(opens in a new tab)").

## Link placement and anchors

- Answer a short question in place; a link doesn't replace the one needed sentence.
- No forced `target="_blank"`, no external-link icons; name the domain in text.
- Avoid duplicate links to one target unless they point to different sections, sit far apart, or serve several entry points. Never one link phrase for two destinations.
- Internal links use site-root-relative URLs. No links outside the doc set from navigation or a table of contents.
- Link to a specific heading instead of saying "scroll".
- Frequently linked headings get an explicit anchor: lowercase, hyphenated, short. HTML: `<section id>` or `<a name>`, `<h2 id>` accepted; Markdown: `{: #anchor-name }`.
- Rewriting a heading with an automatic anchor: add the old anchor or update every inbound link. Don't change a custom anchor unless it holds a term you're removing.

Source: [Cross-references and linking](https://developers.google.com/style/cross-references).
