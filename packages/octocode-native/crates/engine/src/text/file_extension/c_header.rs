//! The dialect of a `.h` header. C and C++ share the extension, and neither
//! the C nor the C++ standard names one; compilers take the including
//! translation unit's language (scip-clang indexes a header inside the
//! translation units that include it) and clangd's fallback command reads a
//! bare `.h` as C++. Here a header reads as C++ only when it holds syntax C
//! cannot (a `namespace` or `template` declaration, a `class`, `using
//! namespace`, `extern "C++"`), and as CUDA (C++ with execution spaces)
//! when a declaration names `__global__` or `__device__`; every other header
//! stays C, so a C header's identifiers that are C++ keywords (`new`,
//! `this`) never break its parse.

/// Grammar extension for a `.h` header holding `content`, or `None` when
/// it stays C (or its dialect's grammar is not compiled in).
pub(super) fn header_grammar(content: &str) -> Option<&'static str> {
    // A macro may spell an execution space for both dialects
    // (`#define HD __host__ __device__`); only a declaration proves one.
    let cuda = content.lines().any(|line| {
        !line.trim_start().starts_with('#')
            && line
                .split(|c: char| !is_ident(c))
                .any(|word| matches!(word, "__global__" | "__device__"))
    });
    if cuda && cfg!(feature = "tree-sitter-cuda") {
        Some("cuh")
    } else if (cuda || content.lines().any(cpp_only_line)) && cfg!(feature = "tree-sitter-cpp") {
        Some("hpp")
    } else {
        None
    }
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The text after `keyword` when `line` starts with it as a whole word.
fn after_keyword<'l>(line: &'l str, keyword: &str) -> Option<&'l str> {
    let rest = line.strip_prefix(keyword)?;
    (!rest.starts_with(is_ident)).then(|| rest.trim_start())
}

/// A line opening a declaration only C++ has. Each form starts the line, so
/// comments, strings and expressions never match.
fn cpp_only_line(line: &str) -> bool {
    let line = line.trim_start();
    let starts_name = |rest: &str| rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    if let Some(rest) = after_keyword(line, "namespace") {
        return starts_name(rest) || rest.starts_with('{');
    }
    if let Some(rest) = after_keyword(line, "template") {
        return rest.starts_with('<');
    }
    if let Some(rest) = after_keyword(line, "using") {
        return after_keyword(rest, "namespace").is_some_and(starts_name);
    }
    if let Some(rest) = after_keyword(line, "class") {
        return starts_name(rest);
    }
    after_keyword(line, "extern").is_some_and(|rest| rest.starts_with("\"C++\""))
}

#[cfg(test)]
mod tests;
