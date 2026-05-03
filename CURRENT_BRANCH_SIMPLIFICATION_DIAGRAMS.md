# Current Branch Simplification Diagrams

## Current Branch Graph

This is the current shape of the `codex/tui-system-simplification` branch. The
branch is already moving toward a read-only skill surface, but the files still
mix three concepts:

- bundled systemskills
- executable skills discovered through `skrun`
- legacy storage-backed user skills and mutation commands

```mermaid
flowchart TD
    subgraph CLIENTS["Client surfaces"]
        TUI["restflow-tui\nstate.rs / reducer.rs / shell.rs / controller.rs\n/skill picker now says View skills"]
        CLI["restflow-cli\ncli.rs hides mutable skill commands\ncommands/skill.rs still keeps handlers"]
    end

    subgraph CORE_SERVICES["restflow-core service/catalog layer"]
        SkillSvc["services/skills.rs\nlist/get still merges system + skrun + legacy storage"]
        ProviderFile["services/adapters/skill_provider.rs\nSystemSkillProvider\nSkrunSkillProvider\nSkillStorageProvider\nCompositeSkillProvider"]
        SkillFiles["skill_files.rs\nbundled systemskills"]
        Storage["storage::skill::SkillStorage\nlegacy persisted skills"]
        SkrunCatalog["external skrun CLI\nskrun skill list/show"]
    end

    subgraph CORE_RUNTIME["restflow-core runtime layer"]
        ToolAssembly["runtime/agent/tools/mod.rs\nservices/tool_registry/assembly.rs\nsession_execution.rs"]
        Activation["runtime/agent/tools/skill_activation.rs\n@skill mention activation"]
    end

    subgraph TOOLS["restflow-tools implementations"]
        LoadSkill["impls/load_skill.rs\nload-only list/read"]
        SkrunTool["impls/skrun.rs\nrun_skill executes skrun skill run"]
    end

    TUI --> SkillSvc
    CLI --> SkillSvc
    SkillSvc --> SkillFiles
    SkillSvc --> SkrunCatalog
    SkillSvc -. compatibility .-> Storage

    ToolAssembly --> ProviderFile
    ProviderFile --> SkillFiles
    ProviderFile --> SkrunCatalog
    ProviderFile -. old provider still exists .-> Storage
    ToolAssembly --> LoadSkill
    ToolAssembly --> SkrunTool
    Activation --> ToolAssembly
    LoadSkill --> ProviderFile
    SkrunTool --> SkrunCatalog
```

## Current File Architecture

```mermaid
flowchart LR
    subgraph CLI["crates/restflow-cli"]
        CliRs["src/cli.rs\nhidden mutable skill subcommands"]
        SkillCmd["src/commands/skill.rs\nlist/show plus legacy create/update/delete/import/export/search/install handlers"]
        Manpage["man/restflow-skill.1\nvisible help removes mutable commands"]
    end

    subgraph TUI["crates/restflow-tui"]
        TuiState["src/state.rs\nSkillManager selection"]
        TuiReducer["src/reducer.rs\nskill overlay actions"]
        TuiShell["src/shell.rs\nskill picker rendering"]
        TuiController["src/controller.rs\nloads skill detail actions"]
        TuiSlash["src/slash_command.rs\n/skill copy"]
    end

    subgraph CORE["crates/restflow-core"]
        Services["src/services/skills.rs\nservice-level effective catalog"]
        Provider["src/services/adapters/skill_provider.rs\nmixed provider implementations"]
        RuntimeTools["src/runtime/agent/tools/mod.rs\nruntime registry and default tools"]
        SkillActivation["src/runtime/agent/tools/skill_activation.rs\nmention activation"]
        SessionExec["src/runtime/task_runtime/executor/session_execution.rs\nmentioned skill resolution"]
        RegistryAssembly["src/services/tool_registry/assembly.rs\nregistry construction"]
    end

    subgraph TOOL_IMPLS["crates/restflow-tools"]
        LoadSkillTool["src/impls/load_skill.rs\nread/list"]
        RunSkillTool["src/impls/skrun.rs\nrun executable skill"]
        ImplMod["src/impls/mod.rs\nexports skrun tool"]
    end

    CliRs --> SkillCmd
    SkillCmd --> Services
    TuiState --> TuiReducer
    TuiReducer --> TuiController
    TuiController --> Services
    TuiShell --> TuiState
    TuiSlash --> TuiReducer

    Services --> Provider
    RuntimeTools --> Provider
    SessionExec --> Provider
    RegistryAssembly --> Provider
    RuntimeTools --> LoadSkillTool
    RuntimeTools --> RunSkillTool
    ImplMod --> RunSkillTool
```

