use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(in super::super) enum SkillPlatform {
    Pi,
    Cursor,
    Claude,
    Codex,
    Opencode,
    Copilot,
    Gemini,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(in super::super) enum SkillScope {
    Global,
    Project,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(in super::super) enum SkillInstallMode {
    #[default]
    Symlink,
    Copy,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(in super::super) enum ConcreteMode {
    Symlink,
    Copy,
}

pub(in super::super) struct PlatformDescriptor {
    pub(in super::super) platform: SkillPlatform,
    pub(in super::super) auto_mode: ConcreteMode,
    pub(in super::super) global_relative_path: &'static str,
    pub(in super::super) project_relative_path: &'static str,
}

pub(in super::super) const SKILL_PLATFORMS: [PlatformDescriptor; 7] = [
    PlatformDescriptor {
        platform: SkillPlatform::Pi,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".pi/agent/skills",
        project_relative_path: ".pi/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Cursor,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".cursor/skills",
        project_relative_path: ".cursor/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Claude,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".claude/skills",
        project_relative_path: ".claude/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Codex,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".agents/skills",
        project_relative_path: ".agents/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Opencode,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".config/opencode/skills",
        project_relative_path: ".opencode/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Copilot,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".copilot/skills",
        project_relative_path: ".github/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Gemini,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".gemini/skills",
        project_relative_path: ".gemini/skills",
    },
];

pub(super) fn descriptor(platform: SkillPlatform) -> &'static PlatformDescriptor {
    SKILL_PLATFORMS
        .iter()
        .find(|candidate| candidate.platform == platform)
        .unwrap_or(&SKILL_PLATFORMS[0])
}

pub(in super::super) fn parse_skill_platforms(raw: &str) -> Result<Vec<SkillPlatform>, String> {
    let mut platforms = Vec::new();
    for value in raw
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        if value == "all" {
            for entry in &SKILL_PLATFORMS {
                if !platforms.contains(&entry.platform) {
                    platforms.push(entry.platform);
                }
            }
            continue;
        }
        let matched = SKILL_PLATFORMS
            .iter()
            .find(|entry| entry.platform.name() == value);
        match matched {
            Some(entry) => {
                if !platforms.contains(&entry.platform) {
                    platforms.push(entry.platform);
                }
            }
            None => return Err(format!("Unknown platform: \"{value}\"")),
        }
    }
    Ok(platforms)
}

impl SkillPlatform {
    pub(in super::super) fn name(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::Cursor => "cursor",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Copilot => "copilot",
            Self::Gemini => "gemini",
        }
    }
}
