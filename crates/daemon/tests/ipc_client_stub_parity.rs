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
    let ipc_client_source = load_source(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"));
    let unix_methods = parse_method_names(&ipc_client_source, "pub async fn ")
        .into_iter()
        .filter(|name| {
            name.contains("session")
                || matches!(
                    name.as_str(),
                    "count_sessions"
                        | "add_message"
                        | "append_message"
                        | "subscribe_session_events"
                )
        })
        .collect::<BTreeSet<_>>();
    let mut unsupported_methods = parse_method_names(&ipc_client_source, "fn ");
    unsupported_methods.extend(parse_method_names(&ipc_client_source, "pub async fn "));
    unsupported_methods.remove("$name");

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
