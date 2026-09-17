use clap::Parser;
use serde_json::{Value, json};
use std::io::{self, Write};

/// Search for text or a regex pattern across local files.
#[derive(Parser)]
pub struct SearchArgs {
    /// Text pattern to search for; regex by default, literal with -F.
    pub pattern: String,
    /// Files or directories to search (default: current directory).
    #[arg(default_value = ".")]
    pub paths: Vec<String>,
    /// Treat the pattern as a plain literal string (no regex meta-characters).
    #[arg(short = 'F', long)]
    pub fixed_strings: bool,
    /// Case-insensitive matching.
    #[arg(short = 'i', long, conflicts_with = "smart_case")]
    pub ignore_case: bool,
    /// Smart case: case-insensitive when the pattern is all-lowercase, sensitive otherwise.
    #[arg(short = 'S', long)]
    pub smart_case: bool,
    /// Match whole words only.
    #[arg(short = 'w', long)]
    pub word_regexp: bool,
    /// Print lines that do NOT match the pattern.
    #[arg(short = 'v', long)]
    pub invert_match: bool,
    /// Include only files matching this glob; prefix with `!` to exclude instead.
    #[arg(short = 'g', long = "glob")]
    pub include: Vec<String>,
    /// Exclude files matching this glob pattern.
    #[arg(long)]
    pub exclude: Vec<String>,
    /// Skip entire directories with this name (e.g. `node_modules`, `target`).
    #[arg(long)]
    pub exclude_dir: Vec<String>,
    /// Restrict to files of this language type (e.g. `rust`, `ts`, `py`).
    #[arg(short = 't', long = "type")]
    pub lang_type: Option<String>,
    /// Lines of surrounding context to show around each match.
    #[arg(short = 'C', long, default_value_t = 0)]
    pub context: u32,
    /// Print only the names of files that contain a match.
    #[arg(short = 'l', long, conflicts_with_all = ["count", "quiet", "view"])]
    pub files: bool,
    /// Print the total match count per file.
    #[arg(short = 'c', long, conflicts_with_all = ["quiet", "view"])]
    pub count: bool,
    /// Print only file names; suppress match content (same effect as -l).
    #[arg(short = 'q', long, conflicts_with = "view")]
    pub quiet: bool,
    /// Also search dot-files and dot-directories.
    #[arg(long)]
    pub hidden: bool,
    /// Search even inside paths excluded by .gitignore.
    #[arg(long)]
    pub no_ignore: bool,
    /// Automatically fetch every result page until all results are returned.
    #[arg(long)]
    pub all: bool,
    /// Emit one structured JSON document instead of grep-style rows.
    #[arg(long, conflicts_with_all = ["quiet", "all"])]
    pub json: bool,
    /// Emit compact single-line JSON (implies --json).
    #[arg(long, conflicts_with_all = ["quiet", "all"])]
    pub compact: bool,
    /// Descend at most N directory levels.
    #[arg(long)]
    pub max_depth: Option<u32>,
    /// Stop after scanning this many files.
    #[arg(long)]
    pub max_files: Option<u32>,
    /// Collect at most this many matches per file.
    #[arg(long)]
    pub max_matches_per_file: Option<u32>,
    /// Number of files returned per result page.
    #[arg(long)]
    pub page_size: Option<u32>,
    /// Result page to return (1-based; use with --snapshot).
    #[arg(long)]
    pub page: Option<u32>,
    /// Match sub-page within a large file.
    #[arg(long)]
    pub match_page: Option<u32>,
    /// Characters of context around each matched value in `matchOnly` view.
    #[arg(long)]
    pub match_window: Option<u32>,
    /// Maximum characters retained per matched line.
    #[arg(long)]
    pub match_content_length: Option<u32>,
    /// Resume a previous paginated search with this opaque continuation token.
    #[arg(long)]
    pub snapshot: Option<String>,
    /// Regex engine: `literal` (exact string), `rust` (default), `pcre2` (PCRE2 syntax).
    #[arg(long, value_parser=["literal", "rust", "pcre2"], conflicts_with="fixed_strings")]
    pub regex: Option<String>,
    /// Multi-line mode: `off` (default), `on` (^ and $ match line boundaries), `dotall` (`.` spans newlines).
    #[arg(long, value_parser=["off", "on", "dotall"])]
    pub multiline: Option<String>,
    /// Sort order: `relevance` (default), `matchCount`, `path`, `modified`, `accessed`, `created`.
    #[arg(long, default_value="relevance", value_parser=["relevance", "matchCount", "path", "modified", "accessed", "created"])]
    pub sort: String,
    /// Reverse the selected sort order.
    #[arg(long)]
    pub reverse: bool,
    /// Language-specific ranking profile (auto-detected by default).
    #[arg(long)]
    pub ranking_profile: Option<String>,
    /// Output format: `paginated` (default with snippets), `files` (paths only), `matchOnly` (values), `countLines`, `countMatches`, `detailed`, `content`, `filesWithout`, `discovery`.
    #[arg(long, value_parser=["matchOnly", "discovery", "detailed", "paginated", "content", "files", "filesWithout", "countLines", "countMatches"])]
    pub view: Option<String>,
    /// Deduplicate matched values in `matchOnly` view: `list` (unique values) or `count` (frequency table).
    #[arg(long, value_parser=["off", "list", "count"])]
    pub unique: Option<String>,
}

