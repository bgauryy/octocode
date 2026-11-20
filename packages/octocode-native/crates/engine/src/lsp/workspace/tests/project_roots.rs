use super::*;

/// A C-family file roots where clangd reads its command, not at a module's
/// own build file: a subdirectory's `Makefile` would hide the project's other
/// files from the server, and misresolve a relative `--compile-commands-dir`.
#[test]
fn a_c_family_file_roots_at_its_project_not_a_module_marker() {
    let root = |dir: &std::path::Path, file: &str| {
        resolve_workspace_root_for_file(dir.join(file).to_string_lossy().into_owned())
            .expect("root")
    };
    let write = |dir: &std::path::Path, file: &str| {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, "").expect("write");
    };
    let text = |dir: &std::path::Path| dir.to_string_lossy().into_owned();

    // The repository, where an out-of-tree build directory lives.
    let repo = TmpDir::new();
    std::fs::create_dir_all(repo.join(".git")).expect("git");
    write(&repo, "src/lib/Makefile");
    write(&repo, "src/lib/lib.c");
    write(&repo, "build/debug/compile_commands.json");
    assert_eq!(root(&repo, "src/lib/lib.c"), text(&repo));

    // The nearest database wins over the repository, as in clangd.
    write(&repo, "vendor/x/compile_commands.json");
    write(&repo, "vendor/x/y/z.cpp");
    assert_eq!(
        root(&repo, "vendor/x/y/z.cpp"),
        text(&repo.join("vendor/x"))
    );

    // No repository: the top of the unbroken chain of build files.
    let tree = TmpDir::new();
    write(&tree, "CMakeLists.txt");
    write(&tree, "lib/CMakeLists.txt");
    write(&tree, "lib/a.c");
    assert_eq!(root(&tree, "lib/a.c"), text(&tree));

    // A generic build file names no Python project; the directory it imports
    // from does (the repository once that holds Python code of its own),
    // until the language's own project file is nearer.
    write(&repo, "tools/Makefile");
    write(&repo, "tools/a.py");
    write(&repo, "tools/sub/Makefile");
    write(&repo, "tools/sub/b.py");
    assert_eq!(root(&repo, "tools/sub/b.py"), text(&repo.join("tools")));
    write(&repo, "setup_env.py");
    assert_eq!(root(&repo, "tools/a.py"), text(&repo));
    std::fs::remove_file(repo.join("setup_env.py")).expect("remove");
    write(&repo, "tools/pyproject.toml");
    assert_eq!(root(&repo, "tools/a.py"), text(&repo.join("tools")));
    // A weaker tier never outranks a stronger project above it.
    write(&repo, "svc/pyproject.toml");
    write(&repo, "svc/app/requirements.txt");
    write(&repo, "svc/app/m.py");
    assert_eq!(root(&repo, "svc/app/m.py"), text(&repo.join("svc")));
    // A project named by suffix, and the solution above its projects.
    write(&repo, "net/All.sln");
    write(&repo, "net/Core/Core.csproj");
    write(&repo, "net/Core/Lib.cs");
    assert_eq!(root(&repo, "net/Core/Lib.cs"), text(&repo.join("net")));
    // A project file above the repository belongs to something else.
    let outer = TmpDir::new();
    write(&outer, "package.json");
    write(&outer, "foo.sln");
    write(&outer, "compile_commands.json");
    std::fs::create_dir_all(outer.join("repo/.git")).expect("git");
    for file in ["repo/tools/a.js", "repo/tools/a.cs", "repo/tools/a.c"] {
        write(&outer, file);
        assert_eq!(root(&outer, file), text(&outer.join("repo")), "{file}");
    }
    // Nor do the widening configs reach above it.
    write(&outer, "tsconfig.json");
    write(&outer, "go.work");
    write(&outer, "pyrightconfig.json");
    for file in ["repo/m/a.ts", "repo/m/a.go"] {
        write(&outer, file);
        assert_eq!(root(&outer, file), text(&outer.join("repo")), "{file}");
    }
    // A Python file roots at the directory it imports from, not the config.
    write(&outer, "repo/m/a.py");
    assert_eq!(root(&outer, "repo/m/a.py"), text(&outer.join("repo/m")));
    // No project file and no repository: the nearest marker stands.
    let bare = TmpDir::new();
    write(&bare, "pkg/Makefile");
    write(&bare, "pkg/a.go");
    assert_eq!(root(&bare, "pkg/a.go"), text(&bare.join("pkg")));
}
