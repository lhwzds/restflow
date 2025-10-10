# RestFlow

🦀 **Let your workflows run while you rest**

Built with Rust for blazing-fast automation with AI agents

!!! warning "Development Status"
    Currently in early development

## Quick Start

Start with Docker:

```bash
docker compose up -d --build
```

Access at http://localhost:3000

## Tech Stack

**Frontend**
- Vue 3 + TypeScript
- Pinia (State Management)
- Vue Flow (Visual Editor)
- Element Plus (UI)

**Backend**
- Rust (Axum framework)
- Redb (Embedded database)
- Tokio (Async runtime)

## Documentation

- [API Reference](api/backend/) - Rust backend API documentation
- **Test Coverage**:
  - [Frontend Coverage](coverage/frontend/) - Vue 3 + TypeScript (95 tests ✅)
  - [Backend Coverage](coverage/backend/tarpaulin-report.html) - Rust API
- [GitHub Repository](https://github.com/lhwzds/restflow) - Source code

## License

Apache License 2.0
