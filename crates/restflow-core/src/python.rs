use anyhow::{Result, anyhow};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{error, info};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

#[derive(Debug)]
pub struct PythonManager {
    uv_binary: PathBuf,
    python_dir: PathBuf,
    venv_dir: PathBuf,
    scripts_dir: PathBuf,
    templates_dir: PathBuf,
    /// Lazy initialization avoids downloading uv at startup if Python is never used
    initialized: OnceCell<()>,
}

impl PythonManager {
    pub async fn new() -> Result<Arc<Self>> {
        let current_dir = env::current_dir()?;
        let uv_binary = current_dir
            .join(".uv")
            .join(if cfg!(windows) { "uv.exe" } else { "uv" });
        let python_dir = current_dir.join("python");
        let venv_dir = python_dir.join(".venv");
        let scripts_dir = python_dir.join("scripts");
        let templates_dir = scripts_dir.join("templates");

        let manager = Arc::new(Self {
            uv_binary,
            python_dir,
            venv_dir,
            scripts_dir,
            templates_dir,
            initialized: OnceCell::new(),
        });

        manager.ensure_initialized().await?;
        Ok(manager)
    }

    async fn ensure_initialized(&self) -> Result<()> {
        if self.initialized.get().is_some() {
            return Ok(());
        }

        fs::create_dir_all(&self.python_dir).await?;
        fs::create_dir_all(&self.scripts_dir).await?;
        fs::create_dir_all(&self.templates_dir).await?;
        fs::create_dir_all(self.uv_binary.parent().unwrap()).await?;

        if !self.uv_binary.exists() {
            self.download_uv().await?;
        }

        if !self.venv_dir.exists() || !self.python_dir.join("pyproject.toml").exists() {
            self.setup_environment().await?;
        }

        self.initialized
            .set(())
            .map_err(|_| anyhow!("Failed to mark as initialized"))?;
        Ok(())
    }

    /// Downloads uv binary and verifies SHA256 checksum to prevent supply chain attacks.
    /// Uses a fixed version instead of 'latest' for reproducibility and security.
    async fn download_uv(&self) -> Result<()> {
        info!("Downloading uv package manager");

        // Use fixed version for security
        const UV_VERSION: &str = "0.8.15";

        let filename = match (env::consts::OS, env::consts::ARCH) {
            ("macos", "aarch64") => "uv-aarch64-apple-darwin.tar.gz",
            ("macos", "x86_64") => "uv-x86_64-apple-darwin.tar.gz",
            ("windows", _) => "uv-x86_64-pc-windows-msvc.zip",
            ("linux", "x86_64") => "uv-x86_64-unknown-linux-gnu.tar.gz",
            ("linux", "aarch64") => "uv-aarch64-unknown-linux-gnu.tar.gz",
            _ => return Err(anyhow!("Unsupported platform")),
        };

        // Generate URLs for both file and checksum
        let base_url = format!(
            "https://github.com/astral-sh/uv/releases/download/{}",
            UV_VERSION
        );
        let file_url = format!("{}/{}", base_url, filename);
        let checksum_url = format!("{}/{}.sha256", base_url, filename);

        info!(url = %file_url, "Downloading uv binary");

        // Download the file
        let response = reqwest::get(&file_url).await?;
        if !response.status().is_success() {
            error!(
                url = %file_url,
                status = %response.status(),
                "Failed to download uv binary from GitHub"
            );
            return Err(anyhow!("Failed to download uv: HTTP {}", response.status()));
        }
        let bytes = response.bytes().await?;

        // Download the checksum
        info!("Downloading checksum for verification");
        let checksum_response = reqwest::get(&checksum_url).await?;
        if !checksum_response.status().is_success() {
            error!(
                url = %checksum_url,
                status = %checksum_response.status(),
                "Failed to download checksum file from GitHub"
            );
            return Err(anyhow!(
                "Failed to download checksum: HTTP {}",
                checksum_response.status()
            ));
        }
        let checksum_text = checksum_response.text().await?;

        // Parse the expected checksum (format: "HASH  filename" or "HASH *filename")
        let expected_checksum = checksum_text
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow!("Invalid checksum format"))?
            .to_lowercase();

        // Calculate actual checksum
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual_checksum = format!("{:x}", hasher.finalize());

        // Verify checksum
        if actual_checksum != expected_checksum {
            error!(
                expected = %expected_checksum,
                actual = %actual_checksum,
                "Checksum verification failed - possible corrupted download or security issue"
            );
            return Err(anyhow!(
                "Checksum verification failed!\nExpected: {}\nActual: {}",
                expected_checksum,
                actual_checksum
            ));
        }

