//! Go modules that are local code though the scan does not hold them: what a
//! `replace` to a local path or a `go.work` member names. An import of one is
//! an unscanned part of the repository, not a third-party module.
use super::go_module_path;
use std::path::{Path, PathBuf};

/// The entries of `keyword` in a `go.mod` or `go.work`, one-line or in a block.
fn directive_entries<'t>(text: &'t str, keyword: &str) -> Vec<&'t str> {
    let mut entries = Vec::new();
    let mut in_block = false;
    for line in text.lines().map(str::trim) {
        if in_block {
            if line == ")" {
                in_block = false;
            } else if !line.is_empty() && !line.starts_with("//") {
                entries.push(line);
            }
        } else if let Some(rest) = line.strip_prefix(keyword) {
            match rest.trim() {
                "(" => in_block = true,
                "" => {}
                entry if rest.starts_with(char::is_whitespace) => entries.push(entry),
                _ => {}
            }
        }
    }
    entries
}

/// The modules a `go.mod` replaces with a local directory.
pub(super) fn local_replacements(text: &str) -> Vec<String> {
    directive_entries(text, "replace")
        .into_iter()
        .filter_map(|entry| {
            let (old, new) = entry.split_once("=>")?;
            new.trim()
                .starts_with(['.', '/'])
                .then(|| old.split_whitespace().next().map(str::to_owned))?
        })
        .collect()
}

/// The module paths of the `go.work` members at or above `root`.
pub(super) fn workspace_members(
    root: &Path,
    above: &[PathBuf],
    read: &dyn Fn(&Path) -> Option<String>,
) -> Vec<String> {
    let work = std::iter::once(root)
        .chain(above.iter().map(PathBuf::as_path))
        .find_map(|dir| Some((dir, read(&dir.join("go.work"))?)));
    let Some((dir, text)) = work else {
        return Vec::new();
    };
    directive_entries(&text, "use")
        .into_iter()
        .filter_map(|member| {
            let member = member.split("//").next()?.trim().trim_matches('"');
            go_module_path(&read(&dir.join(member).join("go.mod"))?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directives_read_as_one_line_or_a_block() {
        let text = "module x.io/a\nreplace x.io/b => ../b\nreplace x.io/far v1.0.0 => example.com/fork v1.0.0\nreplace (\n\tx.io/c v1.2.3 => ./c\n\t// note\n\tx.io/d => example.com/d v1\n)\n";
        assert_eq!(local_replacements(text), ["x.io/b", "x.io/c"]);
        assert_eq!(
            directive_entries("go 1.22\nuse ./a\nuse (\n\t./b\n)\nuseless\n", "use"),
            ["./a", "./b"]
        );
    }
}
