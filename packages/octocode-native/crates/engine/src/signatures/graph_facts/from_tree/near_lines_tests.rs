//! A walk of only the nodes touching some lines finds the declarations named
//! on those lines exactly as the whole declarations-only outline holds them.
use super::declarations_on_lines;
use crate::graph::GraphFactDeclaration;
use crate::signatures::extract_declarations_document;
use crate::signatures::languages::all_entries;

/// The whole outline's declarations named on `lines`, in outline order;
/// `None` when no outline reads the file.
fn whole(source: &str, path: &str, lines: &[usize]) -> Option<Vec<GraphFactDeclaration>> {
    let outline = extract_declarations_document(source, path)?;
    Some(
        outline
            .declarations
            .into_iter()
            .filter(|declaration| {
                lines.contains(&(declaration.selection_range.start.line as usize))
            })
            .collect(),
    )
}

/// Line sets that probe a file: each line alone, each pair of neighbors,
/// every other line from each parity, and every line.
fn line_sets(source: &str) -> Vec<Vec<usize>> {
    let count = source.split('\n').count();
    let mut sets: Vec<Vec<usize>> = (0..count).map(|line| vec![line]).collect();
    sets.extend((1..count).map(|line| vec![line - 1, line]));
    sets.push((0..count).step_by(2).collect());
    sets.push((1..count).step_by(2).collect());
    sets.push((0..count).collect());
    sets
}

/// For every line set: the narrowed walk answers (`expect_narrow`) and names
/// what the whole outline names there, field for field; how many sets it
/// left to the whole outline.
fn agrees(source: &str, path: &str, expect_narrow: bool) -> usize {
    let mut whole_only = 0;
    for lines in line_sets(source) {
        let narrow = declarations_on_lines(source, path, &lines);
        match narrow {
            Some(narrow) => {
                let whole = whole(source, path, &lines).unwrap_or_default();
                assert_eq!(narrow, whole, "{path} lines {lines:?}");
            }
            None => {
                assert!(
                    !expect_narrow,
                    "{path} lines {lines:?}: left to the whole outline"
                );
                whole_only += 1;
            }
        }
    }
    whole_only
}

