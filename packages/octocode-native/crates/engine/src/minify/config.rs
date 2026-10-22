use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// How the content view minifies a file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Comments stripped, blank runs collapsed, indentation kept.
    Code,
    /// Comments stripped and indentation removed, as for JS/TS.
    Brace,
    /// Plain text: whitespace cleanup only.
    General,
    /// Readable JSON.
    Json,
    /// Markdown cleanup.
    Markdown,
}

#[derive(Debug, Clone)]
pub struct FileTypeConfig {
    pub strategy: Strategy,
    /// None → no comment stripping; Some → one or more `COMMENT_GROUPS` names
    pub comments: Option<&'static [&'static str]>,
}

impl FileTypeConfig {
    const fn new(strategy: Strategy) -> Self {
        Self {
            strategy,
            comments: None,
        }
    }
    const fn with(strategy: Strategy, comments: &'static [&'static str]) -> Self {
        Self {
            strategy,
            comments: Some(comments),
        }
    }
}

static MINIFY_CONFIG: LazyLock<HashMap<&'static str, FileTypeConfig>> = LazyLock::new(|| {
    use Strategy::{Brace, Code, General, Json, Markdown};
    let mut m: HashMap<&'static str, FileTypeConfig> = HashMap::new();

    // JS / TS
    macro_rules! insert { ($($ext:literal => $cfg:expr),* $(,)?) => { $(
        let earlier = m.insert($ext, $cfg);
        debug_assert!(earlier.is_none(), "minify config lists {} twice", $ext);
    )* }; }
    insert! {
        "js"    => FileTypeConfig::with(Code,          &["c-style"]),
        "jsx"   => FileTypeConfig::with(Code,          &["c-style"]),
        "mjs"   => FileTypeConfig::with(Code,          &["c-style"]),
        "cjs"   => FileTypeConfig::with(Code,          &["c-style"]),
        "ts"    => FileTypeConfig::with(Code, &["c-style"]),
        "tsx"   => FileTypeConfig::with(Code, &["c-style"]),
        "mts"   => FileTypeConfig::with(Code, &["c-style"]),
        "cts"   => FileTypeConfig::with(Code, &["c-style"]),
        // Python / scripting
        "py"    => FileTypeConfig::with(Code, &["hash","python-docstring"]),
        "pyi"   => FileTypeConfig::with(Code, &["hash","python-docstring"]),
        "yaml"  => FileTypeConfig::with(Code, &["hash"]),
        "yml"   => FileTypeConfig::with(Code, &["hash"]),
        "coffee"=> FileTypeConfig::with(Code, &["hash"]),
        "nim"   => FileTypeConfig::with(Code, &["hash"]),
        "haml"  => FileTypeConfig::with(Code, &["hash","haml"]),
        "slim"  => FileTypeConfig::with(Code, &["hash","slim"]),
        "sass"  => FileTypeConfig::with(Code, &["c-style"]),
        "styl"  => FileTypeConfig::with(Code, &["c-style"]),
        // HTML / XML
        "html"  => FileTypeConfig::with(Code,   &["html"]),
        "htm"   => FileTypeConfig::with(Code,   &["html"]),
        "xml"   => FileTypeConfig::with(Code,   &["html"]),
        "svg"   => FileTypeConfig::with(Code,   &["html"]),
        // CSS
        "css"   => FileTypeConfig::with(Code,   &["c-style"]),
        "less"  => FileTypeConfig::with(Code,   &["c-style"]),
        "scss"  => FileTypeConfig::with(Code,   &["c-style"]),
        // JSON
        "json"  => FileTypeConfig::new(Json),
        "jsonc" => FileTypeConfig::new(Json),
        "json5" => FileTypeConfig::new(Json),
        // C-family
        "go"    => FileTypeConfig::with(Brace, &["c-style"]),
        "java"  => FileTypeConfig::with(Brace, &["c-style"]),
        "c"     => FileTypeConfig::with(Brace, &["c-style"]),
        "h"     => FileTypeConfig::with(Brace, &["c-style"]),
        "cpp"   => FileTypeConfig::with(Brace, &["c-style"]),
        "hpp"   => FileTypeConfig::with(Brace, &["c-style"]),
        "cc"    => FileTypeConfig::with(Brace, &["c-style"]),
        "cxx"   => FileTypeConfig::with(Brace, &["c-style"]),
        "hh"    => FileTypeConfig::with(Brace, &["c-style"]),
        "hxx"   => FileTypeConfig::with(Brace, &["c-style"]),
        "cu"    => FileTypeConfig::with(Brace, &["c-style"]),
        "cuh"   => FileTypeConfig::with(Brace, &["c-style"]),
        "assembly" => FileTypeConfig::with(Code, &["c-style","hash","semicolon"]),
        "s"     => FileTypeConfig::with(Code, &["c-style","hash","semicolon"]),
        "cs"    => FileTypeConfig::with(Brace, &["c-style"]),
        "vb"    => FileTypeConfig::with(Code, &["apostrophe"]),
        "vbs"   => FileTypeConfig::with(Code, &["apostrophe"]),
        "rs"    => FileTypeConfig::with(Brace, &["c-style"]),
        "rust"  => FileTypeConfig::with(Brace, &["c-style"]),
        "swift" => FileTypeConfig::with(Brace, &["c-style"]),
        "kt"    => FileTypeConfig::with(Brace, &["c-style"]),
        "kts"   => FileTypeConfig::with(Brace, &["c-style"]),
        "kotlin"=> FileTypeConfig::with(Brace, &["c-style"]),
        "scala" => FileTypeConfig::with(Brace, &["c-style"]),
        "sc"    => FileTypeConfig::with(Brace, &["c-style"]),
        "sbt"   => FileTypeConfig::with(Brace, &["c-style"]),
        "dart"  => FileTypeConfig::with(Brace, &["c-style"]),
        "groovy"=> FileTypeConfig::with(Brace, &["c-style"]),
        "gradle"=> FileTypeConfig::with(Brace, &["c-style"]),
        "mm"    => FileTypeConfig::with(Brace, &["c-style"]),
        "pas"   => FileTypeConfig::with(Code, &["pascal"]),
        "adb"   => FileTypeConfig::with(Code, &["double-dash"]),
        "ads"   => FileTypeConfig::with(Code, &["double-dash"]),
        "f"     => FileTypeConfig::with(Code, &["bang"]),
        "for"   => FileTypeConfig::with(Code, &["bang"]),
        "f90"   => FileTypeConfig::with(Code, &["bang"]),
        "f95"   => FileTypeConfig::with(Code, &["bang"]),
        "f03"   => FileTypeConfig::with(Code, &["bang"]),
        "f08"   => FileTypeConfig::with(Code, &["bang"]),
        "v"     => FileTypeConfig::with(Code, &["c-style"]),
        "jl"    => FileTypeConfig::with(Code, &["hash"]),
        "nix"   => FileTypeConfig::with(Code, &["hash","c-style"]),
        "php"   => FileTypeConfig::with(Code, &["c-style","hash"]),
        "rb"    => FileTypeConfig::with(Code, &["hash"]),
        "rake"  => FileTypeConfig::with(Code, &["hash"]),
        "gemspec" => FileTypeConfig::with(Code, &["hash"]),
        "ru"    => FileTypeConfig::with(Code, &["hash"]),
        "perl"  => FileTypeConfig::with(Code, &["hash"]),
        "sh"    => FileTypeConfig::with(Code, &["hash"]),
        "bash"  => FileTypeConfig::with(Code, &["hash"]),
        "zsh"   => FileTypeConfig::with(Code, &["hash"]),
        "fish"  => FileTypeConfig::with(Code, &["hash"]),
        "ps1"   => FileTypeConfig::with(Code, &["powershell"]),
        "psm1"  => FileTypeConfig::with(Code, &["powershell"]),
        "psd1"  => FileTypeConfig::with(Code, &["powershell"]),
        "sql"   => FileTypeConfig::with(Code, &["sql"]),
        "tsql"  => FileTypeConfig::with(Code, &["sql"]),
        "plsql" => FileTypeConfig::with(Code, &["sql"]),
        "r"     => FileTypeConfig::with(Code,   &["hash"]),
        "hbs"   => FileTypeConfig::with(Code,   &["template"]),
        "handlebars" => FileTypeConfig::with(Code, &["template"]),
        "ejs"   => FileTypeConfig::with(Code,   &["template"]),
        "pug"   => FileTypeConfig::with(Code, &["c-style"]),
        "jade"  => FileTypeConfig::with(Code, &["c-style"]),
        "mustache" => FileTypeConfig::with(Code,&["template"]),
        "twig"  => FileTypeConfig::with(Code,   &["template"]),
        "jinja" => FileTypeConfig::with(Code,   &["template"]),
        "jinja2"=> FileTypeConfig::with(Code,   &["template"]),
        "erb"   => FileTypeConfig::with(Code,   &["template"]),
        "vue"   => FileTypeConfig::with(Code,   &["html"]),
        "svelte"=> FileTypeConfig::with(Code,   &["html"]),
        "xsl"   => FileTypeConfig::with(Code,   &["html"]),
        "xslt"  => FileTypeConfig::with(Code,   &["html"]),
        "graphql" => FileTypeConfig::with(Code,&["hash"]),
        "gql"   => FileTypeConfig::with(Code, &["hash"]),
        "proto" => FileTypeConfig::with(Brace, &["c-style"]),
        "csv"   => FileTypeConfig::new(Code),
        "ini"   => FileTypeConfig::with(Code, &["hash","semicolon"]),
        "conf"  => FileTypeConfig::with(Code, &["hash"]),
        "config"=> FileTypeConfig::with(Code, &["hash"]),
        "env"   => FileTypeConfig::with(Code, &["hash"]),
        "properties" => FileTypeConfig::with(Code,&["hash"]),
        "tf"    => FileTypeConfig::with(Brace, &["hash","c-style"]),
        "hcl"   => FileTypeConfig::with(Brace, &["hash","c-style"]),
        "tfvars"=> FileTypeConfig::with(Brace, &["hash","c-style"]),
        "pp"    => FileTypeConfig::with(Code, &["hash"]),
        "md"    => FileTypeConfig::new(Markdown),
        "markdown" => FileTypeConfig::new(Markdown),
        "mdx"   => FileTypeConfig::new(Markdown),
        "rst"   => FileTypeConfig::with(Code, &["hash"]),
        "star"  => FileTypeConfig::with(Code, &["hash"]),
        "bzl"   => FileTypeConfig::with(Code, &["hash"]),
        "cmake" => FileTypeConfig::with(Code, &["hash"]),
        "awk"   => FileTypeConfig::with(Code, &["hash"]),
        "pl"    => FileTypeConfig::with(Code,   &["hash"]),
        "pm"    => FileTypeConfig::with(Code,   &["hash"]),
        "fs"    => FileTypeConfig::with(Code, &["c-style","fsharp-block"]),
        "fsx"   => FileTypeConfig::with(Code, &["c-style","fsharp-block"]),
        // OCaml uses (* ... *) nested block comments — same syntax as F# fsharp-block
        "ml"    => FileTypeConfig::with(Code, &["fsharp-block"]),
        "mli"   => FileTypeConfig::with(Code, &["fsharp-block"]),
        "hs"    => FileTypeConfig::with(Code, &["haskell"]),
        "lhs"   => FileTypeConfig::with(Code, &["haskell"]),
        "elm"   => FileTypeConfig::with(Code, &["c-style"]),
        "lisp"  => FileTypeConfig::with(Code, &["semicolon"]),
        "lsp"   => FileTypeConfig::with(Code, &["semicolon"]),
        "scm"   => FileTypeConfig::with(Code, &["semicolon"]),
        "rkt"   => FileTypeConfig::with(Code, &["semicolon"]),
        "clj"   => FileTypeConfig::with(Code,   &["clojure"]),
        "cljs"  => FileTypeConfig::with(Code,   &["clojure"]),
        "ex"    => FileTypeConfig::with(Code,   &["hash"]),
        "exs"   => FileTypeConfig::with(Code,   &["hash"]),
        "erl"   => FileTypeConfig::with(Code,   &["percent"]),
        "hrl"   => FileTypeConfig::with(Code,   &["percent"]),
        "vhd"   => FileTypeConfig::with(Code, &["double-dash"]),
        "vhdl"  => FileTypeConfig::with(Code, &["double-dash"]),
        "asm"   => FileTypeConfig::with(Code, &["semicolon"]),
        "nasm"  => FileTypeConfig::with(Code, &["semicolon"]),
        "wat"   => FileTypeConfig::with(Code, &["wasm-text"]),
        "wast"  => FileTypeConfig::with(Code, &["wasm-text"]),
        "txt"   => FileTypeConfig::new(General),
        "log"   => FileTypeConfig::new(General),
        "cfg"   => FileTypeConfig::with(Code, &["hash"]),
        "gitignore"   => FileTypeConfig::with(Code,&["hash"]),
        "dockerignore"=> FileTypeConfig::with(Code,&["hash"]),
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
