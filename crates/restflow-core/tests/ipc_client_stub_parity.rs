use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn parse_method_names(source: &str, prefix: &str) -> BTreeSet<String> {
    source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix(prefix)?;
            let (name, _) = rest.split_once('(')?;
            Some(name.trim().to_string())
        })
        .collect()
}

fn load_source(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", path.display());
    })
}

#[test]
fn non_unix_stub_covers_session_client_methods() {
    let ipc_client_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/daemon/ipc_client");
    let unix_methods = parse_method_names(
        &load_source(&ipc_client_dir.join("sessions.rs")),
        "pub async fn ",
    );
    let unsupported_methods = parse_method_names(
        &load_source(&ipc_client_dir.join("unsupported.rs")),
        "fn ",
    );

    let missing: Vec<_> = unix_methods
        .difference(&unsupported_methods)
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "unsupported IPC client is missing session methods: {}",
        missing.join(", ")
    );
}
