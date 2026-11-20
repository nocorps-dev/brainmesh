# BrainMesh Development Roadmap

## Phase 1 — Foundation (Implemented)
- Rust Cargo workspace (`brainmesh-core`, `brainmesh-storage`, `brainmesh-api`, `brainmesh-mcp`, `brainmesh-cli`, `brainmesh-sdk`)
- Core data models, schema versioning, and `SecurityPolicy`
- PostgreSQL + pgvector migration schema & SQLite local-first storage backend
- CLI (`init`, `server`, `agent`, `search`, `recall`, `contribute`, `learn`, `knowledge`, `network`, `federation`, `config`, `doctor`, `mcp`)
- Health, readiness, version, and metrics endpoints

## Phase 2 — Knowledge Engine (Implemented)
- 6 knowledge categories (`knowledge`, `experience`, `skill`, `solution`, `discovery`, `lesson`)
- Structured storage, retrieval by ID, version history (`knowledge_versions`)
- Provenance tracking, visibility controls (`private` default), and metadata filtering

## Phase 3 — Recall Engine (Implemented - Hybrid SQLite FTS/Keyword + Ranking)
- Full-text & keyword search, domain/tag/verification/visibility filtering
- Explainable relevance scoring combining text match, verification quality, and freshness
- PostgreSQL + pgvector schema ready for vector similarity

## Phase 4 — Universal Agent Connectivity (Implemented)
- Versioned REST API (`/api/v1`) + OpenAPI 3.1 spec
- MCP server (Stdio + Streamable HTTP)
- Rust SDK, Python SDK (`sdks/python`), TypeScript SDK (`sdks/typescript`)
- Cross-agent OpenCode <-> Hermes Agent MVP demonstration

## Phase 5 — Experience & Learning (Implemented Core Slice)
- Experience submission (`POST /experiences`)
- Sensitive-data redaction, duplicate detection, and structured knowledge extraction (`POST /experiences/{id}/extract`)

## Phase 6 — Trust & Verification (Implemented Core Slice)
- Verification records (`POST /knowledge/{id}/verify`)
- Challenge/dispute records preserving competing claims (`POST /knowledge/{id}/challenge`)
- Verification history (`GET /knowledge/{id}/verification`)

## Phase 7 — Ecosystem & Dashboard (In Progress)
- Framework integrations for LangChain, LangGraph, CrewAI, AutoGen, OpenCode, OpenClaw, Hermes
- Developer Web Dashboard (`apps/dashboard`)

## Phase 8 — Graph & Federation (Core Endpoints Implemented)
- Knowledge relationship graph (`POST /knowledge/{id}/relationships`, `GET /knowledge/{id}/related`)
- Federation node registry and sync status endpoints (`/federation/*`)

## Phase 9 — Scale (Planned)
- Background workers, embedding queue, distributed caching, OpenTelemetry export
