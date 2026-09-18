use std::collections::HashSet;
use std::path::Path;

use super::{PolicyError, PolicyErrorCode};
use crate::security::SecurityRegistry;

const BUILTINS: &[&str] = &["rg", "ls", "find", "grep", "git"];

pub fn normalize_command_name(command: &str) -> String {
    Path::new(command)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .trim_end_matches(".exe")
        .to_ascii_lowercase()
}

pub fn validate_command(
    command: &str,
    args: &[String],
    registry: &SecurityRegistry,
) -> Result<(), PolicyError> {
    let normalized = normalize_command_name(command);
    if !BUILTINS.contains(&normalized.as_str())
        && !registry.allowed_commands().contains(&normalized)
    {
        let mut allowed = BUILTINS
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        allowed.extend_from_slice(registry.allowed_commands());
        return denied(format!(
            "Command '{command}' is not allowed. Allowed commands: {}",
            allowed.join(", ")
        ));
    }
    if normalized.starts_with("rg") {
        validate_rg(args)?;
    }
    if normalized == "find" {
        validate_find(args)?;
    }
    if normalized == "git" {
        validate_git(args)?;
    }
    let pattern_positions = pattern_positions(&normalized, args);
    for (index, argument) in args.iter().enumerate() {
        let pattern = pattern_positions.contains(&index);
        let dangerous = argument.contains("${")
            || argument.contains("$(")
            || argument.contains('`')
            || argument.contains(';')
            || (!pattern
                && argument
                    .chars()
                    .any(|character| "&|$(){}[]<>".contains(character)));
        if dangerous {
            return denied(format!(
                "Dangerous pattern detected in {}: '{argument}'. This may be a command injection attempt.",
                if pattern {
                    "search pattern"
                } else {
                    "argument"
                }
            ));
        }
    }
    Ok(())
}

fn validate_rg(args: &[String]) -> Result<(), PolicyError> {
    let allowed = [
        "-F",
        "-P",
        "-s",
        "-i",
        "-S",
        "--no-unicode",
        "-w",
        "-v",
        "-a",
        "--binary",
        "-L",
        "-n",
        "--line-number",
        "--column",
        "-l",
        "--files-without-match",
        "--count-matches",
        "-c",
        "--no-ignore",
        "--no-config",
        "--hidden",
        "-U",
        "--multiline-dotall",
        "--json",
        "--stats",
        "--no-mmap",
        "--no-messages",
        "-x",
        "--passthru",
        "--debug",
    ];
    let with_values = [
        "-g",
        "--glob",
        "--include",
        "--exclude",
        "--exclude-dir",
        "-A",
        "-B",
        "-C",
        "-m",
        "-t",
        "--type",
        "-T",
        "--type-not",
        "--type-add",
        "-j",
        "--threads",
        "--sort",
        "--sortr",
        "--max-filesize",
        "-E",
        "--encoding",
        "--color",
    ];
    let short: HashSet<char> = allowed
        .iter()
        .filter_map(|flag| {
            (flag.len() == 2 && flag.starts_with('-'))
                .then(|| flag.chars().nth(1))
                .flatten()
        })
        .collect();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--" || !argument.starts_with('-') {
            break;
        }
        if argument.starts_with("--pre") {
            return denied(format!("rg option '{argument}' is not allowed."));
        }
        if with_values.contains(&argument.as_str()) {
            index += 2;
            continue;
        }
        if allowed.contains(&argument.as_str()) {
            index += 1;
            continue;
        }
        if argument.starts_with('-')
            && argument[1..].chars().count() >= 2
            && argument[1..].chars().all(|value| short.contains(&value))
        {
            index += 1;
            continue;
        }
        return denied(format!("rg option '{argument}' is not allowed."));
    }
    Ok(())
}

fn validate_find(args: &[String]) -> Result<(), PolicyError> {
    let disallowed = [
        "-delete", "-exec", "-execdir", "-ok", "-okdir", "-printf", "-fprintf", "-fprint",
        "-fprint0", "-fls", "-ls",
    ];
    if let Some(argument) = args
        .iter()
        .find(|argument| disallowed.contains(&argument.as_str()))
    {
        return denied(format!("find operator '{argument}' is not allowed."));
    }
    Ok(())
}

fn validate_git(args: &[String]) -> Result<(), PolicyError> {
    if args.is_empty() {
        return denied("git command requires a subcommand");
    }
    let safe_keys = [
        "advice.detachedHead",
        "core.autocrlf",
        "core.sparseCheckout",
        "http.extraHeader",
        "http.followRedirects",
        "http.userAgent",
        "http.version",
    ];
    let mut index = 0;
    while index < args.len() && matches!(args[index].as_str(), "-c" | "-C") {
        if args[index] == "-c" {
            let value = args.get(index + 1).map(String::as_str).unwrap_or_default();
            let key = value.split('=').next().unwrap_or_default();
            if !safe_keys.contains(&key) {
                return denied(format!("git config key '{key}' is not allowed via -c"));
            }
        }
        index += 2;
    }
    let subcommand = args.get(index).map(String::as_str).unwrap_or_default();
    if !matches!(subcommand, "clone" | "sparse-checkout") {
        return denied(format!(
            "git subcommand '{subcommand}' is not allowed. Allowed: clone, sparse-checkout"
        ));
    }
    if subcommand == "clone" {
        for argument in &args[index + 1..] {
            if ["file://", "git://", "http://"]
                .iter()
                .any(|protocol| argument.starts_with(protocol))
            {
                return denied(format!(
                    "git clone URL protocol '{}' is not allowed",
                    argument.split("//").next().unwrap_or_default().to_owned() + "//"
                ));
            }
        }
    }
    if subcommand == "sparse-checkout" {
        let action = args.get(index + 1).map(String::as_str).unwrap_or_default();
        if !matches!(action, "init" | "set" | "add" | "list" | "disable") {
            return denied(format!(
                "git sparse-checkout action '{action}' is not allowed"
            ));
        }
    }
    Ok(())
}

fn pattern_positions(command: &str, args: &[String]) -> HashSet<usize> {
    let mut result = HashSet::new();
    if (command.starts_with("rg") || command == "grep")
        && let Some(index) = args.iter().position(|argument| !argument.starts_with('-'))
    {
        result.insert(index);
    }
    if command == "find" {
        for index in 1..args.len() {
            if matches!(
                args[index - 1].as_str(),
                "-name" | "-iname" | "-path" | "-regex" | "-size" | "-perm"
            ) {
                result.insert(index);
            }
        }
    }
    result
}

fn denied<T>(message: impl Into<String>) -> Result<T, PolicyError> {
    Err(PolicyError::new(PolicyErrorCode::CommandDenied, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_shell_injection_but_allows_regex_parens() {
        let registry = SecurityRegistry::default();
        assert!(
            validate_command("rg", &["foo(bar)".to_owned(), ".".to_owned()], &registry).is_ok()
        );
        assert!(validate_command("rg", &["$(id)".to_owned()], &registry).is_err());
    }
    #[test]
    fn rejects_git_file_clone_and_find_exec() {
        let registry = SecurityRegistry::default();
        assert!(
            validate_command(
                "git",
                &["clone".to_owned(), "file:///tmp/x".to_owned()],
                &registry
            )
            .is_err()
        );
        assert!(
            validate_command("find", &[".".to_owned(), "-exec".to_owned()], &registry).is_err()
        );
    }
}
