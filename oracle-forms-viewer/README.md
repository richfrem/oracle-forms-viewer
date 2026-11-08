# oracle-forms-viewer

[![CI](https://github.com/richfrem/oracle-forms-viewer/actions/workflows/ci.yml/badge.svg)](https://github.com/richfrem/oracle-forms-viewer/actions)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

A high-performance, pure-native Rust toolchain, CLI, and standalone web viewer for Oracle Forms modules (`.fmb`, `.mmb`, `.olb`, Forms XML exports, and `.pld` libraries).

Zero JVM, zero Node.js, zero Python, and zero Oracle runtime/DLL dependencies.

---

## Workspace Architecture

```text
oracle-forms-viewer/
├── Cargo.toml                            # Virtual workspace definition
├── crates/
│   ├── oracle-forms-core/                # Shared domain models, sidecars, and geometry
│   ├── oracle-forms-xml/                 # Streaming quick-xml parser for Forms2XML exports & PLD
│   ├── oracle-forms-binary/              # ROS 60050 stream & heuristic binary extractor
│   ├── oracle-forms-cli/                 # Standalone CLI inspect & dump-sql tool
│   └── oracle-forms-server/              # Axum HTTP server with embedded UI assets (rust-embed)
├── ui/                                   # Vanilla HTML/CSS/JS frontend
├── samples/                              # Curated example forms (XML, PLD, and binary FMB)
└── tests/                                # Multi-crate integration & differential test suites
```

---

## Features

- **Blazing Fast XML Parsing**: Streaming StAX-like reader using `quick-xml` parsing complex modules in sub-milliseconds with precise byte and line mapping.
- **Direct Binary ROS Extraction**: Recovers readable PL/SQL triggers, program units, bind variables, blocks, items, record groups, and canvas layouts directly from binary `.fmb` files without Oracle installation or Windows DLLs.
- **Single-Binary Web Viewer**: Zero-dependency executable serving an interactive, pan/zoom SVG visual layout editor, object navigator, and PL/SQL code viewer directly from embedded memory (`rust-embed`).
- **Clean CLI**: Inspect module hierarchies and dump clean PL/SQL sidecars directly to disk.

---

## Quickstart

### Prerequisites

- [Rust 1.80+](https://rustup.rs/)

### Build & Run Test Suite

```bash
cargo check
cargo test
```

### CLI Usage

```bash
# Inspect an XML export sample
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml

# Output full module index as JSON
cargo run -p oracle-forms-cli -- inspect samples/orders_fmb.xml --json

# Inspect a binary .fmb sample directly
cargo run -p oracle-forms-cli -- inspect samples/simple_module.fmb

# Dump all PL/SQL blocks (triggers, procedures) to disk
cargo run -p oracle-forms-cli -- dump-sql samples/orders_fmb.xml -o ./extracted-sql
```

### Standalone Web Viewer

```bash
# Run viewer on default port 7070
cargo run -p oracle-forms-server

# Or specify custom port
PORT=8080 cargo run -p oracle-forms-server
```

Open `http://127.0.0.1:7070` in any modern web browser.

---

## License

Dual-licensed under either:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
