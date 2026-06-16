use crate::comment_remover::remove_comments;

// ── CSS ──────────────────────────────────────────────────────────────────────

/// Regex baseline — always available, fast.
pub fn minify_css_core(content: &str) -> String {
    let s = remove_comments(content, &["c-style"]);
    let s = super::collapse_whitespace(&s);
    let s = super::re_tighten_punct(&s);
    s.trim().to_owned()
}

/// High-quality CSS minification via lightningcss (100× better than regex).
/// Uses `minify_css_core` on parse or panic error.
pub fn minify_css_quality(content: &str) -> String {
    std::panic::catch_unwind(|| {
        use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
        match StyleSheet::parse(content, ParserOptions::default()) {
            Ok(ss) => ss
                .to_css(PrinterOptions {
                    minify: true,
                    ..Default::default()
                })
                .map(|out| out.code.to_string())
                .unwrap_or_else(|_| minify_css_core(content)),
            Err(_) => minify_css_core(content),
        }
    })
    .unwrap_or_else(|_| minify_css_core(content))
}

// ── HTML ─────────────────────────────────────────────────────────────────────

/// Regex baseline — always available.
pub fn minify_html_core(content: &str) -> String {
    let s = remove_comments(content, &["html"]);
    let s = super::collapse_whitespace(&s);
    let s = s.replace("> <", "><");
    s.trim().to_owned()
}

/// High-quality HTML minification via minify-html crate.
/// Uses `minify_html_core` on error.
pub fn minify_html_quality(content: &str) -> String {
    std::panic::catch_unwind(|| {
        use minify_html::{minify, Cfg};
        let cfg = Cfg {
            minify_css: true,
            minify_js: false,
            ..Cfg::default()
        };
        let out = minify(content.as_bytes(), &cfg);
        String::from_utf8(out).unwrap_or_else(|_| minify_html_core(content))
    })
    .unwrap_or_else(|_| minify_html_core(content))
}
