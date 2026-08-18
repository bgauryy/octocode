# Figures and alt text

Load when a page carries a screenshot, diagram, or any other image.

## When to use an image

- Use an image only when it explains something words handle badly. Never carry new information in an image — images aren't translated and aren't readable by everyone.
- Never use an image of text, a code sample, or terminal output. Use real text.
- Introduce most images with a complete sentence: colon when the image follows immediately, period when other material intervenes. IF the image is a screenshot right after the procedural text that describes that UI → THEN skip the introduction.

## Alt text

- Every `img` needs an `alt` attribute. Omitting it makes assistive technology read the filename aloud.
- Informative images: describe what the image conveys in this context, using at most 155 characters. Write a full sentence or a noun phrase, with punctuation so screen readers pause. No "Image of…" prefix, no all caps.
- Decorative images take `alt=""`: purely ornamental art, UI icons, and a screenshot that only repeats what the text already says (for example, a screenshot showing which fields to fill in).
- IF the image carries more than 155 characters of information → THEN put the detail in the body text or a figure description and keep alt text short.
- Use the same alt text for repeated instances of the same image. Introduce a diagram in the text, not in the alt text, and never let a caption substitute for alt text.

## Captions, numbers, descriptions

- Three distinct elements: alt text (short, for assistive technology), caption (optional label), figure description (longer explanation in the text).
- Caption format: `**Figure 1.** Request flow through the proxy.` — complete sentence, end punctuation. Wrap `img` and `figcaption` in `figure`. Don't lowercase-cap "figure" mid-sentence and don't fold the caption into the referring sentence.
- Refer to figures by number, never `the image above`. IF figure numbers aren't available → THEN repeat the figure where the reader needs it.
- Use numbered callouts explained in prose instead of dense annotations inside the graphic. If text must appear in a graphic, keep it short, in sentence case.

## Files and screenshots

- SVG for diagrams, PNG as fallback, MP4 instead of animated GIF; keep the image within the column width; no image maps, no transparent backgrounds; descriptive filenames.
- High-DPI: the 1x file goes in `src`, the 2x file in `srcset`, named `BASENAME_2x.EXTENSION` at exactly double the dimensions — never an upscaled 1x. Don't center images, and don't nest an `img` inside a `p`.
- Screenshots: crop to the relevant UI, stay consistent across operating systems and visual treatments, and cover personal data with solid opaque blocks — not blur. Flatten layered exports so the covered data is really gone.

Upstream: [Figures and other images](https://developers.google.com/style/images). Verify a disputed or missing rule against the live page → `references/style-sources.md`.

Next: tables and notices → `references/style-blocks.md`; UI wording around the screenshot → `references/style-ui.md`.
