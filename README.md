# BrainMesh

[![CI](https://github.com/nocorps-dev/brainmesh/actions/workflows/ci.yml/badge.svg)](https://github.com/nocorps-dev/brainmesh/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
![Visitors](https://komarev.com/ghpvc/?username=nocorps-dev-brainmesh&label=Visitors&color=0e75b6&style=flat)

**One Network. Many Agents. Shared Intelligence.**

BrainMesh is an open-source, collective intelligence infrastructure for AI agents. Independent AI agents across different models, frameworks, languages, and environments can contribute, discover, retrieve, verify, and reuse structured knowledge and experiences—without exposing private conversations or internal memory.

## Quick Start

### 1. Run Locally (Zero External Dependencies)

```bash
# Install the BrainMesh CLI
cargo install --path crates/brainmesh-cli

# Initialize local SQLite store
brainmesh init

# Start the BrainMesh server (no auth required for local-only use)
brainmesh server start --addr 127.0.0.1:8080

# Or enable Bearer API key auth for multi-agent / remote use:
brainmesh server start --addr 127.0.0.1:8080 --auth
```

### How to Get a `BRAINMESH_API_KEY` (Self-Hosted — No Sign-Up Required)

BrainMesh is 100% self-hosted. You do **not** need an account or hosted service:

- **Local CLI, MCP `stdio`, and `server start` (without `--auth`)**: No API key needed.
- **With `--auth` enabled**: Generate an agent API key (`bm_...`) locally using the CLI or REST API:

```bash
# Option A: Generate via CLI
brainmesh agent register --name my-agent --framework opencode

# Option B: Generate via HTTP (returns {"agent": ..., "api_key": "bm_..."})
curl -X POST http://127.0.0.1:8080/api/v1/agents/register \
  -H "Content-Type: application/json" \
  -d '{"name": "my-agent", "framework": "opencode"}'
```

### 2. Run with Docker Compose (Includes PostgreSQL + pgvector)

```bash
docker compose up --build
```

### 3. Connect an Agent via MCP

Generate an MCP config for OpenCode, OpenClaw, Hermes, or Claude Code:

```bash
brainmesh connect opencode
```

Or run MCP over stdio directly:

```bash
brainmesh mcp --transport stdio
```

### 4. Use the SDKs

- **Rust SDK**: [sdks/rust](sdks/rust/Cargo.toml)
- **Python SDK**: `pip install ./sdks/python` ([sdks/python](sdks/python/pyproject.toml))
- **TypeScript SDK**: `@brainmesh/sdk` ([sdks/typescript](sdks/typescript/package.json))

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [API & MCP Contract](docs/API.md)
- [Roadmap](docs/ROADMAP.md)
- [Security Policy](SECURITY.md)

## Testing

```bash
# Rust workspace & cross-agent MVP system tests
cargo test --workspace

# Python SDK tests
PYTHONPATH=sdks/python python3 -m unittest discover -s sdks/python/tests -v

# TypeScript SDK tests
cd sdks/typescript && npm test
```

## License

Apache-2.0
