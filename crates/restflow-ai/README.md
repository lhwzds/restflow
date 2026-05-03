# RestFlow AI

RestFlow AI is the Rust agent framework crate. It owns the ReAct executor,
provider clients, tool interfaces, and the optional Python native extension.

Build the Python module with maturin from this directory. The pyproject enables
the Python SDK feature and PyO3's extension-module dependency feature for
packaging; Rust tests should use `--features python` and must not enable
`pyo3/extension-module`.

```bash
maturin develop
```
