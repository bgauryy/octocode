use std::path::PathBuf;

use regex::{Regex, RegexBuilder};

use crate::policy::{PolicyError, PolicyErrorCode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchAccuracy {
    High,
    Medium,
}

#[derive(Clone, Debug)]
pub struct SensitiveDataPattern {
    pub name: String,
    pub description: String,
    pub regex: Regex,
    pub file_context: Option<Regex>,
    pub match_accuracy: Option<MatchAccuracy>,
    /// Mirrors JavaScript RegExp's `g` replacement behavior.
    pub global: bool,
}

impl SensitiveDataPattern {
    pub fn compile(
        name: impl Into<String>,
        description: impl Into<String>,
        source: &str,
        case_insensitive: bool,
        multi_line: bool,
        file_context: Option<&str>,
    ) -> Result<Self, PolicyError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(PolicyError::new(
                PolicyErrorCode::InvalidInput,
                "Each pattern must have a name and regex",
            ));
        }
        let compile_name = name.clone();
        let compile = |value: &str| {
            RegexBuilder::new(value)
                .case_insensitive(case_insensitive)
                .multi_line(multi_line)
                .build()
                .map_err(|error| {
                    PolicyError::new(
                        PolicyErrorCode::UnsupportedRegex,
                        format!(
                            "Pattern '{compile_name}' is not supported by the native regex engine: {error}"
                        ),
                    )
                })
        };
        Ok(Self {
            name,
            description: description.into(),
            regex: compile(source)?,
            file_context: file_context.map(compile).transpose()?,
            match_accuracy: None,
            global: true,
        })
    }

    /// Compile the JavaScript-compatible subset used by configuration. Unsupported
    /// flags and syntax fail visibly instead of silently changing match behavior.
    pub fn compile_js(
        name: impl Into<String>,
        description: impl Into<String>,
        source: &str,
        flags: &str,
        file_context: Option<(&str, &str)>,
    ) -> Result<Self, PolicyError> {
        if flags
            .chars()
            .any(|flag| !matches!(flag, 'g' | 'i' | 'm' | 's' | 'u' | 'y'))
        {
            return Err(PolicyError::new(
                PolicyErrorCode::UnsupportedRegex,
                format!("Unsupported JavaScript regex flags: {flags}"),
            ));
        }
        if flags.contains('y') {
            return Err(PolicyError::new(
                PolicyErrorCode::UnsupportedRegex,
                "JavaScript sticky regexes are unsupported by the native policy engine",
            ));
        }
        let mut pattern = Self::compile(
            name,
            description,
            source,
            flags.contains('i'),
            flags.contains('m'),
            file_context.map(|(source, _)| source),
        )?;
        if flags.contains('s') {
            pattern.regex = RegexBuilder::new(source)
                .case_insensitive(flags.contains('i'))
                .multi_line(flags.contains('m'))
                .dot_matches_new_line(true)
                .build()
                .map_err(|error| {
                    PolicyError::new(PolicyErrorCode::UnsupportedRegex, error.to_string())
                })?;
        }
        if let Some((context_source, context_flags)) = file_context {
            if context_flags
                .chars()
                .any(|flag| !matches!(flag, 'g' | 'i' | 'm' | 's' | 'u'))
            {
                return Err(PolicyError::new(
                    PolicyErrorCode::UnsupportedRegex,
                    format!("Unsupported JavaScript file-context regex flags: {context_flags}"),
                ));
            }
            pattern.file_context = Some(
                RegexBuilder::new(context_source)
                    .case_insensitive(context_flags.contains('i'))
                    .multi_line(context_flags.contains('m'))
                    .dot_matches_new_line(context_flags.contains('s'))
                    .build()
                    .map_err(|error| {
                        PolicyError::new(PolicyErrorCode::UnsupportedRegex, error.to_string())
                    })?,
            );
        }
        pattern.global = flags.contains('g');
        Ok(pattern)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SecurityRegistry {
    secret_patterns: Vec<SensitiveDataPattern>,
    allowed_commands: Vec<String>,
    allowed_roots: Vec<PathBuf>,
    ignored_path_patterns: Vec<Regex>,
    ignored_file_patterns: Vec<Regex>,
    version: u64,
    frozen: bool,
}

impl SecurityRegistry {
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn frozen(&self) -> bool {
        self.frozen
    }
    pub fn secret_patterns(&self) -> &[SensitiveDataPattern] {
        &self.secret_patterns
    }
    pub fn allowed_commands(&self) -> &[String] {
        &self.allowed_commands
    }
    pub fn allowed_roots(&self) -> &[PathBuf] {
        &self.allowed_roots
    }
    pub fn ignored_path_patterns(&self) -> &[Regex] {
        &self.ignored_path_patterns
    }
    pub fn ignored_file_patterns(&self) -> &[Regex] {
        &self.ignored_file_patterns
    }

    pub fn add_secret_patterns(
        &mut self,
        patterns: impl IntoIterator<Item = SensitiveDataPattern>,
    ) -> Result<(), PolicyError> {
        self.mutable()?;
        for pattern in patterns {
            if !self
                .secret_patterns
                .iter()
                .any(|existing| existing.name == pattern.name)
            {
                self.secret_patterns.push(pattern);
            }
        }
        self.version += 1;
        Ok(())
    }
    pub fn add_allowed_commands(
        &mut self,
        commands: impl IntoIterator<Item = String>,
    ) -> Result<(), PolicyError> {
        self.mutable()?;
        for command in commands {
            if command.trim().is_empty() {
                return Err(PolicyError::new(
                    PolicyErrorCode::InvalidInput,
                    "Each command must be a non-empty string",
                ));
            }
            let command = normalize_command_name(&command);
            if !self.allowed_commands.contains(&command) {
                self.allowed_commands.push(command);
            }
        }
        self.version += 1;
        Ok(())
    }
    pub fn add_allowed_roots(
        &mut self,
        roots: impl IntoIterator<Item = PathBuf>,
    ) -> Result<(), PolicyError> {
        self.mutable()?;
        for root in roots {
            if root.as_os_str().is_empty() {
                return Err(PolicyError::new(
                    PolicyErrorCode::InvalidInput,
                    "Each root must be a non-empty path",
                ));
            }
            if !self.allowed_roots.contains(&root) {
                self.allowed_roots.push(root);
            }
        }
        self.version += 1;
        Ok(())
    }
    pub fn add_ignored_path_patterns(
        &mut self,
        patterns: impl IntoIterator<Item = Regex>,
    ) -> Result<(), PolicyError> {
        self.mutable()?;
        for pattern in patterns {
            if !self
                .ignored_path_patterns
                .iter()
                .any(|existing| existing.as_str() == pattern.as_str())
            {
                self.ignored_path_patterns.push(pattern);
            }
        }
        self.version += 1;
        Ok(())
    }
    pub fn add_ignored_file_patterns(
        &mut self,
        patterns: impl IntoIterator<Item = Regex>,
    ) -> Result<(), PolicyError> {
        self.mutable()?;
        for pattern in patterns {
            if !self
                .ignored_file_patterns
                .iter()
                .any(|existing| existing.as_str() == pattern.as_str())
            {
                self.ignored_file_patterns.push(pattern);
            }
        }
        self.version += 1;
        Ok(())
    }
    pub fn freeze(&mut self) {
        self.frozen = true;
    }
    pub fn reset(&mut self) {
        self.frozen = false;
        self.secret_patterns.clear();
        self.allowed_commands.clear();
        self.allowed_roots.clear();
        self.ignored_path_patterns.clear();
        self.ignored_file_patterns.clear();
        self.version += 1;
    }
    fn mutable(&self) -> Result<(), PolicyError> {
        if self.frozen {
            Err(PolicyError::new(
                PolicyErrorCode::RegistryFrozen,
                "SecurityRegistry is frozen — call reset() to unfreeze before mutating",
            ))
        } else {
            Ok(())
        }
    }
}

pub fn normalize_command_name(command: &str) -> String {
    std::path::Path::new(command)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .trim_end_matches(".exe")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_deduplicates_versions_and_freezes() {
        let mut registry = SecurityRegistry::default();
        registry
            .add_allowed_commands(["/bin/RG".to_owned(), "rg".to_owned()])
            .unwrap();
        assert_eq!(registry.allowed_commands(), ["rg"]);
        assert_eq!(registry.version(), 1);
        registry.freeze();
        assert_eq!(
            registry
                .add_allowed_roots([PathBuf::from("/tmp")])
                .unwrap_err()
                .code,
            PolicyErrorCode::RegistryFrozen
        );
        registry.reset();
        assert!(!registry.frozen());
    }
    #[test]
    fn unsupported_js_regex_is_visible() {
        let error =
            SensitiveDataPattern::compile("lookbehind", "", "(?<=token=)x", false, false, None)
                .unwrap_err();
        assert_eq!(error.code, PolicyErrorCode::UnsupportedRegex);
    }
}
