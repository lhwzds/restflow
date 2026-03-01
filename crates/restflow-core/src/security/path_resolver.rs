use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CommandResolution {
    pub resolved_path: Option<PathBuf>,
    pub executable_name: String,
}

impl CommandResolution {
    pub fn resolve(command: &str, cwd: Option<&Path>) -> Option<Self> {
        let first_token = shell_words::split(command).ok()?.into_iter().next()?;
        let resolved = if first_token.contains('/') {
            let path = match cwd {
                Some(cwd) => cwd.join(&first_token),
                None => PathBuf::from(&first_token),
            };
            path.canonicalize().ok()
        } else {
            which::which(&first_token).ok()
        };

        let executable_name = resolved
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| first_token.clone());

        Some(Self {
            resolved_path: resolved,
            executable_name,
        })
    }
}

pub fn matches_path_pattern(pattern: &str, resolution: &CommandResolution) -> bool {
    if pattern.contains('/') {
        if let Some(ref resolved) = resolution.resolved_path {
            return glob_match::glob_match(pattern, &resolved.to_string_lossy());
        }
        return false;
    }

    glob_match::glob_match(pattern, &resolution.executable_name)
}
