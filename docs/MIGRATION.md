# Configuration Migration Guide

RestFlow now unifies configuration under `~/.restflow/`.

## Automatic Migration
Most migrations happen automatically when you run RestFlow:
- Master key is migrated from `secret-master-key.json` to `master.key`
- Auth profiles are migrated from JSON file to the database
- Old TOML config is converted to JSON

## Manual Migration
For explicit migration, run:

```bash
restflow migrate --dry-run # Preview changes
restflow migrate # Execute migration
```

## Old Locations (Can Be Deleted)
After migration, these can be safely removed:
- `~/.config/restflow/` - old CLI configuration
- `~/Library/Application Support/com.restflow.app/` - old Tauri data (macOS)
- `~/.restflow/secret-master-key.json` - old master key format
- `~/.restflow/auth_profiles.json` - old auth profiles
- `~/.restflow/restflow-mcp.db` - old MCP database (if exists)

## New Structure

```
~/.restflow/
├── config.json # All configuration
├── restflow.db # All data (shared by CLI, GUI, MCP)
├── master.key # Encryption key
└── logs/ # Application logs
```

## Environment Variables
- `RESTFLOW_DIR` - override configuration directory
- `RESTFLOW_MASTER_KEY` - provide master key directly (hex or base64)

## Files Changed
| File | Change |
|------|--------|
| `crates/restflow-cli/src/commands/migrate.rs` | New file |
| `crates/restflow-cli/src/commands/mod.rs` | Add migrate module |
| `crates/restflow-cli/src/cli.rs` | Add migrate subcommand args |
| `crates/restflow-cli/src/main.rs` | Wire migrate command |
| `docs/MIGRATION.md` | New documentation |

## Testing

```bash
cargo build -p restflow-cli
./target/debug/restflow migrate --dry-run
./target/debug/restflow migrate
```
