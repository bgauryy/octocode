# Timeless docs and claims

Load when text talks about time, roadmap, competitors, performance, names, or other people's content.

## Timeless wording

- Cut time anchors: `currently`, `now`, `new`, `soon`, `latest`, `eventually`, `presently`, `existing`, `old`, `older`, `newer`, `does not yet`, `in the future`, `as of this writing`, `at present`.
- Readers assume the docs describe the product as it is, so `currently supports` means no more than `supports`.
- IF a change needs a date → THEN name the release: "The January 14, 2021 release adds…".
- Describe what the product does, not how it differs from a previous version.
- Exceptions: procedural and time-stamped content — press releases, blog posts, release notes. `soon` is also fine in a procedure describing a state change: "The VM goes offline `soon` after you send the shutdown command."

## Future features

- Don't document, promise, or hint at unreleased features, prices, or dates. No `coming soon`, `in a future release`, `we plan to`.
- Pre-announcing anything requires approval from your legal counsel.

## Excessive claims

- No superlatives or absolutes: `best`, `fastest`, `simplest`, `never`, `always`. Reserve `ensure` and `guarantee` for a promise the system genuinely keeps.
- Performance, cost, and capacity claims need a citable source, or they get cut.
- Security phrasing stays honest: "helps prevent account takeover as part of a broader strategy", not "prevents phishing".
- Don't disparage or benchmark competitors; describe your own mechanism and the scenario where it helps.
- Test a claim against what stays true later, not only what is true today.

## Product and feature names

- Use the full official name with the owner's capitalization. Don't invent abbreviations and don't shorten an official one — matching a UI label is the only exception, and the text around it must still make clear which product it names. Don't use a product name or feature name as a verb, and don't make one plural or possessive.
- Follow the capitalization a project publishes for its own concepts — in a Kubernetes context, "a Job creates one or more Pods" — which outranks the general caution about case carrying meaning.
- Feature names are lowercase unless the product capitalizes them. "the" goes before tool and API names, not before a product name.
- IF an official name begins lowercase → THEN keep it lowercase even at the start of a sentence, or better, rewrite the sentence.
- Use "service" when referring to several products at once; IF "services" is ambiguous → THEN name the products.

## Trademarks

- Follow the owner's usage guidelines and use a trademark as a modifier of a noun: "a Chromebook notebook computer", not "a Chromebook". Never as a verb, a plural, or a possessive, and never altered.

## Third-party content

- Don't copy third-party docs, blogs, reference works, or open source documentation — licenses vary and attribution isn't permission. The same goes for images, logos, code, and speech.
- Write the definition yourself, then link the source: "a [recovery point objective (RPO)](…)".
- Don't document another company's product; link to their documentation.

Upstream: [Timeless documentation](https://developers.google.com/style/timeless-documentation) · [Future features](https://developers.google.com/style/future) · [Excessive claims](https://developers.google.com/style/excessive-claims) · [Third-party content](https://developers.google.com/style/other-sources) · [Trademarks](https://developers.google.com/style/trademarks) · [Product names](https://developers.google.com/style/product-names). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: word-level swaps for time words → `references/style-words.md`; link patterns → `references/style-links.md`.
