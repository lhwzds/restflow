use super::*;

#[test]
fn test_build_with_batch_registers_batch_and_preserves_tools() {
    let registry = ToolRegistryBuilder::new()
        .with_bash(BashConfig::default())
        .build_with_batch();
    assert!(registry.has("batch"));
    assert!(registry.has("bash"));
}
