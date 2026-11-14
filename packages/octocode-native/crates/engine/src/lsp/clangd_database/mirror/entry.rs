//! What one entry of a project's compilation database may keep in its copy: only
//! a source of this project, whose flags read nothing outside it and hold no secret.
use super::*;
use serde_json::{Map, Value, json};

/// Whether an entry reads only what this project owns. A repository's database
/// chooses what a background index reads at the first query, with no request
/// naming those files, and the index keeps their names on disk. So an entry
/// stays only for a C-family source whose real path lies inside `root` under no
/// hidden name (not `.ssh/k.c`, `.env.c`, a link to either, or `prod.env` compiled
/// with `-x c`), and whose flags pass [`flags_stay_inside`]. Paths are judged real,
/// so a database spelled through a symlink (`/tmp`) keeps its entries. A file left
/// out is still read if a request asks, through the path policy.
pub(super) fn stays_in_project(entry: &Value, directory: &Path, root: &Path) -> bool {
    let inside = |path: &Path| {
        let path = directory.join(path);
        let path = path
            .canonicalize()
            .unwrap_or_else(|_| crate::text::lexical(&path));
        path.strip_prefix(root).is_ok_and(|inner| {
            !inner
                .components()
                .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
        })
    };
    let spelled = entry["file"].as_str().unwrap_or_default();
    let file = Path::new(spelled);
    let is_source = is_clangd_source(file);
    if !is_source || !inside(file) {
        return false;
    }
    let words = match entry["arguments"].as_array() {
        Some(arguments) => arguments
            .iter()
            .filter_map(|word| word.as_str().map(str::to_owned))
            .collect::<Vec<_>>(),
        None => shell_words(entry["command"].as_str().unwrap_or_default()),
    };
    flags_stay_inside(&words, spelled, directory, &inside)
}

/// Whether clangd compiles `file` by its name, so a `prod.env` compiled with `-x c`
/// is none: anything routed to clangd, and the spellings clang also compiles that no
/// language route uses.
fn is_clangd_source(file: &Path) -> bool {
    let name = file.to_string_lossy().to_lowercase();
    crate::lsp::family::is_c_family(crate::lsp::config::detect_language_id(&name).as_deref())
        || Path::new(&name)
            .extension()
            .is_some_and(|extension| COMPILED_UNROUTED.iter().any(|known| extension == *known))
}

/// Whether no flag can read a file outside the project. Clang has hundreds of
/// options that take a file and more each release, so this fails closed: a flag
/// known to read no input (a search path, an output, a macro) is passed over, and
/// anything else that names a path must name one inside. A value is a path when
/// it is absolute, has a separator, or names an existing file; `-x c` from a build
/// folder outside the project names none.
fn flags_stay_inside(
    words: &[String],
    source: &str,
    directory: &Path,
    inside: &impl Fn(&Path) -> bool,
) -> bool {
    let reads_inside = |value: &str| {
        let path = Path::new(value);
        let is_path = path.is_absolute()
            || value.contains(['/', '\\'])
            || directory.join(path).symlink_metadata().is_ok();
        !is_path || inside(path)
    };
    // `-Xclang -include -Xclang <file>` passes each word through.
    let mut words = words.iter().filter(|word| *word != "-Xclang");
    let mut in_flags = false;
    while let Some(word) = words.next() {
        if hides_flags(word) {
            return false;
        }
        in_flags |= word.starts_with('-');
        if !in_flags {
            // The compiler, and any launcher before it (`ccache /usr/bin/cc`).
            if word.contains("clang-cl") {
                return false;
            }
            continue;
        }
        if NO_INPUT_JOINED.iter().any(|flag| word.starts_with(flag)) {
            continue;
        }
        if NO_INPUT.iter().any(|flag| word.starts_with(flag)) {
            if NO_INPUT.contains(&word.as_str()) {
                words.next();
            }
            continue;
        }
        if let Some(value) = FORCED_INCLUDES
            .iter()
            .find_map(|flag| forced_value(word, flag))
        {
            // Clang looks for a relative forced file in the working directory
            // first, then along the search paths, which reach outside the project
            // (`-I`, `-isystem`, `-iquote`, the system headers): only a file the
            // project holds there, or an absolute path inside it, stays.
            let value = match value {
                "" => words.next().map_or("", String::as_str),
                value => value,
            };
            let path = Path::new(value);
            if !(path.is_absolute() || directory.join(path).is_file()) || !inside(path) {
                return false;
            }
            continue;
        }
        if word != source && !values_of(word).all(reads_inside) {
            return false;
        }
    }
    true
}

/// A flag that brings in more flags than the entry shows, or another driver's
/// syntax that nothing here reads.
fn hides_flags(word: &str) -> bool {
    word.starts_with('@')
        || word.starts_with("-Wp,")
        || word.starts_with("--driver-mode")
        || word.starts_with("/FI")
        || word.starts_with("-FI")
        || word.starts_with("/clang:")
        || word == "--config"
        || word.starts_with("--config=")
}

/// The value `word` gives forced-include `flag`, empty when it is the next word.
/// `-include` joins its value; a `--` spelling takes `=`, so `--include-directory`
/// is another flag.
fn forced_value<'a>(word: &'a str, flag: &str) -> Option<&'a str> {
    let value = word.strip_prefix(flag)?;
    match value.strip_prefix('=') {
        Some(value) => Some(value),
        None if flag.starts_with("--") && !value.is_empty() => None,
        None => Some(value),
    }
}

