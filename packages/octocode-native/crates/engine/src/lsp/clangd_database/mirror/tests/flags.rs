//! Which flags and file names keep an entry in the copy.
use super::*;

/// Each spelling clang accepts of an option that reads a file, and each word that
/// hides further flags, takes the entry out. The last ones are the same options
/// reading a file the project owns, which is no reason to leave.
#[test]
fn every_spelling_of_a_flag_reading_outside_the_project_is_left_out() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let real = root.canonicalize().expect("real");
    let dir = real.to_string_lossy();
    let outside = "/elsewhere/x.h";
    let refused: Vec<Vec<String>> = [
        vec!["--imacros", outside],
        vec!["--imacros=/elsewhere/x.h"],
        vec!["-imacros", outside],
        vec!["-imacros/elsewhere/x.h"],
        vec!["--include", outside],
        vec!["--include=/elsewhere/x.h"],
        vec!["-include", outside],
        vec!["-include/elsewhere/x.h"],
        vec!["-include", "../up.h"],
        vec!["-include../up.h"],
        vec!["-include", ".hidden.h"],
        vec!["-include-pch", "/elsewhere/x.h.pch"],
        vec!["-include-pch", ".hidden.pch"],
        vec!["-include-pch", "../x.h.pch"],
        vec!["--config", outside],
        vec!["--config=/elsewhere/flags.cfg"],
        vec!["--config=own.cfg"],
        vec!["--config", "own.cfg"],
        vec!["@own.rsp"],
        vec!["@/elsewhere/own.rsp"],
        vec!["-ivfsoverlay", "/elsewhere/o.yaml"],
        vec!["-ivfsoverlay/elsewhere/o.yaml"],
        vec!["-fmodule-map-file=/elsewhere/m.modulemap"],
        vec!["-fmodule-file=m=/elsewhere/m.pcm"],
        vec!["-fprofile-use=/elsewhere/p.prof"],
        vec!["-fsanitize-ignorelist=/elsewhere/i.txt"],
        vec!["-fplugin=/elsewhere/p.so"],
        vec!["-fbasic-block-sections=list=/elsewhere/b.txt"],
        vec!["-Xclang", "-load", "-Xclang", "/elsewhere/p.so"],
        vec!["-Xclang", "-include", "-Xclang", outside],
        vec!["-Xclang=-include=/elsewhere/x.h"],
        vec!["-Xpreprocessor", "-include", "-Xpreprocessor", outside],
        vec!["-Wp,-include,/elsewhere/x.h"],
        vec!["-unknown-reader", outside],
        vec!["--unknown-reader=../x.h"],
        // A flag that takes no separate value does not hide the next word.
        vec!["-W", "--imacros", outside],
        vec!["--gcc-toolchain=/opt/gcc", "-include/elsewhere/x.h"],
        vec!["--gcc-toolchain", "-include/elsewhere/x.h"],
    ]
    .into_iter()
    .map(|words| words.into_iter().map(str::to_owned).collect())
    .collect();
    let mut database = refused
        .iter()
        .enumerate()
        .map(|(index, words)| {
            let mut arguments = vec!["clang".to_owned(), "-c".to_owned()];
            arguments.extend(words.iter().cloned());
            json!({"directory": dir, "file": format!("r{index}.c"), "arguments": arguments})
        })
        .collect::<Vec<_>>();
    database.extend([
        json!({"directory": dir, "file": "own1.c", "arguments": ["clang", "--imacros", "own.h", "own1.c"]}),
        json!({"directory": dir, "file": "own2.c", "arguments": ["clang", "-include-pch", "own.h.pch", "own2.c"]}),
        json!({"directory": dir, "file": "own3.c", "arguments": ["clang", format!("-fprofile-use={dir}/own.prof"), "own3.c"]}),
        json!({"directory": dir, "file": "own4.c", "command": "clang --include=sub/own.h own4.c"}),
    ]);
    let (_, entries) = mirror_of(&root, &home, &json!(database));
    let refused_left = kept_files(&entries)
        .into_iter()
        .filter(|file| file.starts_with('r'))
        .map(|file| {
            let index: usize = file[1..file.len() - 2].parse().expect("index");
            refused[index].join(" ")
        })
        .collect::<Vec<_>>();
    assert!(refused_left.is_empty(), "still mirrored: {refused_left:?}");
    assert_eq!(
        kept_files(&entries),
        ["own1.c", "own2.c", "own3.c", "own4.c"]
    );
}