        info!("Checksum verified successfully");

        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join(if filename.ends_with(".zip") {
            "uv.zip"
        } else {
            "uv.tar.gz"
        });

        fs::write(&archive_path, &bytes).await?;

        // Extract the binary
        if archive_path.extension().and_then(|s| s.to_str()) == Some("zip") {
            self.extract_zip(&archive_path).await?;
        } else {
            self.extract_tar_gz(&archive_path).await?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&self.uv_binary).await?.permissions();
            perms.set_mode(0o755); // Make binary executable (rwxr-xr-x)
            fs::set_permissions(&self.uv_binary, perms).await?;
        }

        info!("uv installed successfully");
        Ok(())
    }

    /// Extract tar.gz archive
    async fn extract_tar_gz(&self, archive_path: &Path) -> Result<()> {
        use flate2::read::GzDecoder;
        use std::fs::File;
        use tar::Archive;

        let file = File::open(archive_path)?;
        let gz = GzDecoder::new(file);
        let mut archive = Archive::new(gz);

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?;
            if path.file_name() == Some(std::ffi::OsStr::new("uv")) {
                entry.unpack(&self.uv_binary)?;
                return Ok(());
            }
        }

        Err(anyhow!("uv binary not found in archive"))
    }

    /// Extract zip archive (Windows)
    async fn extract_zip(&self, archive_path: &Path) -> Result<()> {
        use std::fs::File;
        use std::io::copy;
        use zip::ZipArchive;

        let file = File::open(archive_path)?;
        let mut archive = ZipArchive::new(file)?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file.name().ends_with("uv.exe") {
                let mut out_file = File::create(&self.uv_binary)?;
                copy(&mut file, &mut out_file)?;
                return Ok(());
            }
        }

        Err(anyhow!("uv.exe not found in archive"))
    }

    async fn setup_environment(&self) -> Result<()> {
        info!("Setting up Python environment");

        // Install Python 3.12
        let python_install_output = Command::new(&self.uv_binary)
            .args(["python", "install", "3.12"])
            .output()
            .await?;

        if !python_install_output.status.success() {
            error!(
                stderr = %String::from_utf8_lossy(&python_install_output.stderr),
                "Failed to install Python 3.12 using uv"
            );
            return Err(anyhow!("Failed to install Python 3.12"));
        }

        // Create virtual environment if not exists
        if !self.venv_dir.exists() {
            let venv_output = Command::new(&self.uv_binary)
                .current_dir(&self.python_dir)
                .args(["venv"])
                .output()
                .await?;

            if !venv_output.status.success() {
                error!(
                    stderr = %String::from_utf8_lossy(&venv_output.stderr),
                    python_dir = %self.python_dir.display(),
                    "Failed to create Python virtual environment"
                );
                return Err(anyhow!("Failed to create virtual environment"));
            }
        }

        // Sync dependencies (requires pyproject.toml to exist)
        // SECURITY: python_dir is internal configuration, not user-controlled.
        if self.python_dir.join("pyproject.toml").exists() {
            let sync_output = Command::new(&self.uv_binary)
                .current_dir(&self.python_dir)
                .args(["sync"])
                .output()
                .await?;

            if !sync_output.status.success() {
                error!(
                    stderr = %String::from_utf8_lossy(&sync_output.stderr),
                    python_dir = %self.python_dir.display(),
                    "Failed to sync Python dependencies"
                );
                return Err(anyhow!("Failed to sync dependencies"));
            }
        }

        info!("Python environment ready");
        Ok(())
    }

    /// Executes a Python script in an isolated subprocess with JSON I/O.
    /// Scripts receive input via stdin and must output valid JSON to stdout.
    pub async fn execute_script(&self, script_name: &str, input: Value) -> Result<Value> {
        self.ensure_initialized().await?;

        // Basic validation for internal use
        if script_name.is_empty() || script_name.contains(['/', '\\', '.']) {
            return Err(anyhow!("Invalid script name"));
        }

        // SECURITY: script_name validated above - no path traversal possible.
        let script_path = self.scripts_dir.join(format!("{}.py", script_name));
        if !script_path.exists() {
            return Err(anyhow!("Script not found: {}", script_name));
        }

        // Use uv run for execution
        let mut cmd = Command::new(&self.uv_binary);
        cmd.current_dir(&self.python_dir)
            .args(["run", "python", script_path.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;

        // Pass input via stdin
        if let Some(mut stdin) = child.stdin.take() {
            let input_json = serde_json::to_string(&input)?;
            stdin.write_all(input_json.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin);
        }

        // 30 second timeout prevents hanging scripts from blocking workflow execution
        let output = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
            .await
            .map_err(|_| anyhow!("Script execution timeout"))??;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Script failed: {}", stderr));
        }

        let output_str = String::from_utf8(output.stdout)?;
        serde_json::from_str(&output_str).map_err(|e| anyhow!("Failed to parse output: {}", e))
    }

    pub async fn save_script(&self, name: &str, content: &str) -> Result<PathBuf> {
        self.ensure_initialized().await?;

        if name.is_empty() || name.contains(['/', '\\', '.']) {
            return Err(anyhow!("Invalid script name"));
        }

        // SECURITY: name validated above - no path traversal possible.
        let script_path = self.scripts_dir.join(format!("{}.py", name));
        fs::write(&script_path, content).await?;
        Ok(script_path)
    }

    pub async fn list_scripts(&self) -> Result<Vec<String>> {
        self.ensure_initialized().await?;

        let mut scripts = Vec::new();
        if self.scripts_dir.exists() {
            let mut entries = fs::read_dir(&self.scripts_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("py")
                    && let Some(name) = path.file_stem().and_then(|s| s.to_str())
                {
                    scripts.push(name.to_string());
                }
            }
        }
        Ok(scripts)
    }

    pub fn is_ready(&self) -> bool {
        self.initialized.get().is_some()
    }

    /// Execute inline Python code with PEP 723 dependencies using uv run
    pub async fn execute_inline_code(
        &self,
        code: &str,
        input: Value,
        env_vars: HashMap<String, String>,
    ) -> Result<Value> {
        self.ensure_initialized().await?;

        // Create temporary script file
        let temp_dir = tempfile::tempdir()?;
        let script_path = temp_dir.path().join("script.py");
        fs::write(&script_path, code).await?;

        // Use uv run --no-project to execute (automatically handles PEP 723 dependencies)
        // --refresh ensures dependency changes are detected and installed
        let mut cmd = Command::new(&self.uv_binary);
        cmd.args([
            "run",
            "--no-project",
            "--refresh",
            script_path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

        // Inject environment variables (e.g., API keys from Secret Manager)
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn()?;

        // Pass input via stdin
        if let Some(mut stdin) = child.stdin.take() {
            let input_json = serde_json::to_string(&input)?;
            stdin.write_all(input_json.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin);
        }

        // 30 second timeout for inline code execution
        let output =
            match tokio::time::timeout(Duration::from_secs(30), child.wait_with_output()).await {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => {
                    error!(error = %e, "Failed to wait for Python script execution");
                    return Err(e.into());
                }
                Err(_) => {
                    error!("Python script execution timeout after 30 seconds");
                    return Err(anyhow!("Script execution timeout"));
                }
            };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                stderr = %stderr,
                exit_code = ?output.status.code(),
                "Python script execution failed"
            );
            return Err(anyhow!("Script execution failed: {}", stderr));
        }

        let output_str = String::from_utf8(output.stdout)?;
        serde_json::from_str(&output_str)
            .map_err(|e| anyhow!("Failed to parse output as JSON: {}", e))
    }

    /// List all available Python script templates by scanning .py files
    pub async fn list_templates(&self) -> Result<Vec<TemplateInfo>> {
        self.ensure_initialized().await?;

        let mut templates = Vec::new();

        if !self.templates_dir.exists() {
            return Ok(templates);
        }

        // SECURITY: templates_dir is internal configuration, not user-controlled.
        let mut entries = fs::read_dir(&self.templates_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Only process .py files
            if path.extension().and_then(|s| s.to_str()) != Some("py") {
                continue;
            }

            let file_stem = path.file_stem().and_then(|s| s.to_str());
            if file_stem.is_none() {
                continue;
            }

            let content = fs::read_to_string(&path).await?;

            // Parse metadata block: # /// metadata ... # ///
            let metadata = Self::parse_metadata_block(&content)?;
            let dependencies = Self::parse_dependencies_block(&content)?;

            templates.push(TemplateInfo {
                id: file_stem.unwrap().to_string(),
                name: metadata.get("name").cloned().unwrap_or_default(),
                description: metadata.get("description").cloned().unwrap_or_default(),
                dependencies,
            });
        }

        // Sort alphabetically by id (filename)
        templates.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(templates)
    }

    /// Parse metadata block from Python file
    fn parse_metadata_block(content: &str) -> Result<HashMap<String, String>> {
        // Match the entire metadata block: # /// metadata ... # ///
        let re = regex::Regex::new(r"(?s)# /// metadata\s*\n(.*?)# ///")
            .map_err(|e| anyhow!("Invalid metadata regex: {}", e))?;

        if let Some(caps) = re.captures(content) {
            let block_content = caps.get(1).unwrap().as_str();

            // Remove '# ' prefix from each line and join
            let json_str: String = block_content
                .lines()
                .map(|line| line.trim_start_matches('#').trim())
                .collect::<Vec<_>>()
                .join("\n");

            let metadata: HashMap<String, Value> = serde_json::from_str(&json_str)?;

            // Convert to HashMap<String, String>
            let mut result = HashMap::new();
            for (key, value) in metadata {
                if let Some(s) = value.as_str() {
                    result.insert(key, s.to_string());
                }
            }
            Ok(result)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Parse dependencies block from Python file (PEP 723 format)
    fn parse_dependencies_block(content: &str) -> Result<Vec<String>> {
        // Match the entire script block: # /// script ... # ///
        let re = regex::Regex::new(r"(?s)# /// script\s*\n(.*?)# ///")
            .map_err(|e| anyhow!("Invalid dependencies regex: {}", e))?;

        if let Some(caps) = re.captures(content) {
            let block_content = caps.get(1).unwrap().as_str();

            // Remove '# ' prefix from each line
            let clean_content: String = block_content
                .lines()
                .map(|line| line.trim_start_matches('#').trim())
                .collect::<Vec<_>>()
                .join("\n");

            // Extract dependencies array using regex ((?s) enables DOTALL mode to match newlines)
            let dep_re = regex::Regex::new(r"(?s)dependencies\s*=\s*\[(.*?)\]")
                .map_err(|e| anyhow!("Invalid dependency array regex: {}", e))?;

            if let Some(dep_caps) = dep_re.captures(&clean_content) {
                let deps_str = dep_caps.get(1).unwrap().as_str();

                // Parse each dependency (quoted strings)
                let deps: Vec<String> = deps_str
                    .split(',')
                    .filter_map(|dep| {
                        let trimmed = dep.trim().trim_matches('"').trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    })
                    .collect();

                return Ok(deps);
            }
        }

        Ok(Vec::new())
    }

    /// Get a specific template by ID
    pub async fn get_template(&self, template_id: &str) -> Result<HashMap<String, String>> {
        self.ensure_initialized().await?;

        // Validate template ID (alphanumeric, underscore, and hyphen only)
        if !template_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(anyhow!("Invalid template ID"));
        }

        // Build template file path
        // SECURITY: template_id validated above - no path traversal possible.
        let template_path = self.templates_dir.join(format!("{}.py", template_id));
        if !template_path.exists() {
            return Err(anyhow!("Template not found: {}", template_id));
        }

        // Read file content
        let content = fs::read_to_string(&template_path).await?;

        // Parse metadata and dependencies
        let metadata = Self::parse_metadata_block(&content)?;
        let dependencies = Self::parse_dependencies_block(&content)?;

        // Remove metadata blocks from content (user only sees actual code)
        let clean_content = Self::strip_metadata_blocks(&content);

        // Return template info
        let mut result = HashMap::new();
        result.insert("id".to_string(), template_id.to_string());
        result.insert(
            "name".to_string(),
            metadata.get("name").cloned().unwrap_or_default(),
        );
        result.insert(
            "description".to_string(),
            metadata.get("description").cloned().unwrap_or_default(),
        );
        result.insert("content".to_string(), clean_content);
        result.insert(
            "dependencies".to_string(),
            serde_json::to_string(&dependencies)?,
        );

        Ok(result)
    }

    /// Remove metadata and script blocks from Python code
    fn strip_metadata_blocks(content: &str) -> String {
        let mut result = content.to_string();

        // Remove # /// script ... # ///
        if let Ok(re) = regex::Regex::new(r"(?s)# /// script\s*\n.*?# ///\s*\n+") {
            result = re.replace_all(&result, "").to_string();
        }

        // Remove # /// metadata ... # ///
        if let Ok(re) = regex::Regex::new(r"(?s)# /// metadata\s*\n.*?# ///\s*\n+") {
            result = re.replace_all(&result, "").to_string();
        }

        result.trim_start().to_string()
    }

    /// Create a mock PythonManager for testing without initialization.
    /// This avoids downloading uv binary or creating filesystem structures.
    pub fn new_mock() -> Arc<Self> {
        Arc::new(Self {
            uv_binary: PathBuf::from("/tmp/test-uv"),
            python_dir: PathBuf::from("/tmp/python"),
            venv_dir: PathBuf::from("/tmp/.venv"),
            scripts_dir: PathBuf::from("/tmp/scripts"),
            templates_dir: PathBuf::from("/tmp/templates"),
            initialized: OnceCell::new(),
        })
    }
}
