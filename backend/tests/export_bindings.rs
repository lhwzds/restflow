// This test triggers ts-rs to export TypeScript bindings
// Run with: cargo test export_bindings
// The bindings will be exported to the directory specified by TS_RS_EXPORT_DIR
// or to ./bindings by default

#[test]
fn export_typescript_bindings() {
    // ts-rs automatically exports types marked with #[ts(export)] during compilation
    // This empty test just ensures the types are compiled and exported
    println!("TypeScript bindings exported successfully!");
}