/// What `word` could name: itself when no flag, else each `=` or `,` part after
/// the flag's name (`-fmodule-file=m=/x`), and what follows a separator in the name
/// (`-ivfsoverlay/x`).
fn values_of(word: &str) -> impl Iterator<Item = &str> {
    let (name, rest) = word.split_once('=').unwrap_or((word, ""));
    let joined = if name.starts_with('-') {
        name.find(['/', '\\']).map_or("", |at| &name[at..])
    } else {
        name
    };
    std::iter::once(joined)
        .chain(rest.split(['=', ',']))
        .filter(|value| !value.is_empty())
}

/// `entry` as the copy holds it: only the fields clang reads, with every value the
/// secret scanner recognizes replaced by its marker. The flag stays, so the macro
/// stays defined and the file still indexes. A `command` holding one becomes
/// `arguments`, word by word, so the marker cannot change how the rest splits; a
/// path holding one would name no file, so the entry leaves. `None` also when the
/// scanner failed: nothing it did not read is written.
pub(super) fn without_secrets(entry: &Value, scans: &mut Scans) -> Option<Value> {
    let mut kept = Map::new();
    for field in ["directory", "file"] {
        let path = entry[field].as_str().unwrap_or_default();
        scans.redacted(path).ok()?.is_none().then_some(())?;
        kept.insert(field.to_owned(), json!(path));
    }
    if let Some(arguments) = entry["arguments"].as_array() {
        let words = arguments
            .iter()
            .filter_map(Value::as_str)
            .map(|word| scans.cleaned(word))
            .collect::<Option<Vec<_>>>()?;
        kept.insert("arguments".to_owned(), json!(words));
    } else if let Some(command) = entry["command"].as_str() {
        let words = shell_words(command);
        let clean = words
            .iter()
            .map(|word| scans.cleaned(word))
            .collect::<Option<Vec<_>>>()?;
        if clean == words {
            kept.insert("command".to_owned(), json!(command));
        } else {
            kept.insert("arguments".to_owned(), json!(clean));
        }
    }
    if let Some(output) = entry["output"].as_str() {
        kept.insert("output".to_owned(), json!(scans.cleaned(output)?));
    }
    Some(Value::Object(kept))
}

/// What the scanner said of each word: a database repeats most flags on every
/// entry, and it is read at every server start.
#[derive(Default)]
pub(super) struct Scans(std::collections::HashMap<String, Option<String>>);

impl Scans {
    /// `text`'s redacted form when it holds a secret.
    fn redacted(&mut self, text: &str) -> crate::error::Result<Option<String>> {
        if let Some(known) = self.0.get(text) {
            return Ok(known.clone());
        }
        let found = crate::security::sanitizer::sanitize_leaf(text)?;
        self.0.insert(text.to_owned(), found.clone());
        Ok(found)
    }

    /// `text` with any secret in it replaced; `None` when the scanner failed.
    fn cleaned(&mut self, text: &str) -> Option<String> {
        let redacted = self.redacted(text).ok()?;
        Some(redacted.unwrap_or_else(|| text.to_owned()))
    }
}

/// `command` split as a shell splits it: a quoted path is one word, without quotes.
fn shell_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut letters = command.chars();
    while let Some(letter) = letters.next() {
        match (quote, letter) {
            (Some(open), _) if letter == open => quote = None,
            (Some('"'), '\\') | (None, '\\') => {
                word.extend(letters.next());
                started = true;
            }
            (Some(_), _) => word.push(letter),
            (None, '"' | '\'') => {
                quote = Some(letter);
                started = true;
            }
            (None, _) if letter.is_whitespace() => {
                if std::mem::take(&mut started) {
                    words.push(std::mem::take(&mut word));
                }
            }
            (None, _) => {
                word.push(letter);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

/// Sources clang compiles that have no language route here. An index holds their
/// uses of a routed file's symbols.
const COMPILED_UNROUTED: [&str; 3] = ["c++", "m", "mm"];

/// Flags that read the file they name, whichever way it is written. `-include-pch`
/// comes first: `-include` would take its name for a joined value.
const FORCED_INCLUDES: [&str; 5] = [
    "-include-pch",
    "-include",
    "-imacros",
    "--include",
    "--imacros",
];

/// Flags that read no input, with their value joined or as the next word: search
/// paths a source's own `#include` must still name, outputs, and macros.
const NO_INPUT: [&str; 27] = [
    "-D",
    "-U",
    "-I",
    "-F",
    "-L",
    "-B",
    "-o",
    "-MF",
    "-MT",
    "-MQ",
    "-MJ",
    "-isystem",
    "-iquote",
    "-idirafter",
    "-iframework",
    "-isysroot",
    "-iwithsysroot",
    "-iprefix",
    "-iwithprefix",
    "-iwithprefixbefore",
    "-cxx-isystem",
    "-stdlib++-isystem",
    "-gcc-toolchain",
    "-resource-dir",
    "--sysroot",
    "--include-directory",
    "--output",
];

/// Flags that read no input, with their value only joined: the next word is not
/// theirs, so it is judged. These also take a path that is only recorded or that
/// another tool reads.
const NO_INPUT_JOINED: [&str; 11] = [
    "-Wa,",
    "-Wl,",
    "--gcc-toolchain=",
    "--gcc-install-dir=",
    "-ffile-prefix-map=",
    "-fdebug-prefix-map=",
    "-fmacro-prefix-map=",
    "-fcoverage-prefix-map=",
    "-fdebug-compilation-dir=",
    "-fcoverage-compilation-dir=",
    "--embed-dir=",
];