/// A source per grammar with nested types, members, attributes, comments and
/// headers that start before the name line. A registry entry without one
/// fails here instead of going untested.
fn fixture(ext: &str) -> &'static str {
    match ext {
        "ts" => {
            "export class Box<T> {\n  constructor(private value: T) {}\n  get(): T { return this.value; }\n}\nexport function target(value: number): number {\n  return value;\n}\n"
        }
        "tsx" => "export function target(value: number) {\n  return <div>{value}</div>;\n}\n",
        "js" => "export function target(value) {\n  return value;\n}\nclass A { m() {} }\n",
        "py" => concat!(
            "import os\n",
            "\n",
            "@decorator\n",
            "def target(value):\n",
            "    \"\"\"Doc.\"\"\"\n",
            "    return helper(value)\n",
            "\n",
            "class Box(Base):\n",
            "    size = 1\n",
            "    size = 2\n",
            "    label: str = \"x\"\n",
            "\n",
            "    def __init__(self, value):\n",
            "        self.value = value\n",
            "        self.count = 0\n",
            "\n",
            "    def grow(self):\n",
            "        self.count = self.count + 1\n",
            "        self.extra = 1\n",
            "\n",
            "    @property\n",
            "    def shown(self):\n",
            "        self.extra = 2\n",
            "        return self.value\n",
            "\n",
            "    class Inner:\n",
            "        depth = 3\n",
            "        def walk(self):\n",
            "            self.depth = 4\n",
            "\n",
            "async def fetch(url,\n",
            "                timeout=1):\n",
            "    pass\n",
            "\n",
            "LIMIT = 10\n",
        ),
        "go" => concat!(
            "package fixture\n",
            "\n",
            "import \"fmt\"\n",
            "\n",
            "const (\n",
            "\tFirst = iota\n",
            "\tSecond\n",
            ")\n",
            "\n",
            "var limit, other = 1, 2\n",
            "\n",
            "// Box holds a value.\n",
            "type Box struct {\n",
            "\tvalue int\n",
            "\tLabel string\n",
            "}\n",
            "\n",
            "type Shape interface {\n",
            "\tArea() float64\n",
            "}\n",
            "\n",
            "func (b *Box) Grow(by int) int {\n",
            "\tb.value += by\n",
            "\treturn b.value\n",
            "}\n",
            "\n",
            "func target(value int) int {\n",
            "\tfmt.Println(value)\n",
            "\treturn value\n",
            "}\n",
        ),
        "rs" => concat!(
            "//! Crate doc.\n",
            "use std::fmt;\n",
            "\n",
            "/// A box.\n",
            "#[derive(Debug,\n",
            "    Clone)]\n",
            "pub struct Box<T> {\n",
            "    pub value: T,\n",
            "    count: usize,\n",
            "}\n",
            "\n",
            "pub(crate) enum Shape {\n",
            "    Circle { radius: f64 },\n",
            "    Square(f64),\n",
            "}\n",
            "\n",
            "pub trait Area {\n",
            "    fn area(&self) -> f64;\n",
            "    const SIDES: u32 = 0;\n",
            "}\n",
            "\n",
            "impl<T: fmt::Debug> Box<T> {\n",
            "    pub fn new(value: T) -> Self {\n",
            "        Self { value, count: 0 }\n",
            "    }\n",
            "\n",
            "    fn grow(&mut self) -> Result<usize, String> {\n",
            "        self.count += 1;\n",
            "        Ok(self.count)\n",
            "    }\n",
            "}\n",
            "\n",
            "mod inner {\n",
            "    pub static LIMIT: u32 = 10;\n",
            "    pub type Alias = u32;\n",
            "    pub fn target(value: u32) -> u32 { value }\n",
            "}\n",
            "\n",
            "macro_rules! twice {\n",
            "    ($e:expr) => { $e * 2 };\n",
            "}\n",
            "\n",
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    #[test]\n",
            "    fn grows() {}\n",
            "}\n",
        ),
        "java" => concat!(
            "package fixture;\n",
            "\n",
            "import java.util.List;\n",
            "\n",
            "/** A box. */\n",
            "@Deprecated\n",
            "public class Box<T> {\n",
            "  private final T value;\n",
            "  public static int count = 0, other = 1;\n",
            "\n",
            "  public Box(T value) {\n",
            "    this.value = value;\n",
            "  }\n",
            "\n",
            "  @Override\n",
            "  public String toString() {\n",
            "    return String.valueOf(value);\n",
            "  }\n",
            "\n",
            "  static class Inner {\n",
            "    void walk() {}\n",
            "  }\n",
            "}\n",
            "\n",
            "interface Shape {\n",
            "  double area();\n",
            "}\n",
            "\n",
            "enum Color { RED, GREEN }\n",
            "\n",
            "record Point(int x, int y) {\n",
            "  int target(int value) { return value; }\n",
            "}\n",
        ),
        "c" => concat!(
            "#include <stdio.h>\n",
            "\n",
            "#define LIMIT 10\n",
            "\n",
            "struct box {\n",
            "  int value;\n",
            "  int count;\n",
            "};\n",
            "\n",
            "typedef struct box box_t;\n",
            "\n",
            "static int\n",
            "grow(struct box *b, int by);\n",
            "\n",
            "static int\n",
            "grow(struct box *b, int by)\n",
            "{\n",
            "  b->value += by;\n",
            "  return b->value;\n",
            "}\n",
            "\n",
            "#if 0\n",
            "int disabled(void) { return 0; }\n",
            "#endif\n",
            "\n",
            "enum color { RED, GREEN };\n",
            "\n",
            "int target(int value) {\n",
            "  return grow(0, value);\n",
            "}\n",
        ),
        "cpp" => concat!(
            "#include <vector>\n",
            "\n",
            "namespace fixture {\n",
            "namespace al = std;\n",
            "\n",
            "template <typename T>\n",
            "class Box {\n",
            "public:\n",
            "  explicit Box(T value);\n",
            "  T get() const { return value_; }\n",
            "\n",
            "private:\n",
            "  T value_;\n",
            "  static int count_;\n",
            "};\n",
            "\n",
            "template <typename T>\n",
            "Box<T>::Box(T value) : value_(value) {}\n",
            "\n",
            "enum class Color { Red, Green };\n",
            "\n",
            "struct Point { int x, y; };\n",
            "\n",
            "int target(int value) {\n",
            "  return value;\n",
            "}\n",
            "}  // namespace fixture\n",
            "\n",
            "void fixture::outside() {}\n",
        ),
        "cs" => concat!(
            "using System;\n",
            "\n",
            "namespace Fixture\n",
            "{\n",
            "    /// <summary>A box.</summary>\n",
            "    [Serializable]\n",
            "    public class Box<T>\n",
            "    {\n",
            "        private readonly T value;\n",
            "        public int Count { get; set; }\n",
            "\n",
            "        public Box(T value)\n",
            "        {\n",
            "            this.value = value;\n",
            "        }\n",
            "\n",
            "        public int target(int value)\n",
            "        {\n",
            "            return value;\n",
            "        }\n",
            "    }\n",
            "\n",
            "    public interface IShape { double Area(); }\n",
            "\n",
            "    public enum Color { Red, Green }\n",
            "\n",
            "    public record Point(int X, int Y);\n",
            "}\n",
        ),
        "scala" => concat!(
            "package fixture\n",
            "\n",
            "trait Shape {\n",
            "  def area: Double\n",
            "}\n",
            "\n",
            "case class Point(x: Int, y: Int) extends Shape {\n",
            "  def area: Double = 0.0\n",
            "}\n",
            "\n",
            "object Fixture {\n",
            "  val limit = 10\n",
            "  var count = 0\n",
            "\n",
            "  def target(value: Int): Int = {\n",
            "    val local = value\n",
            "    local\n",
            "  }\n",
            "\n",
            "  class Inner {\n",
            "    def walk(): Unit = ()\n",
            "  }\n",
            "}\n",
        ),
        "cu" => concat!(
            "#include <cuda.h>\n",
            "\n",
            "struct Box { int value; };\n",
            "\n",
            "__device__ int helper(int value) { return value; }\n",
            "\n",
            "__global__ void target(int *value) {\n",
            "  *value = helper(*value);\n",
            "}\n",
            "\n",
            "void launch(int *value) {\n",
            "  target<<<1, 32>>>(value);\n",
            "}\n",
        ),
        "asm" => "target:\n  mov %rax, %rbx\n  call helper\n  ret\nhelper:\n  ret\n",
        _ => panic!("missing grammar fixture for .{ext}"),
    }
}

