//! The route of a file's language server (the program, arguments and source
//! discovery resolves, before the per-server defaults every launch gets),
//! kept per workspace root and extension. Resolving it reads `PATH`
//! (`which`), the `lsp-servers.json` files, the managed installs and the
//! toolchain state of the language, and every lead and `lspSearch` request
//! asks; the kept route is served while the stamps of those reads hold
//! ([`inputs_of`], [`searched`]) and resolved again as soon as one differs. A route with no
//! server (an extension nothing routes) is kept the same way.
use super::catalog::spec_for_extension;
use super::defaults::{apply_rust_analyzer_defaults, apply_server_default_options};
use super::invocation::rustup::{self, RustupView};
use super::invocation::{PYRIGHT_FAMILY, is_typescript_server_command};
use super::kept::{Kept, follow_path_search, kept};
use super::types::{LspDiscoveryOptions, ServerSpec};
use super::user_config::UserServer;
use crate::lsp::commands::has_path_separator;
use crate::lsp::types::LspLanguageServerConfig;
use crate::stamp::Proof;
use octocode_io::memo::Memo;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What a launch adds to its route.
#[derive(Clone, Copy, Debug)]
enum Finish {
    /// Nothing: an assembly entry launches exactly as the user wrote it.
    AsIs,
    /// The per-server defaults by command, and the tsserver pin.
    Defaults,
    /// rust-analyzer's defaults, whatever the managed file is named.
    RustAnalyzer,
}

/// What resolving a route looked for outside the files [`inputs_of`] stamps,
/// by the program it was asked to resolve: stamped after the answer, from
/// the route itself, so a user entry that names a server stamps what that
/// server's lookup reads, whatever its extension.
#[derive(Clone, Debug)]
struct Lookups {
    /// The programs looked up on `PATH`; with exactly one, the directories
    /// after the one holding it need no stamp.
    programs: Vec<String>,
    /// The TypeScript server's lookups: `node_modules` of each directory it
    /// is searched from (the CLI, the workspace's `tsgo`).
    node_modules: bool,
    /// The Python user script directories (`pip install --user`).
    python_scripts: bool,
}

impl Lookups {
    /// The lookups of resolving `command`, as a server invocation does.
    fn of(command: &str) -> Self {
        let typescript = is_typescript_server_command(command);
        let mut programs = vec![command.to_owned()];
        if typescript {
            programs.push("node".to_owned());
        }
        Self {
            programs,
            node_modules: typescript,
            python_scripts: command == "pylsp",
        }
    }

    /// The lookups of the built-in route of `spec`, which Python extends with
    /// its preference order.
    fn built_in(spec: &ServerSpec) -> Self {
        let mut lookups = Self::of(spec.command);
        if spec.language_id == "python" {
            lookups.python_scripts = true;
            lookups
                .programs
                .extend(PYRIGHT_FAMILY.iter().map(|name| (*name).to_owned()));
        }
        lookups
    }
}

/// A resolved route and what each launch adds to it.
#[derive(Clone, Debug)]
pub(super) struct Route {
    config: LspLanguageServerConfig,
    finish: Finish,
    lookups: Lookups,
    /// Whether every question resolution asked got an answer. A route built on
    /// an unanswered one (a rustup probe its caller stopped) is the caller's,
    /// not kept: the next call asks again.
    answered: bool,
}

impl Route {
    /// An entry launched exactly as written.
    pub(super) fn as_is(user: UserServer) -> Self {
        Self {
            lookups: Lookups::of(&user.command),
            config: user.config,
            finish: Finish::AsIs,
            answered: true,
        }
    }

    /// A user entry, which gets the defaults of the server it names.
    pub(super) fn user(user: UserServer) -> Self {
        Self {
            finish: Finish::Defaults,
            ..Self::as_is(user)
        }
    }

    /// The built-in route of `spec`; `answered` is false when a question its
    /// resolution asked got no answer.
    pub(super) fn built_in(
        spec: &ServerSpec,
        config: LspLanguageServerConfig,
        answered: bool,
    ) -> Self {
        Self {
            config,
            finish: if spec.language_id == "rust" {
                Finish::RustAnalyzer
            } else {
                Finish::Defaults
            },
            lookups: Lookups::built_in(spec),
            answered,
        }
    }

    /// The launch configuration: the route with its defaults, applied on every
    /// call (a clangd database mirror follows its source).
    pub(super) fn finished(&self, options: &LspDiscoveryOptions) -> LspLanguageServerConfig {
        let mut config = self.config.clone();
        match self.finish {
            Finish::AsIs => {}
            Finish::RustAnalyzer => apply_rust_analyzer_defaults(&mut config),
            Finish::Defaults => {
                apply_server_default_options(
                    &mut config,
                    options.octocode_home.as_deref(),
                    options.index_home(),
                );
                super::invocation::pin_tsserver(&mut config, options);
            }
        }
        config
    }
}

/// Everything a route is an answer to besides the files [`inputs_of`] stamps:
/// the request's place and options, and the process variables resolution reads.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Question {
    workspace_root: String,
    extension: String,
    native_typescript: bool,
    anchor: Option<PathBuf>,
    trust_project: bool,
    octocode_home: Option<PathBuf>,
    invocation_dir: Option<PathBuf>,
    path_var: Option<OsString>,
    home: Option<PathBuf>,
    rustup_path: Option<OsString>,
    rustup_home: Option<PathBuf>,
    rustup_toolchain: Option<OsString>,
}

