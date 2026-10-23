//! Python docstring removal.

pub fn strip_python_docstrings(content: &str) -> String {
    let lines: Vec<&str> = content.split('\n').collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let blank: &str = "";
    let mut i = 0;

    let prev_code_line = |idx: usize, lines: &[&str]| -> &'static str {
        for j in (0..idx).rev() {
            let t = lines[j].trim();
            if !t.is_empty() && !t.starts_with('#') {
                return "NON_EMPTY"; // just signals "something is there"
            }
        }
        ""
    };

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let delim = if trimmed.starts_with("\"\"\"") {
                "\"\"\""
            } else {
                "'''"
            };
            let prev = prev_code_line(i, &lines);
            let is_doc = prev.is_empty() || {
                // Check if previous code line ends with ':'
                let mut code_line = "";
                for j in (0..i).rev() {
                    let t = lines[j].trim();
                    if !t.is_empty() && !t.starts_with('#') {
                        code_line = t;
                        break;
                    }
                }
                code_line.ends_with(':') || code_line.is_empty()
            };

            if is_doc {
                let after_open = trimmed.get(3..).unwrap_or("");
                out.push(blank); // preserve line count
                i += 1;
                if !after_open.contains(delim) {
                    while i < lines.len() {
                        out.push(blank);
                        let nl = lines[i];
                        i += 1;
                        if nl.contains(delim) {
                            break;
                        }
                    }
                }
                continue;
            }
        }
        out.push(line);
        i += 1;
    }
    out.join("\n")
}