/// Every grammar, under every extension it goes by: oxc's files are left to
/// the whole outline; every other line set answers as the outline does.
#[test]
fn every_grammar_names_on_its_lines_what_its_whole_outline_names() {
    for entry in all_entries() {
        let source = fixture(entry.extensions[0]);
        for ext in entry.extensions {
            let path = format!("fixture.{ext}");
            let outline = extract_declarations_document(source, &path);
            let oxc = crate::text::JS_TS_EXTENSIONS.contains(ext);
            if oxc || outline.is_none() {
                for lines in line_sets(source) {
                    assert_eq!(declarations_on_lines(source, &path, &lines), None, "{path}");
                }
                continue;
            }
            assert!(
                outline.is_some_and(|outline| outline.declarations.len() > 1),
                "{path}: the fixture declares"
            );
            assert_eq!(agrees(source, &path, true), 0, "{path}");
        }
    }
}

/// A Python member is declared once per class, by its first declaration:
/// read alone, the second `size` or `self.extra` would declare it again.
#[test]
fn a_python_member_declared_before_the_lines_declares_nothing_on_them() {
    let source = fixture("py");
    let path = "members.py";
    let line = |text: &str| {
        source
            .lines()
            .position(|line| line.trim() == text)
            .expect("fixture line")
    };
    for text in ["size = 2", "self.extra = 2", "self.count = self.count + 1"] {
        assert_eq!(
            declarations_on_lines(source, path, &[line(text)]),
            Some(Vec::new()),
            "{text}"
        );
    }
    let first = declarations_on_lines(source, path, &[line("self.extra = 1")]).expect("narrow");
    assert_eq!(
        first.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
        ["extra"]
    );
}

