# Contributing to AiMesh

Thank you for your interest in contributing to AiMesh. This guide will help you get started.

## Code of Conduct

By participating in this project, you agree to maintain a respectful and inclusive environment for everyone.

## How to Contribute

### Reporting Issues

- Check existing issues before creating a new one
- Use a clear, descriptive title
- Provide detailed steps to reproduce the issue
- Include your environment details (OS, Rust version, etc.)

### Submitting Pull Requests

1. Fork the repository
2. Create a feature branch from `main`:
   ```bash
   git checkout -b feature/your-feature-name
   ```
3. Make your changes following the coding standards
4. Write or update tests as needed
5. Ensure all tests pass:
   ```bash
   cargo test
   ```
6. Commit your changes with a descriptive message:
   ```bash
   git commit -m "Add feature: description of changes"
   ```
7. Push to your fork:
   ```bash
   git push origin feature/your-feature-name
   ```
8. Open a pull request against the `main` branch

### Coding Standards

- Follow Rust best practices and idioms
- Use `rustfmt` for code formatting
- Run `clippy` and address warnings:
  ```bash
  cargo clippy
  ```
- Write documentation for public APIs
- Add tests for new functionality
- Keep commits focused and atomic

### Documentation

- Update README.md if adding new features
- Add inline documentation using `///` comments
- Update docs/ for architectural changes

## Development Setup

1. Install Rust (1.70 or later):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Clone the repository:
   ```bash
   git clone https://github.com/YASSERRMD/AiMesh.git
   cd AiMesh
   ```

3. Build the project:
   ```bash
   cargo build
   ```

4. Run tests:
   ```bash
   cargo test
   ```

## Project Structure

```
AiMesh/
├── src/
│   ├── lib.rs           # Library entry point
│   ├── main.rs          # Binary entry point
│   ├── protocol/        # Message protocol (Protobuf)
│   ├── routing/         # Cost-aware routing engine
│   ├── storage/         # Barq-DB/GraphDB integration
│   ├── transport/       # QUIC transport layer
│   ├── orchestration/   # Scatter-gather workflows
│   ├── dedup/           # Semantic deduplication
│   └── observability/   # Metrics and tracing
├── proto/               # Protocol Buffer definitions
├── tests/               # Integration tests
├── docs/                # Documentation
└── examples/            # Example code
```

## Questions

For questions or discussions, please open a GitHub issue.

## License

By contributing to AiMesh, you agree that your contributions will be licensed under the MIT License.