## Target Simplified Graph

The simplified graph should have one catalog boundary and one execution
boundary. TUI and CLI only view or request through the daemon/core layer.

```mermaid
flowchart TD
    subgraph CLIENTS["Thin client surfaces"]
        TUI["TUI /skill\nview, detail, @skill mention"]
        CLI["CLI skill\nlist/show visible\nlegacy mutation hidden or isolated"]
    end

    subgraph CORE_CATALOG["restflow-core: EffectiveSkillCatalog"]
        Effective["EffectiveSkillCatalog\nsingle read API for runtime-visible skills"]
        System["SystemSkillCatalog\nbundled read-only systemskills"]
        Skrun["RunSkillCatalog\nread-only skrun discovery"]
        Legacy["LegacySkillCompatibility\noptional read/migration path only"]
    end

    subgraph CORE_RUNTIME["restflow-core: runtime assembly"]
        Runtime["ToolRegistry assembly\nuses EffectiveSkillCatalog"]
        Mention["@skill activation\nread first, suggested tools only by policy"]
    end

    subgraph TOOL_BOUNDARY["restflow-tools: tool implementations"]
        LoadSkill["load_skill\nlist/read only"]
        RunSkrun["run_skill\nexecute skrun skill with JSON input"]
    end

    subgraph EXTERNAL["External process boundary"]
        SkrunCli["skrun CLI\nskill list/show/run"]
    end

    TUI --> Effective
    CLI --> Effective

    Effective --> System
    Effective --> Skrun
    Effective -. compatibility only .-> Legacy

    Runtime --> Effective
    Runtime --> LoadSkill
    Runtime --> RunSkrun
    Mention --> Runtime

    LoadSkill --> Effective
    Skrun --> SkrunCli
    RunSkrun --> SkrunCli
```

## More Aggressive Storage-Free Target

If the product goal is to simplify RestFlow into only `agent + skill run + TUI`,
then the target graph is smaller than the daemon-centric architecture. In this
shape, RestFlow becomes an ephemeral terminal agent runner. `skrun` owns skill
installation/catalog/execution, and RestFlow does not own durable sessions,
tasks, runs, secrets, memory, or skill storage.

```mermaid
flowchart TD
    TUI["restflow-tui\ncomposer, transcript, model picker,\nview-only skill picker"]

    Agent["Agent Runtime\nprompt assembly, model call,\ntool loop, streaming events"]

    SkillRun["Skill Run Boundary\nload_skill reads discovered skill docs\nrun_skill invokes executable skill"]

    Tools["Tool Set\nbash/file/http/subagent/etc.\nonly if explicitly kept"]

    Skrun["external skrun\nskill list/show/run\nowns installed skills"]

    ProviderAuth["Provider auth/config\nenv or provider-native CLI config\nno RestFlow DB secrets"]

    TUI --> Agent
    Agent --> ProviderAuth
    Agent --> SkillRun
    Agent --> Tools
    SkillRun --> Skrun
```

In this model, the dependency shape becomes:

```mermaid
flowchart LR
    TUI["restflow-tui"] --> AGENT["restflow-agent/restflow-ai"]
    AGENT --> TOOLS["restflow-tools"]
    TOOLS --> SKRUN["external skrun CLI"]
    AGENT --> MODELS["restflow-models/restflow-traits"]

    AGENT -. no DB .-> STORAGE["restflow-storage removed from active path"]
    TUI -. no daemon persistence .-> STORAGE
    TOOLS -. no skill catalog DB .-> STORAGE
```

What disappears from active product scope:

- saved chat sessions and resume history
- durable Task / Run scheduling and history
- redb-backed tool traces and execution inspection
- RestFlow-managed secrets and auth profiles
- RestFlow-managed skill create/update/delete/import/install
- memory storage

What remains:

- TUI conversation surface
- one in-memory agent execution loop
- model selection/auth through environment or provider-native config
- `load_skill` as a read-only view over `skrun skill list/show`
- `run_skill` as the executable skill runner
- optional non-persistent tool execution

## Target File Architecture

This is the target shape to simplify toward. It can be reached incrementally;
the important part is to isolate catalog ownership from runtime tool execution.

