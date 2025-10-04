use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::env;
use anyhow::{Result, anyhow};
use tokio::process::Command;
use tokio::fs;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use std::time::Duration;
use once_cell::sync::OnceCell;
use sha2::{Sha256, Digest};
use tracing::info;

pub struct PythonManager {
    uv_binary: PathBuf,
    python_dir: PathBuf,
    venv_dir: PathBuf,
    scripts_dir: PathBuf,
    /// Lazy initialization avoids downloading uv at startup if Python is never used
    initialized: OnceCell<()>,
}

impl PythonManager {
    pub async fn new() -> Result<Arc<Self>> {
        let current_dir = env::current_dir()?;
        let uv_binary = current_dir.join(".uv").join(if cfg!(windows) { "uv.exe" } else { "uv" });
        let python_dir = current_dir.join("python");
        let venv_dir = python_dir.join(".venv");
        let scripts_dir = python_dir.join("scripts");
        
        let manager = Arc::new(Self {
            uv_binary,
            python_dir,
            venv_dir,
            scripts_dir,
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
        fs::create_dir_all(self.uv_binary.parent().unwrap()).await?;
        
        if !self.uv_binary.exists() {
            self.download_uv().await?;
        }
        
        if !self.venv_dir.exists() || !self.python_dir.join("pyproject.toml").exists() {
            self.setup_environment().await?;
        }
        
        self.initialized.set(()).map_err(|_| anyhow!("Failed to mark as initialized"))?;
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
        let base_url = format!("https://github.com/astral-sh/uv/releases/download/{}", UV_VERSION);
        let file_url = format!("{}/{}", base_url, filename);
        let checksum_url = format!("{}/{}.sha256", base_url, filename);

        info!(url = %file_url, "Downloading uv binary");

        // Download the file
        let response = reqwest::get(&file_url).await?;
        if !response.status().is_success() {
            return Err(anyhow!("Failed to download uv: HTTP {}", response.status()));
        }
        let bytes = response.bytes().await?;


        // Download the checksum
        info!("Downloading checksum for verification");
        let checksum_response = reqwest::get(&checksum_url).await?;
        if !checksum_response.status().is_success() {
            return Err(anyhow!("Failed to download checksum: HTTP {}", checksum_response.status()));
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
            return Err(anyhow!(
                "Checksum verification failed!\nExpected: {}\nActual: {}",
                expected_checksum,
                actual_checksum
            ));
        }

        info!("Checksum verified successfully");

        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join(if filename.ends_with(".zip") { "uv.zip" } else { "uv.tar.gz" });
        
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
        use tar::Archive;
        use std::fs::File;
        
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
        use zip::ZipArchive;
        use std::fs::File;
        use std::io::copy;
        
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

        Command::new(&self.uv_binary)
            .args(["python", "install", "3.12"])
            .output()
            .await?;
        
        if !self.venv_dir.exists() {
            Command::new(&self.uv_binary)
                .current_dir(&self.python_dir)
                .args(["venv"])
                .output()
                .await?;
        }
        
        // Sync dependencies (requires pyproject.toml to exist)
        if self.python_dir.join("pyproject.toml").exists() {
            Command::new(&self.uv_binary)
                .current_dir(&self.python_dir)
                .args(["sync"])
                .output()
                .await?;
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
        let output = tokio::time::timeout(
            Duration::from_secs(30),
            child.wait_with_output()
        ).await
        .map_err(|_| anyhow!("Script execution timeout"))??;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Script failed: {}", stderr));
        }
        
        let output_str = String::from_utf8(output.stdout)?;
        serde_json::from_str(&output_str)
            .map_err(|e| anyhow!("Failed to parse output: {}", e))
    }
    
    pub async fn save_script(&self, name: &str, content: &str) -> Result<PathBuf> {
        self.ensure_initialized().await?;
        
        if name.is_empty() || name.contains(['/', '\\', '.']) {
            return Err(anyhow!("Invalid script name"));
        }
        
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
                if path.extension().and_then(|s| s.to_str()) == Some("py") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        scripts.push(name.to_string());
                    }
                }
            }
        }
        Ok(scripts)
    }
    
    pub fn is_ready(&self) -> bool {
        self.initialized.get().is_some()
    }
}