static ROUTES: Memo<Question, Kept<Option<Route>>> = Memo::new(&crate::memory::MEMORY);

/// The route for `extension` in `workspace_root`: kept while its inputs hold,
/// else `resolve()`.
pub(super) fn kept_route(
    extension: &str,
    workspace_root: &str,
    options: &LspDiscoveryOptions,
    anchor: Option<&Path>,
    native_typescript: bool,
    resolve: impl FnOnce() -> Option<Route>,
) -> Option<Route> {
    let view = RustupView::current();
    let question = Question {
        workspace_root: workspace_root.to_owned(),
        extension: extension.to_owned(),
        native_typescript,
        anchor: anchor.map(Path::to_path_buf),
        trust_project: options.trusts_project(),
        octocode_home: options.octocode_home.clone(),
        invocation_dir: options.invocation_dir.clone(),
        path_var: crate::process_env::var_os("PATH"),
        home: std::env::home_dir(),
        rustup_path: view.path_var.clone(),
        rustup_home: view.home.clone(),
        rustup_toolchain: view.toolchain.clone(),
    };
    let inputs = || inputs_of(&question, &view, options);
    kept(&ROUTES, question.clone(), inputs, resolve, |route| {
        searched(&question, route.as_ref())
    })
}

/// What a resolved `route` (or none) looked for: the `PATH` directories as far
/// as the search went, each looked-up program's file in them, an explicit
/// program path, and by the route's own lookups the Python user script
/// directories and the `node_modules` (and its `.bin` and `@typescript`) of
/// each directory a TypeScript server is looked for from. `None` for a route
/// built on an unanswered question.
fn searched(question: &Question, route: Option<&Route>) -> Option<Proof> {
    if route.is_some_and(|route| !route.answered) {
        return None;
    }
    let lookups = route.map(|route| &route.lookups);
    let programs = lookups
        .into_iter()
        .flat_map(|lookups| lookups.programs.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let mut inputs = Proof::default();
    let cwd = Path::new(&question.workspace_root);
    let mut path_vars = vec![&question.path_var];
    if question.rustup_path != question.path_var {
        path_vars.push(&question.rustup_path);
    }
    for path_var in path_vars {
        let found = match programs.as_slice() {
            [program] => which::which_in(program, path_var.as_ref(), cwd).ok(),
            _ => None,
        };
        follow_path_search(&mut inputs, path_var.as_deref(), &programs, found.as_deref());
    }
    for program in programs.iter().filter(|program| has_path_separator(program)) {
        inputs.follow(PathBuf::from(program));
    }
    if let Some(lookups) = lookups {
        if lookups.python_scripts {
            follow_python_scripts(&mut inputs, question);
        }
        if lookups.node_modules {
            follow_node_modules(&mut inputs, question);
        }
    }
    Some(inputs)
}

/// Stamp the Python user script directories under the user's home.
fn follow_python_scripts(inputs: &mut Proof, question: &Question) {
    if let Some(home) = &question.home {
        inputs.follow(home.join("Library/Python"));
        for dir in super::invocation::python_user_script_dirs(home) {
            inputs.follow(dir);
        }
    }
}

/// Stamp `node_modules`, `.bin` and `@typescript` of each directory a
/// TypeScript server is looked for from, and each of their ancestors.
fn follow_node_modules(inputs: &mut Proof, question: &Question) {
    let starts = [
        question.anchor.as_deref().and_then(Path::parent),
        Some(Path::new(&question.workspace_root)),
        question.invocation_dir.as_deref(),
    ];
    let dirs = starts
        .into_iter()
        .flatten()
        .flat_map(Path::ancestors)
        .collect::<BTreeSet<_>>();
    for dir in dirs {
        let modules = dir.join("node_modules");
        inputs.follow(modules.join(".bin"));
        inputs.follow(modules.join("@typescript"));
        inputs.follow(modules);
    }
}

/// What resolving the route of `question` reads besides what [`searched`]
/// stamps after it: the `lsp-servers.json` files, the managed install root and
/// the spec's managed binary with its completion marker, and for Rust the
/// rustup state that selects a toolchain component. `None` when the rustup
/// state cannot be named, as the rustup answer itself is then not kept.
fn inputs_of(
    question: &Question,
    view: &RustupView,
    options: &LspDiscoveryOptions,
) -> Option<Proof> {
    let mut inputs = Proof::default();
    for (config, _) in super::user_config::user_config_paths(&question.workspace_root, options) {
        inputs.follow(config);
    }
    let spec = spec_for_extension(&question.extension);
    if let Some(root) = options.managed_root() {
        inputs.follow(root.clone());
        if let Some(bin) = spec.as_ref().and_then(|spec| {
            crate::lsp::managed::cached_server_bin_path(
                &root,
                spec.command,
                &crate::lsp::managed::platform_id(),
            )
        }) {
            inputs.follow(crate::lsp::managed::marker_path(&bin));
            inputs.follow(bin);
        }
    }
    if spec.is_some_and(|spec| spec.language_id == "rust") {
        rustup::follow_selection(&mut inputs, view, Path::new(&question.workspace_root))?;
    }
    Some(inputs)
}
