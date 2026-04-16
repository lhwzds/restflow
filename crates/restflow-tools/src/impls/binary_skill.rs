use async_trait::async_trait;
use restflow_build::{
    BuildBinaryOptions, BuildProfile, CreateSkillProjectOptions, RunBinaryOptions,
    UpdateSkillProjectOptions, build_skill_binary, create_skill_project, read_skill_project,
    run_skill_binary, update_skill_project,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::Result;
use crate::security::{SecurityGate, ToolAction};
use crate::{Tool, ToolErrorCategory, ToolOutput, check_security};

#[derive(Debug, Deserialize)]
struct BinarySkillNewInput {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    toolchain: Option<String>,
    #[serde(default)]
    cargo_toml: Option<String>,
    #[serde(default)]
    main_rs: Option<String>,
    #[serde(default)]
    skill_markdown: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BinarySkillBuildInput {
    id: String,
    #[serde(default)]
    release: bool,
    #[serde(default)]
    toolchain: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BinarySkillRunInput {
    id: String,
    #[serde(default)]
    input: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct BinarySkillReadInput {
    id: String,
}

#[derive(Debug, Deserialize)]
struct BinarySkillUpdateInput {
    id: String,
    #[serde(default)]
    cargo_toml: Option<String>,
    #[serde(default)]
    main_rs: Option<String>,
    #[serde(default)]
    skill_markdown: Option<String>,
}

#[derive(Clone)]
pub struct BinarySkillNewTool {
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for BinarySkillNewTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BinarySkillNewTool {
    pub fn new() -> Self {
        Self {
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

#[derive(Clone)]
pub struct BinarySkillBuildTool {
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for BinarySkillBuildTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BinarySkillBuildTool {
    pub fn new() -> Self {
        Self {
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

#[derive(Clone)]
pub struct BinarySkillRunTool {
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

#[derive(Clone)]
pub struct BinarySkillReadTool {
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for BinarySkillReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BinarySkillReadTool {
    pub fn new() -> Self {
        Self {
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

#[derive(Clone)]
pub struct BinarySkillUpdateTool {
    security_gate: Option<Arc<dyn SecurityGate>>,
    agent_id: Option<String>,
    task_id: Option<String>,
}

impl Default for BinarySkillUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BinarySkillUpdateTool {
    pub fn new() -> Self {
        Self {
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

impl Default for BinarySkillRunTool {
    fn default() -> Self {
        Self::new()
    }
}

impl BinarySkillRunTool {
    pub fn new() -> Self {
        Self {
            security_gate: None,
            agent_id: None,
            task_id: None,
        }
    }

    pub fn with_security(
        mut self,
        security_gate: Arc<dyn SecurityGate>,
        agent_id: impl Into<String>,
        task_id: impl Into<String>,
    ) -> Self {
        self.security_gate = Some(security_gate);
        self.agent_id = Some(agent_id.into());
        self.task_id = Some(task_id.into());
        self
    }
}

async fn check_build_security(
    gate: Option<&dyn SecurityGate>,
    tool_name: &str,
    operation: &str,
    target: &str,
    agent_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<Option<String>> {
    check_security(
        gate,
        ToolAction {
            tool_name: tool_name.to_string(),
            operation: operation.to_string(),
            target: target.to_string(),
            summary: format!("{tool_name} {operation} {target}"),
        },
        agent_id,
        task_id,
    )
    .await
}

#[async_trait]
impl Tool for BinarySkillNewTool {
    fn name(&self) -> &str {
        "binary_skill_new"
    }

    fn description(&self) -> &str {
        "Create a new Rust skill binary project under ~/.restflow/skills/<id>."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "name": { "type": "string" },
                "toolchain": { "type": "string" },
                "cargo_toml": { "type": "string" },
                "main_rs": { "type": "string" },
                "skill_markdown": { "type": "string" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: BinarySkillNewInput = serde_json::from_value(input)?;
        if let Some(message) = check_build_security(
            self.security_gate.as_deref(),
            self.name(),
            "new",
            &params.id,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::non_retryable_error(
                message,
                ToolErrorCategory::Auth,
            ));
        }
        match create_skill_project(&CreateSkillProjectOptions {
            id: params.id,
            name: params.name,
            toolchain: params.toolchain,
            cargo_toml: params.cargo_toml,
            main_rs: params.main_rs,
            skill_markdown: params.skill_markdown,
        }) {
            Ok(result) => Ok(ToolOutput::success(json!({
                "skill_dir": result.skill_dir,
                "artifact_path": result.artifact_path,
                "manifest_path": result.manifest_path,
                "source_path": result.source_path,
                "skill_markdown_path": result.skill_markdown_path,
            }))),
            Err(error) => Ok(ToolOutput::non_retryable_error(
                error.to_string(),
                ToolErrorCategory::Execution,
            )),
        }
    }
}

#[async_trait]
impl Tool for BinarySkillBuildTool {
    fn name(&self) -> &str {
        "binary_skill_build"
    }

    fn description(&self) -> &str {
        "Build a Rust skill binary project using the RestFlow-managed toolchain."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "release": { "type": "boolean" },
                "toolchain": { "type": "string" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: BinarySkillBuildInput = serde_json::from_value(input)?;
        if let Some(message) = check_build_security(
            self.security_gate.as_deref(),
            self.name(),
            "build",
            &params.id,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::non_retryable_error(
                message,
                ToolErrorCategory::Auth,
            ));
        }

        match build_skill_binary(&BuildBinaryOptions {
            skill_id: params.id,
            profile: if params.release {
                BuildProfile::Release
            } else {
                BuildProfile::Debug
            },
            toolchain_override: params.toolchain,
        }) {
            Ok(result) => {
                if result.success {
                    Ok(ToolOutput::success(json!({
                        "binary_path": result.binary_path,
                        "profile": result.profile,
                        "toolchain": result.toolchain,
                        "stdout": result.stdout,
                        "stderr": result.stderr,
                        "exit_code": result.exit_code,
                    })))
                } else {
                    Ok(ToolOutput::non_retryable_error(
                        format!(
                            "build failed for {} (exit code {}): {}",
                            result.binary_path.display(),
                            result.exit_code,
                            result.stderr
                        ),
                        ToolErrorCategory::Execution,
                    ))
                }
            }
            Err(error) => Ok(ToolOutput::retryable_error(
                error.to_string(),
                ToolErrorCategory::Execution,
            )),
        }
    }
}

#[async_trait]
impl Tool for BinarySkillRunTool {
    fn name(&self) -> &str {
        "binary_skill_run"
    }

    fn description(&self) -> &str {
        "Run a compiled skill binary and pass JSON input to stdin."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "input": {}
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: BinarySkillRunInput = serde_json::from_value(input)?;
        if let Some(message) = check_build_security(
            self.security_gate.as_deref(),
            self.name(),
            "run",
            &params.id,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::non_retryable_error(
                message,
                ToolErrorCategory::Auth,
            ));
        }

        let stdin_json = params
            .input
            .map(|value| serde_json::to_string(&value))
            .transpose()?;
        match run_skill_binary(&RunBinaryOptions {
            skill_id: params.id,
            stdin_json,
        }) {
            Ok(result) => {
                if result.success {
                    let parsed_stdout = serde_json::from_str::<Value>(&result.stdout)
                        .unwrap_or_else(|_| json!({ "stdout": result.stdout }));
                    Ok(ToolOutput::success(json!({
                        "binary_path": result.binary_path,
                        "stdout": parsed_stdout,
                        "stderr": result.stderr,
                        "exit_code": result.exit_code,
                    })))
                } else {
                    Ok(ToolOutput::non_retryable_error(
                        format!(
                            "binary exited with code {}: {}",
                            result.exit_code, result.stderr
                        ),
                        ToolErrorCategory::Execution,
                    ))
                }
            }
            Err(error) => Ok(ToolOutput::non_retryable_error(
                error.to_string(),
                ToolErrorCategory::Execution,
            )),
        }
    }
}

#[async_trait]
impl Tool for BinarySkillReadTool {
    fn name(&self) -> &str {
        "binary_skill_read"
    }

    fn description(&self) -> &str {
        "Read the current Cargo.toml, main.rs, and SKILL.md for a generated binary skill."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: BinarySkillReadInput = serde_json::from_value(input)?;
        if let Some(message) = check_build_security(
            self.security_gate.as_deref(),
            self.name(),
            "read",
            &params.id,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::non_retryable_error(
                message,
                ToolErrorCategory::Auth,
            ));
        }

        match read_skill_project(&params.id) {
            Ok(result) => Ok(ToolOutput::success(json!({
                "skill_dir": result.skill_dir,
                "artifact_path": result.artifact_path,
                "manifest_path": result.manifest_path,
                "source_path": result.source_path,
                "skill_markdown_path": result.skill_markdown_path,
                "cargo_toml": result.cargo_toml,
                "main_rs": result.main_rs,
                "skill_markdown": result.skill_markdown,
            }))),
            Err(error) => Ok(ToolOutput::non_retryable_error(
                error.to_string(),
                ToolErrorCategory::Execution,
            )),
        }
    }
}

#[async_trait]
impl Tool for BinarySkillUpdateTool {
    fn name(&self) -> &str {
        "binary_skill_update"
    }

    fn description(&self) -> &str {
        "Update Cargo.toml, main.rs, and/or SKILL.md for an existing binary skill."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "cargo_toml": { "type": "string" },
                "main_rs": { "type": "string" },
                "skill_markdown": { "type": "string" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        let params: BinarySkillUpdateInput = serde_json::from_value(input)?;
        if let Some(message) = check_build_security(
            self.security_gate.as_deref(),
            self.name(),
            "update",
            &params.id,
            self.agent_id.as_deref(),
            self.task_id.as_deref(),
        )
        .await?
        {
            return Ok(ToolOutput::non_retryable_error(
                message,
                ToolErrorCategory::Auth,
            ));
        }

        match update_skill_project(&UpdateSkillProjectOptions {
            id: params.id,
            cargo_toml: params.cargo_toml,
            main_rs: params.main_rs,
            skill_markdown: params.skill_markdown,
        }) {
            Ok(result) => Ok(ToolOutput::success(json!({
                "skill_dir": result.skill_dir,
                "artifact_path": result.artifact_path,
                "manifest_path": result.manifest_path,
                "source_path": result.source_path,
                "skill_markdown_path": result.skill_markdown_path,
                "cargo_toml": result.cargo_toml,
                "main_rs": result.main_rs,
                "skill_markdown": result.skill_markdown,
            }))),
            Err(error) => Ok(ToolOutput::non_retryable_error(
                error.to_string(),
                ToolErrorCategory::Execution,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use restflow_ai::llm::{MockLlmClient, MockStep};
    use restflow_ai::tools::ToolRegistry;
    use restflow_ai::{AgentConfig, AgentExecutor};
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[tokio::test]
    async fn binary_skill_new_tool_accepts_custom_files() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let tool = BinarySkillNewTool::new();
        let output = tool
            .execute(json!({
                "id": "pdf-reader",
                "cargo_toml": "[package]\nname = \"pdf-reader\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde_json = \"1\"\n",
                "main_rs": "fn main() { println!(\"{\\\"ok\\\":true}\"); }\n",
                "skill_markdown": "# PDF Reader\n"
            }))
            .await
            .expect("tool execution");

        assert!(output.success);
        let skill_dir = temp.path().join("skills").join("pdf-reader");
        assert!(skill_dir.join("Cargo.toml").exists());
        assert!(skill_dir.join("src").join("main.rs").exists());
        assert!(skill_dir.join("SKILL.md").exists());

        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[tokio::test]
    async fn binary_skill_build_and_run_tools_round_trip() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };
        unsafe {
            std::env::set_var(
                "RESTFLOW_BUILD_CARGO",
                std::env::var("CARGO").expect("cargo env"),
            )
        };

        let new_tool = BinarySkillNewTool::new();
        let build_tool = BinarySkillBuildTool::new();
        let run_tool = BinarySkillRunTool::new();

        let created = new_tool
            .execute(json!({ "id": "echo-skill" }))
            .await
            .expect("create skill");
        assert!(created.success);

        let built = build_tool
            .execute(json!({ "id": "echo-skill" }))
            .await
            .expect("build skill");
        assert!(built.success);

        let ran = run_tool
            .execute(json!({
                "id": "echo-skill",
                "input": { "hello": "world" }
            }))
            .await
            .expect("run skill");
        assert!(ran.success);
        assert!(
            ran.result.to_string().contains("hello"),
            "expected stdout payload to round-trip JSON input"
        );

        unsafe { std::env::remove_var("RESTFLOW_BUILD_CARGO") };
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[tokio::test]
    async fn binary_skill_build_tool_surfaces_compile_failure_as_tool_error() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };
        unsafe {
            std::env::set_var(
                "RESTFLOW_BUILD_CARGO",
                std::env::var("CARGO").expect("cargo env"),
            )
        };

        let new_tool = BinarySkillNewTool::new();
        let build_tool = BinarySkillBuildTool::new();

        let created = new_tool
            .execute(json!({
                "id": "broken-skill",
                "main_rs": "fn main() { let _ = ; }\n"
            }))
            .await
            .expect("create skill");
        assert!(created.success);

        let built = build_tool
            .execute(json!({ "id": "broken-skill" }))
            .await
            .expect("build skill");
        assert!(!built.success);
        assert!(
            built
                .error
                .as_deref()
                .is_some_and(|message| message.contains("build failed")),
            "expected structured tool error for compile failure"
        );

        unsafe { std::env::remove_var("RESTFLOW_BUILD_CARGO") };
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[tokio::test]
    async fn binary_skill_read_and_update_round_trip() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let new_tool = BinarySkillNewTool::new();
        let read_tool = BinarySkillReadTool::new();
        let update_tool = BinarySkillUpdateTool::new();

        let created = new_tool
            .execute(json!({
                "id": "editable-skill",
                "main_rs": "fn main() { let _ = ; }\n"
            }))
            .await
            .expect("create skill");
        assert!(created.success);

        let read = read_tool
            .execute(json!({ "id": "editable-skill" }))
            .await
            .expect("read skill");
        assert!(read.success);
        assert!(
            read.result["main_rs"]
                .as_str()
                .unwrap()
                .contains("let _ = ;")
        );

        let updated = update_tool
            .execute(json!({
                "id": "editable-skill",
                "cargo_toml": "[package]\nname = \"editable-skill\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\n",
                "main_rs": "use serde::Deserialize;\n#[derive(Deserialize)]\nstruct Input { value: String }\nfn main() { println!(\"{}\", serde_json::json!({\"ok\": true})); }\n",
                "skill_markdown": "# Editable Skill\n"
            }))
            .await
            .expect("update skill");
        assert!(updated.success);
        assert!(
            updated.result["cargo_toml"]
                .as_str()
                .unwrap()
                .contains("serde =")
        );

        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[tokio::test]
    async fn real_agent_loop_can_create_binary_skill() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };

        let llm = Arc::new(MockLlmClient::from_steps(
            "mock-model",
            vec![
                MockStep::tool_call(
                    "call-1",
                    "binary_skill_new",
                    json!({
                        "id": "agent-created-skill",
                        "skill_markdown": "# Agent Created Skill\n",
                        "main_rs": "fn main() { println!(\"{\\\"ok\\\":true}\"); }\n"
                    }),
                ),
                MockStep::text("done"),
            ],
        ));
        let mut tools = ToolRegistry::new();
        tools.register(BinarySkillNewTool::new());
        let executor = AgentExecutor::new(llm, Arc::new(tools));

        let result = executor
            .run(AgentConfig::new("Create a new binary skill"))
            .await
            .expect("agent execution");
        assert!(result.success);

        let skill_dir = temp.path().join("skills").join("agent-created-skill");
        assert!(skill_dir.exists());
        assert!(skill_dir.join("Cargo.toml").exists());
        assert!(skill_dir.join("src").join("main.rs").exists());
        assert!(skill_dir.join("SKILL.md").exists());

        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }

    #[tokio::test]
    async fn real_agent_loop_can_repair_build_failure_until_success() {
        let _lock = env_lock();
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("RESTFLOW_DIR", temp.path()) };
        unsafe {
            std::env::set_var(
                "RESTFLOW_BUILD_CARGO",
                std::env::var("CARGO").expect("cargo env"),
            )
        };

        let llm = Arc::new(MockLlmClient::from_steps(
            "mock-model",
            vec![
                MockStep::tool_call(
                    "call-new",
                    "binary_skill_new",
                    json!({
                        "id": "repairable-skill",
                        "cargo_toml": "[package]\nname = \"repairable-skill\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde_json = \"1\"\n",
                        "main_rs": "use serde::Deserialize;\n#[derive(Deserialize)]\nstruct Input { value: String }\nfn main() { println!(\"{}\", serde_json::json!({\"ok\": true})); }\n",
                        "skill_markdown": "# Repairable Skill\n"
                    }),
                ),
                MockStep::tool_call(
                    "call-build-1",
                    "binary_skill_build",
                    json!({ "id": "repairable-skill" }),
                ),
                MockStep::tool_call(
                    "call-read",
                    "binary_skill_read",
                    json!({ "id": "repairable-skill" }),
                ),
                MockStep::tool_call(
                    "call-update",
                    "binary_skill_update",
                    json!({
                        "id": "repairable-skill",
                        "cargo_toml": "[package]\nname = \"repairable-skill\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\n",
                        "main_rs": "use serde::Deserialize;\n#[derive(Deserialize)]\nstruct Input { value: String }\nfn main() { println!(\"{}\", serde_json::json!({\"ok\": true, \"value\": \"fixed\"})); }\n"
                    }),
                ),
                MockStep::tool_call(
                    "call-build-2",
                    "binary_skill_build",
                    json!({ "id": "repairable-skill" }),
                ),
                MockStep::tool_call(
                    "call-run",
                    "binary_skill_run",
                    json!({ "id": "repairable-skill", "input": { "value": "hello" } }),
                ),
                MockStep::text("done"),
            ],
        ));

        let mut tools = ToolRegistry::new();
        tools.register(BinarySkillNewTool::new());
        tools.register(BinarySkillBuildTool::new());
        tools.register(BinarySkillReadTool::new());
        tools.register(BinarySkillUpdateTool::new());
        tools.register(BinarySkillRunTool::new());
        let executor = AgentExecutor::new(llm, Arc::new(tools));

        let result = executor
            .run(AgentConfig::new("Create, repair, and run a binary skill"))
            .await
            .expect("agent execution");
        assert!(result.success);

        let binary = temp
            .path()
            .join("skills")
            .join("repairable-skill")
            .join("bin")
            .join("debug")
            .join("repairable-skill");
        assert!(binary.exists());

        unsafe { std::env::remove_var("RESTFLOW_BUILD_CARGO") };
        unsafe { std::env::remove_var("RESTFLOW_DIR") };
    }
}
