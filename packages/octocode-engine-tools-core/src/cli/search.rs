use clap::Parser;
use serde_json::{Value, json};
use std::io::{self, Write};

#[derive(Parser)]
pub struct SearchArgs {
    pub pattern: String,
    #[arg(default_value = ".")]
    pub paths: Vec<String>,
    #[arg(short = 'F', long)]
    pub fixed_strings: bool,
    #[arg(short = 'i', long, conflicts_with = "smart_case")]
    pub ignore_case: bool,
    #[arg(short = 'S', long)]
    pub smart_case: bool,
    #[arg(short = 'w', long)]
    pub word_regexp: bool,
    #[arg(short = 'v', long)]
    pub invert_match: bool,
    #[arg(short = 'g', long = "glob")]
    pub include: Vec<String>,
    #[arg(long)]
    pub exclude: Vec<String>,
    #[arg(long)]
    pub exclude_dir: Vec<String>,
    #[arg(short = 't', long = "type")]
    pub lang_type: Option<String>,
    #[arg(short = 'C', long, default_value_t = 0)]
    pub context: u32,
    #[arg(short = 'l', long, conflicts_with_all = ["count", "quiet", "view"])]
    pub files: bool,
    #[arg(short = 'c', long, conflicts_with_all = ["quiet", "view"])]
    pub count: bool,
    #[arg(short = 'q', long, conflicts_with = "view")]
    pub quiet: bool,
    #[arg(long)]
    pub hidden: bool,
    #[arg(long)]
    pub no_ignore: bool,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub max_depth: Option<u32>,
    #[arg(long)]
    pub max_files: Option<u32>,
    #[arg(long)]
    pub max_matches_per_file: Option<u32>,
    #[arg(long)]
    pub page_size: Option<u32>,
    #[arg(long)]
    pub page: Option<u32>,
    #[arg(long)]
    pub match_page: Option<u32>,
    #[arg(long)]
    pub match_window: Option<u32>,
    #[arg(long)]
    pub match_content_length: Option<u32>,
    #[arg(long)]
    pub snapshot: Option<String>,
    #[arg(long, value_parser=["literal", "rust", "pcre2"], conflicts_with="fixed_strings")]
    pub regex: Option<String>,
    #[arg(long, value_parser=["off", "on", "dotall"])]
    pub multiline: Option<String>,
    #[arg(long, default_value="traversal", value_parser=["traversal", "path", "relevance", "matchCount", "modified", "accessed", "created"])]
    pub sort: String,
    #[arg(long)]
    pub reverse: bool,
    #[arg(long)]
    pub ranking_profile: Option<String>,
    #[arg(long, value_parser=["matchOnly", "discovery", "detailed", "paginated", "content", "files", "filesWithout", "countLines", "countMatches"])]
    pub view: Option<String>,
    #[arg(long, value_parser=["off", "list", "count"])]
    pub unique: Option<String>,
}

impl SearchArgs {
    pub fn queries(&self) -> io::Result<Value> {
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
            .map(|queries| json!(queries))
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