/// The whole outline decides what a Rust item macro's body declares, and a
/// GoogleTest include decides what a C++ file's `TEST` declares; lines away
/// from the macro still read narrowly.
#[test]
fn a_macro_body_or_a_googletest_include_is_left_to_the_whole_outline() {
    let items = "pub fn before() {}\nmake_items! {\n    pub fn origin() {}\n}\npub fn after() {}\n";
    assert_eq!(declarations_on_lines(items, "items.rs", &[2]), None);
    assert_eq!(
        whole(items, "items.rs", &[2]).expect("outline")[0].name,
        "origin"
    );
    agrees(items, "items.rs", false);
    let narrow = declarations_on_lines(items, "items.rs", &[0, 4]).expect("narrow");
    assert_eq!(narrow, whole(items, "items.rs", &[0, 4]).expect("outline"));
    let gtest = "#include <gtest/gtest.h>\nTEST(Suite, Name) {\n}\nint plain() { return 0; }\n";
    assert_eq!(declarations_on_lines(gtest, "t.cpp", &[3]), None);
}

/// A path no grammar reads is left to the whole outline, which reads none
/// either; no lines name nothing.
#[test]
fn an_unread_path_is_left_to_the_whole_outline_and_no_lines_name_nothing() {
    assert_eq!(
        declarations_on_lines("plain text\n", "notes.txt", &[0]),
        None
    );
    assert_eq!(
        extract_declarations_document("plain text\n", "notes.txt"),
        None
    );
    assert_eq!(
        declarations_on_lines("fn a() {}\n", "a.rs", &[]),
        Some(Vec::new())
    );
}

/// Parity over the language battery's pinned clones (opt-in: they live outside
/// the repository): in each clone, the first [`FILES_PER_CLONE`] files (path
/// order) of a grammar this walk reads, at each line set: each tenth line,
/// the lines spelling the file's commonest identifier, and every line.
#[test]
#[ignore = "reads packages/octocode-benchmark/local/repos"]
fn battery_corpus_names_on_its_lines_what_its_whole_outline_names() {
    const FILES_PER_CLONE: usize = 250;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../octocode-benchmark/local/repos");
    let mut clones = std::fs::read_dir(&root)
        .expect("battery clones")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    clones.sort();
    let (mut files, mut narrow, mut whole_only) = (0_usize, 0_usize, 0_usize);
    for clone in clones {
        let mut read = 0;
        let mut stack = vec![clone];
        while let Some(dir) = stack.pop() {
            if read == FILES_PER_CLONE {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut paths = entries
                .flatten()
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths.into_iter().rev() {
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name != ".git") {
                        stack.push(path);
                    }
                    continue;
                }
                if read == FILES_PER_CLONE {
                    break;
                }
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let name = path.to_string_lossy().into_owned();
                if source.len() > 512 * 1024
                    || crate::signatures::graph_facts_grammar(&source, &name).is_none()
                {
                    continue;
                }
                read += 1;
                let count = source.split('\n').count();
                let common = commonest_identifier(&source);
                let sets = [
                    (0..count).step_by(10).collect::<Vec<_>>(),
                    source
                        .split('\n')
                        .enumerate()
                        .filter(|(_, line)| {
                            common.as_deref().is_some_and(|word| line.contains(word))
                        })
                        .map(|(at, _)| at)
                        .collect(),
                    (0..count).collect(),
                ];
                for lines in sets {
                    match declarations_on_lines(&source, &name, &lines) {
                        Some(found) => {
                            narrow += 1;
                            let expected = whole(&source, &name, &lines).unwrap_or_default();
                            assert_eq!(found, expected, "{name} lines {lines:?}");
                        }
                        None => whole_only += 1,
                    }
                }
            }
        }
        files += read;
    }
    eprintln!("battery parity: {files} files, {narrow} narrow line sets, {whole_only} whole-only");
    assert!(files > 0, "no battery clone found");
}

fn commonest_identifier(source: &str) -> Option<String> {
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for word in source
        .split(|c: char| !crate::text::is_ident_char(c))
        .filter(|word| word.len() > 3)
    {
        *counts.entry(word).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(a.0)))
        .map(|(word, _)| word.to_owned())
}
