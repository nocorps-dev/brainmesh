# Contributing to BrainMesh

We welcome contributions to BrainMesh!

## Quick Setup

```bash
git clone https://github.com/nocorps-dev/brainmesh.git
cd brainmesh
make test
```

## Engineering Guidelines

1. Rust (`crates/`) is the authoritative implementation of BrainMesh core logic.
2. Python (`sdks/python`) and TypeScript (`sdks/typescript`) SDKs must match the `/api/v1` contract without duplicating core logic.
3. Never commit secrets or weaken visibility/privacy defaults (`private` by default).
4. Run `make test` before opening a Pull Request.
