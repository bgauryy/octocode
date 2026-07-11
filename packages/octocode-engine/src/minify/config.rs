use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone)]
pub struct FileTypeConfig {
    pub strategy: &'static str,
    /// None → no comment stripping; Some → one or more CommentPatternGroup names
    pub comments: Option<&'static [&'static str]>,
}

impl FileTypeConfig {
    const fn new(strategy: &'static str) -> Self {
        Self {
            strategy,
            comments: None,
        }
    }
    const fn with(strategy: &'static str, comments: &'static [&'static str]) -> Self {
        Self {
            strategy,
            comments: Some(comments),
        }
    }
}

static MINIFY_CONFIG: LazyLock<HashMap<&'static str, FileTypeConfig>> = LazyLock::new(|| {
    let mut m: HashMap<&'static str, FileTypeConfig> = HashMap::new();

    // JS / TS
    macro_rules! insert { ($($ext:literal => $cfg:expr),* $(,)?) => { $(m.insert($ext, $cfg);)* }; }
    insert! {
        "js"    => FileTypeConfig::with("terser",       &["c-style"]),
        "jsx"   => FileTypeConfig::with("terser",       &["c-style"]),
        "mjs"   => FileTypeConfig::with("terser",       &["c-style"]),
        "cjs"   => FileTypeConfig::with("terser",       &["c-style"]),
        "ts"    => FileTypeConfig::with("conservative", &["c-style"]),
        "tsx"   => FileTypeConfig::with("conservative", &["c-style"]),
        "mts"   => FileTypeConfig::with("conservative", &["c-style"]),
        "cts"   => FileTypeConfig::with("conservative", &["c-style"]),
        // Python / scripting
        "py"    => FileTypeConfig::with("conservative", &["hash","python-docstring"]),
        "pyi"   => FileTypeConfig::with("conservative", &["hash","python-docstring"]),
        "yaml"  => FileTypeConfig::with("conservative", &["hash"]),
        "yml"   => FileTypeConfig::with("conservative", &["hash"]),
        "coffee"=> FileTypeConfig::with("conservative", &["hash"]),
        "nim"   => FileTypeConfig::with("conservative", &["hash"]),
        "haml"  => FileTypeConfig::with("conservative", &["hash","haml"]),
        "slim"  => FileTypeConfig::with("conservative", &["hash","slim"]),
        "sass"  => FileTypeConfig::with("conservative", &["c-style"]),
        "styl"  => FileTypeConfig::with("conservative", &["c-style"]),
        // HTML / XML
        "html"  => FileTypeConfig::with("aggressive",   &["html"]),
        "htm"   => FileTypeConfig::with("aggressive",   &["html"]),
        "xml"   => FileTypeConfig::with("aggressive",   &["html"]),
        "svg"   => FileTypeConfig::with("aggressive",   &["html"]),
        // CSS
        "css"   => FileTypeConfig::with("aggressive",   &["c-style"]),
        "less"  => FileTypeConfig::with("aggressive",   &["c-style"]),
        "scss"  => FileTypeConfig::with("aggressive",   &["c-style"]),
        // JSON
        "json"  => FileTypeConfig::new("json"),
        "jsonc" => FileTypeConfig::new("json"),
        "json5" => FileTypeConfig::new("json"),
        // C-family
        "go"    => FileTypeConfig::with("conservative", &["c-style"]),
        "java"  => FileTypeConfig::with("conservative", &["c-style"]),
        "c"     => FileTypeConfig::with("conservative", &["c-style"]),
        "h"     => FileTypeConfig::with("conservative", &["c-style"]),
        "cpp"   => FileTypeConfig::with("conservative", &["c-style"]),
        "hpp"   => FileTypeConfig::with("conservative", &["c-style"]),
        "cc"    => FileTypeConfig::with("conservative", &["c-style"]),
        "cxx"   => FileTypeConfig::with("conservative", &["c-style"]),
        "hh"    => FileTypeConfig::with("conservative", &["c-style"]),
        "hxx"   => FileTypeConfig::with("conservative", &["c-style"]),
        "cs"    => FileTypeConfig::with("conservative", &["c-style"]),
        "vb"    => FileTypeConfig::with("conservative", &["apostrophe"]),
        "vbs"   => FileTypeConfig::with("conservative", &["apostrophe"]),
        "rs"    => FileTypeConfig::with("conservative", &["c-style"]),
        "rust"  => FileTypeConfig::with("conservative", &["c-style"]),
        "swift" => FileTypeConfig::with("conservative", &["c-style"]),
        "kt"    => FileTypeConfig::with("conservative", &["c-style"]),
        "kts"   => FileTypeConfig::with("conservative", &["c-style"]),
        "kotlin"=> FileTypeConfig::with("conservative", &["c-style"]),
        "scala" => FileTypeConfig::with("conservative", &["c-style"]),
        "dart"  => FileTypeConfig::with("conservative", &["c-style"]),
        "groovy"=> FileTypeConfig::with("conservative", &["c-style"]),
        "gradle"=> FileTypeConfig::with("conservative", &["c-style"]),
        "mm"    => FileTypeConfig::with("conservative", &["c-style"]),
        "pas"   => FileTypeConfig::with("conservative", &["pascal"]),
        "adb"   => FileTypeConfig::with("conservative", &["double-dash"]),
        "ads"   => FileTypeConfig::with("conservative", &["double-dash"]),
        "f"     => FileTypeConfig::with("conservative", &["bang"]),
        "for"   => FileTypeConfig::with("conservative", &["bang"]),
        "f90"   => FileTypeConfig::with("conservative", &["bang"]),
        "f95"   => FileTypeConfig::with("conservative", &["bang"]),
        "f03"   => FileTypeConfig::with("conservative", &["bang"]),
        "f08"   => FileTypeConfig::with("conservative", &["bang"]),
        "zig"   => FileTypeConfig::with("conservative", &["c-style"]),
        "v"     => FileTypeConfig::with("conservative", &["c-style"]),
        "jl"    => FileTypeConfig::with("conservative", &["hash"]),
        "nix"   => FileTypeConfig::with("conservative", &["hash","c-style"]),
        "php"   => FileTypeConfig::with("conservative", &["c-style","hash"]),
        "rb"    => FileTypeConfig::with("conservative", &["hash"]),
        "rake"  => FileTypeConfig::with("conservative", &["hash"]),
        "gemspec" => FileTypeConfig::with("conservative", &["hash"]),
        "ru"    => FileTypeConfig::with("conservative", &["hash"]),
        "perl"  => FileTypeConfig::with("conservative", &["hash"]),
        "sh"    => FileTypeConfig::with("conservative", &["hash"]),
        "bash"  => FileTypeConfig::with("conservative", &["hash"]),
        "zsh"   => FileTypeConfig::with("conservative", &["hash"]),
        "fish"  => FileTypeConfig::with("conservative", &["hash"]),
        "ps1"   => FileTypeConfig::with("conservative", &["powershell"]),
        "psm1"  => FileTypeConfig::with("conservative", &["powershell"]),
        "psd1"  => FileTypeConfig::with("conservative", &["powershell"]),
        "sql"   => FileTypeConfig::with("conservative", &["sql"]),
        "tsql"  => FileTypeConfig::with("conservative", &["sql"]),
        "plsql" => FileTypeConfig::with("conservative", &["sql"]),
        "lua"   => FileTypeConfig::with("aggressive",   &["lua"]),
        "r"     => FileTypeConfig::with("aggressive",   &["hash"]),
        "hbs"   => FileTypeConfig::with("aggressive",   &["template"]),
        "handlebars" => FileTypeConfig::with("aggressive", &["template"]),
        "ejs"   => FileTypeConfig::with("aggressive",   &["template"]),
        "pug"   => FileTypeConfig::with("conservative", &["c-style"]),
        "jade"  => FileTypeConfig::with("conservative", &["c-style"]),
        "mustache" => FileTypeConfig::with("aggressive",&["template"]),
        "twig"  => FileTypeConfig::with("aggressive",   &["template"]),
        "jinja" => FileTypeConfig::with("aggressive",   &["template"]),
        "jinja2"=> FileTypeConfig::with("aggressive",   &["template"]),
        "erb"   => FileTypeConfig::with("aggressive",   &["template"]),
        "vue"   => FileTypeConfig::with("aggressive",   &["html"]),
        "svelte"=> FileTypeConfig::with("aggressive",   &["html"]),
        "xsl"   => FileTypeConfig::with("aggressive",   &["html"]),
        "xslt"  => FileTypeConfig::with("aggressive",   &["html"]),
        "graphql" => FileTypeConfig::with("conservative",&["hash"]),
        "gql"   => FileTypeConfig::with("conservative", &["hash"]),
        "proto" => FileTypeConfig::with("conservative", &["c-style"]),
        "csv"   => FileTypeConfig::new("conservative"),
        "toml"  => FileTypeConfig::with("conservative", &["hash"]),
        "ini"   => FileTypeConfig::with("conservative", &["hash","semicolon"]),
        "conf"  => FileTypeConfig::with("conservative", &["hash"]),
        "config"=> FileTypeConfig::with("conservative", &["hash"]),
        "env"   => FileTypeConfig::with("conservative", &["hash"]),
        "properties" => FileTypeConfig::with("conservative",&["hash"]),
        "tf"    => FileTypeConfig::with("conservative", &["hash","c-style"]),
        "hcl"   => FileTypeConfig::with("conservative", &["hash","c-style"]),
        "tfvars"=> FileTypeConfig::with("conservative", &["hash","c-style"]),
        "pp"    => FileTypeConfig::with("conservative", &["hash"]),
        "md"    => FileTypeConfig::new("markdown"),
        "markdown" => FileTypeConfig::new("markdown"),
        "mdx"   => FileTypeConfig::new("markdown"),
        "rst"   => FileTypeConfig::with("conservative", &["hash"]),
        "star"  => FileTypeConfig::with("conservative", &["hash"]),
        "bzl"   => FileTypeConfig::with("conservative", &["hash"]),
        "cmake" => FileTypeConfig::with("conservative", &["hash"]),
        "awk"   => FileTypeConfig::with("conservative", &["hash"]),
        "pl"    => FileTypeConfig::with("aggressive",   &["hash"]),
        "pm"    => FileTypeConfig::with("aggressive",   &["hash"]),
        "fs"    => FileTypeConfig::with("conservative", &["c-style","fsharp-block"]),
        "fsx"   => FileTypeConfig::with("conservative", &["c-style","fsharp-block"]),
        // OCaml uses (* ... *) nested block comments — same syntax as F# fsharp-block
        "ml"    => FileTypeConfig::with("conservative", &["fsharp-block"]),
        "mli"   => FileTypeConfig::with("conservative", &["fsharp-block"]),
        "hs"    => FileTypeConfig::with("conservative", &["haskell"]),
        "lhs"   => FileTypeConfig::with("conservative", &["haskell"]),
        "elm"   => FileTypeConfig::with("conservative", &["c-style"]),
        "lisp"  => FileTypeConfig::with("conservative", &["semicolon"]),
        "lsp"   => FileTypeConfig::with("conservative", &["semicolon"]),
        "scm"   => FileTypeConfig::with("conservative", &["semicolon"]),
        "rkt"   => FileTypeConfig::with("conservative", &["semicolon"]),
        "clj"   => FileTypeConfig::with("aggressive",   &["clojure"]),
        "cljs"  => FileTypeConfig::with("aggressive",   &["clojure"]),
        "ex"    => FileTypeConfig::with("aggressive",   &["hash"]),
        "exs"   => FileTypeConfig::with("aggressive",   &["hash"]),
        "erl"   => FileTypeConfig::with("aggressive",   &["percent"]),
        "hrl"   => FileTypeConfig::with("aggressive",   &["percent"]),
        "vhd"   => FileTypeConfig::with("conservative", &["double-dash"]),
        "vhdl"  => FileTypeConfig::with("conservative", &["double-dash"]),
        "asm"   => FileTypeConfig::with("conservative", &["semicolon"]),
        "nasm"  => FileTypeConfig::with("conservative", &["semicolon"]),
        "wat"   => FileTypeConfig::with("conservative", &["wasm-text"]),
        "wast"  => FileTypeConfig::with("conservative", &["wasm-text"]),
        "txt"   => FileTypeConfig::new("general"),
        "log"   => FileTypeConfig::new("general"),
        "cfg"   => FileTypeConfig::with("conservative", &["hash"]),
        "gitignore"   => FileTypeConfig::with("conservative",&["hash"]),
        "dockerignore"=> FileTypeConfig::with("conservative",&["hash"]),
    }
    m
});

pub fn minify_config() -> &'static HashMap<&'static str, FileTypeConfig> {
    &MINIFY_CONFIG
}

static INDENTATION_SENSITIVE_NAMES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "makefile",
        "dockerfile",
        "procfile",
        "justfile",
        "rakefile",
        "gemfile",
        "podfile",
        "fastfile",
        "vagrantfile",
        "jenkinsfile",
        "cakefile",
        "pipfile",
        "buildfile",
        "capfile",
        "brewfile",
    ]
    .into_iter()
    .collect()
});

pub fn indentation_sensitive_names() -> &'static HashSet<&'static str> {
    &INDENTATION_SENSITIVE_NAMES
}