impl SearchArgs {
    /// Returns one query object per path. Callers iterate and execute each
    /// query individually — multi-query batching has been removed.
    pub fn queries(&self) -> io::Result<Vec<Value>> {
        let view = if self.files || self.quiet {
            "files"
        } else if self.count {
            "countLines"
        } else {
            self.view.as_deref().unwrap_or("paginated")
        };
        let mut query = json!({
            "searchText":self.pattern, "regex":self.regex.as_deref().unwrap_or(if self.fixed_strings {"literal"} else {"rust"}),
            "caseMode":if self.ignore_case {"insensitive"} else if self.smart_case {"smart"} else {"sensitive"},
            "contextLines":self.context, "sort":self.sort, "resultView":view,
        });
        for (key, value) in [
            ("wholeWord", self.word_regexp),
            ("invertMatch", self.invert_match),
            ("hidden", self.hidden),
            ("noIgnore", self.no_ignore),
            ("reverse", self.reverse),
        ] {
            if value {
                query[key] = json!(true);
            }
        }
        let mut include = Vec::new();
        let mut exclude = self.exclude.clone();
        for pattern in &self.include {
            if let Some(pattern) = pattern.strip_prefix('!') {
                exclude.push(pattern.to_owned());
            } else {
                include.push(pattern.clone());
            }
        }
        for (key, values) in [
            ("include", &include),
            ("exclude", &exclude),
            ("excludeDir", &self.exclude_dir),
        ] {
            if !values.is_empty() {
                query[key] = json!(values);
            }
        }
        for (key, value) in [
            ("maxDepth", self.max_depth),
            ("maxFiles", self.max_files),
            ("maxMatchesPerFile", self.max_matches_per_file),
            ("pageSize", self.page_size),
            ("page", self.page),
            ("matchPage", self.match_page),
            ("matchWindow", self.match_window),
            ("matchContentLength", self.match_content_length),
        ] {
            if let Some(value) = value {
                query[key] = json!(value);
            }
        }
        for (key, value) in [
            ("snapshot", &self.snapshot),
            ("langType", &self.lang_type),
            ("multiline", &self.multiline),
            ("rankingProfile", &self.ranking_profile),
            ("unique", &self.unique),
        ] {
            if let Some(value) = value {
                query[key] = json!(value);
            }
        }
        self.paths
            .iter()
            .map(|path| {
                let mut item = query.clone();
                item["path"] = json!(std::path::absolute(path)?.to_string_lossy());
                Ok(item)
            })
            .collect::<io::Result<Vec<Value>>>()
    }
}

pub fn write_row(row: &Value, response: &Value) -> io::Result<bool> {
    let mut found = false;
    let mut stdout = io::stdout().lock();
    for file in row["data"]["files"].as_array().into_iter().flatten() {
        found = true;
        let path = file["path"].as_str().unwrap_or_default();
        let path = response["base"]
            .as_str()
            .map_or_else(|| path.to_owned(), |base| format!("{base}/{path}"));
        if let Some(matches) = file["matches"].as_array() {
            for item in matches {
                let text = item["value"].as_str().unwrap_or_default();
                write!(stdout, "{path}:{}:{}:{text}", item["line"], item["column"])?;
                if !text.ends_with('\n') {
                    writeln!(stdout)?;
                }
            }
        } else if let Some(count) = file
            .get("totalMatchedLines")
            .or_else(|| file.get("totalOccurrences"))
        {
            writeln!(stdout, "{path}:{count}")?;
        } else {
            writeln!(stdout, "{path}")?;
        }
    }
    Ok(found)
}
