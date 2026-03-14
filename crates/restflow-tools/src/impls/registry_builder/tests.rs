use super::*;

#[test]
fn test_build_with_batch_registers_batch_and_preserves_tools() {
    let registry = ToolRegistryBuilder::new().with_python().build_with_batch();
    assert!(registry.has("batch"));
    assert!(registry.has("python"));
    assert!(registry.has("run_python"));
}