```mermaid
flowchart LR
    subgraph CORE["crates/restflow-core"]
        CatalogMod["src/services/skill_catalog/mod.rs\nEffectiveSkillCatalog facade"]
        CatalogSystem["src/services/skill_catalog/system.rs\nsystemskill read provider"]
        CatalogSkrun["src/services/skill_catalog/skrun.rs\nskrun list/show provider"]
        CatalogLegacy["src/services/skill_catalog/legacy_storage.rs\ncompatibility or migration only"]
        ServicesSkills["src/services/skills.rs\nthin service wrapper"]
        RuntimeRegistry["src/runtime/agent/tools/mod.rs\nregistry wiring only"]
        RuntimeActivation["src/runtime/agent/tools/skill_activation.rs\nactivation policy only"]
    end

    subgraph TOOLS["crates/restflow-tools"]
        LoadSkillRs["src/impls/load_skill.rs\nread/list only"]
        SkrunRs["src/impls/skrun.rs\nrun only"]
    end

    subgraph TUI["crates/restflow-tui"]
        TuiSkillView["state/reducer/controller/shell\nview-only skill picker"]
    end

    subgraph CLI["crates/restflow-cli"]
        CliSkill["cli.rs + commands/skill.rs\nvisible list/show\nhidden compatibility commands"]
    end

    CatalogMod --> CatalogSystem
    CatalogMod --> CatalogSkrun
    CatalogMod -. optional .-> CatalogLegacy
    ServicesSkills --> CatalogMod
    RuntimeRegistry --> CatalogMod
    RuntimeActivation --> CatalogMod
    RuntimeRegistry --> LoadSkillRs
    RuntimeRegistry --> SkrunRs
    TuiSkillView --> ServicesSkills
    CliSkill --> ServicesSkills
```

## Dependency Rules After Simplification

```mermaid
flowchart TD
    TUI["restflow-tui"] --> CORE["restflow-core services"]
    CLI["restflow-cli"] --> CORE
    CORE --> TOOLS["restflow-tools"]
    CORE --> TRAITS["restflow-traits"]
    TOOLS --> TRAITS

    CORE -. no direct UI state .-> TUI
    TOOLS -. no daemon/runtime ownership .-> CORE
    CLI -. no direct storage skill mutation as primary UX .-> STORAGE["storage skills"]
    TUI -. no direct storage writes .-> STORAGE
```

Required invariants:

- `restflow-tui` and `restflow-cli` remain client surfaces.
- `restflow-core` owns catalog composition and runtime assembly.
- `restflow-tools` owns `load_skill` and `run_skill` behavior only.
- Legacy storage-backed skills do not become model-visible tool names.
- `skrun` is an external process boundary, not daemon-owned persistence.

For the aggressive storage-free target, replace those invariants with:

- `restflow-tui` calls an in-process or lightweight agent runtime.
- The agent runtime is ephemeral unless the user explicitly exports output.
- `skrun` owns skill persistence and executable skill lifecycle.
- RestFlow does not create, install, store, or mutate skills.
- RestFlow does not promise resume/history/task/run inspection.

## Simplification Moves

1. Rename the current mixed `skill_provider.rs` conceptually into a catalog
   boundary, then split it only if the file keeps growing.
2. Keep `services/skills.rs` as the API wrapper, not the place where every
   catalog source is manually merged.
3. Keep mutable skill CLI handlers hidden and mark them as compatibility until a
   cleanup/migration branch removes or replaces them.
4. Keep TUI `/skill` view-only unless a separate product decision reintroduces
   install or edit flows.
5. Keep `load_skill` and `run_skill` separate: reading a skill is not running a
   skill.

## Storage-Free Migration Moves

If we choose the aggressive `agent + skill run + TUI` direction, the migration
should be explicit:

1. First remove storage from the skill path only.
   - `load_skill` reads systemskill/skrun catalog only.
   - `run_skill` executes through `skrun`.
   - Hidden legacy skill mutation commands remain temporarily but are no longer
     part of the runtime path.

2. Then decide whether RestFlow still has daemon/session/task ambitions.
   - If yes, keep storage for sessions/tasks and stop at the target simplified
     graph above.
   - If no, delete or isolate task/run/session/memory/auth surfaces behind a
     separate legacy feature or branch.

3. Finally collapse the active architecture to:
   - `restflow-tui`
   - agent runtime
   - `restflow-tools`
   - external `skrun`

This is a product cut, not only a code refactor.