/// Compile commands with no file of their own to read.
const ORDINARY_COMMANDS: [&str; 18] = [
    "clang -I/usr/include -I /opt/include -std=c++17",
    "clang -isystem /opt/sdk/include -isystem/opt/x -iquote /q",
    "clang -idirafter /d -iframework /f -F/f -F /g -L/l",
    "clang -isysroot /SDKs/MacOSX.sdk --sysroot=/SDKs/x --sysroot /s",
    "clang --gcc-toolchain=/opt/gcc --gcc-install-dir=/opt/gcc/13",
    "clang -resource-dir=/r -resource-dir /r -B/usr/bin -B /b",
    "clang --include-directory=/i --include-directory /i",
    "clang -DDATADIR=\"/usr/share\" -D PREFIX=\"/usr/local\" -UX -U Y",
    "clang -o /elsewhere/a.o -MF /elsewhere/a.d -MT /elsewhere/a.o",
    "clang -MD -MMD -MP -MJ /elsewhere/a.json -MQ /elsewhere/a.o",
    "clang -ffile-prefix-map=/build=. -fdebug-prefix-map=/build=.",
    "clang -fmacro-prefix-map=/build=. -fdebug-compilation-dir=/build",
    "clang -Wl,-rpath,/opt/lib -Wa,-I/opt/asm -Wall -Wno-unused -W",
    "clang -x c -arch arm64 -target arm64-apple-macos11",
    "clang -fsanitize=address,undefined -fPIC -O2 -g -pthread",
    "clang -march=armv8-a+crc -mmacosx-version-min=11 -framework Metal",
    "/usr/bin/ccache /usr/bin/clang -c",
    "/usr/local/bin/sccache /opt/llvm/bin/clang++ -std=gnu++20",
];

/// Ordinary flags cost no coverage: search paths, outputs and macro values can
/// point anywhere, and a compiler launcher or an out-of-tree build is normal.
#[test]
fn ordinary_flags_keep_their_entries() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let build = TmpDir::new();
    let real = root.canonicalize().expect("real");
    let dir = real.to_string_lossy();
    let out_of_tree = build.canonicalize().expect("real");
    let out_of_tree = out_of_tree.to_string_lossy();
    let source = |name: &str| format!("{dir}/{name}");
    let kept = ORDINARY_COMMANDS
        .iter()
        .map(|command| command.split(' ').map(str::to_owned).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut database = kept
        .iter()
        .enumerate()
        .map(|(index, words)| {
            let name = format!("k{index}.c");
            let mut arguments = words.clone();
            arguments.push(name.clone());
            json!({"directory": dir, "file": name, "arguments": arguments})
        })
        .collect::<Vec<_>>();
    // Built out of tree: relative values resolve under the build folder.
    database.push(json!({
        "directory": out_of_tree, "file": source("tree.c"),
        "command": format!("/usr/bin/cc -x c -arch arm64 -fsanitize=address -o CMakeFiles/t.dir/tree.c.o -MF CMakeFiles/t.d -I{out_of_tree}/generated -c {}", source("tree.c")),
    }));
    // A program path that merely spells the name is no driver mode.
    database.push(json!({
        "directory": dir, "file": "named.c", "arguments": ["clang", "-I/opt/clang-cl-tests/include", "named.c"],
    }));
    let (_, entries) = mirror_of(&root, &home, &json!(database));
    let names = kept_files(&entries);
    let lost = database
        .iter()
        .filter(|entry| {
            !names
                .iter()
                .any(|name| name == entry["file"].as_str().unwrap_or_default())
        })
        .map(|entry| format!("{}", entry["arguments"]))
        .collect::<Vec<_>>();
    assert!(lost.is_empty(), "lost: {lost:?}");
}

/// Every extension routed to clangd keeps its entry, whatever else names them, and
/// so do the spellings clang compiles that no language route uses.
#[test]
fn every_clangd_extension_keeps_its_entry() {
    let root = TmpDir::new();
    let home = TmpDir::new();
    let real = root.canonicalize().expect("real");
    let dir = real.to_string_lossy();
    let routed = crate::lsp::config::ROUTED_EXTENSIONS
        .iter()
        .filter(|extension| {
            crate::lsp::family::is_c_family(
                crate::lsp::config::detect_language_id(&format!("x{extension}")).as_deref(),
            )
        })
        .map(|extension| format!("r{extension}"))
        .collect::<Vec<_>>();
    assert!(routed.contains(&"r.hh".to_owned()), "{routed:?}");
    assert!(routed.contains(&"r.hxx".to_owned()), "{routed:?}");
    assert!(routed.contains(&"r.cuh".to_owned()), "{routed:?}");
    let names = routed
        .iter()
        .map(String::as_str)
        .chain(["u.c++", "u.m", "u.mm", "u.C", "u.HPP"])
        .collect::<Vec<_>>();
    let database = names
        .iter()
        .map(|name| json!({"directory": dir, "file": name, "arguments": ["clang", "-c", name]}))
        .chain([
            json!({"directory": dir, "file": "prod.env", "arguments": ["clang", "-x", "c", "prod.env"]}),
            json!({"directory": dir, "file": "server.pem", "arguments": ["clang", "-x", "c", "server.pem"]}),
        ])
        .collect::<Vec<_>>();
    let (_, entries) = mirror_of(&root, &home, &json!(database));
    assert_eq!(kept_files(&entries), names);
}
