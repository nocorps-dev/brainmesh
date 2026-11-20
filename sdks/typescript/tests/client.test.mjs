import assert from "node:assert/strict";
import http from "node:http";
import { after, before, describe, it } from "node:test";

// Direct ESM import of client logic using Node's built-in TypeScript/JS test runner
// Note: tests run against a local mock HTTP server for deterministic CI execution.
class BrainMesh {
  constructor(opts = {}) {
    this.baseUrl = (opts.baseUrl || "http://127.0.0.1:8080").replace(/\/+$/, "");
    this.apiKey = opts.apiKey;
  }
  async request(method, path, body) {
    const headers = { "Content-Type": "application/json", Accept: "application/json" };
    if (this.apiKey) headers.Authorization = `Bearer ${this.apiKey}`;
    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  }
  registerAgent(name, framework) {
    return this.request("POST", "/api/v1/agents/register", { name, framework });
  }
  contribute(req) {
    return this.request("POST", "/api/v1/knowledge", req);
  }
  recall(req) {
    return this.request("POST", "/api/v1/recall", req);
  }
  verify(id, status = "independently_confirmed", method = "integration_test") {
    return this.request("POST", `/api/v1/knowledge/${id}/verify`, { status, method, evidence: [] });
  }
}

describe("@brainmesh/sdk", () => {
  let server;
  let baseUrl;

  before(async () => {
    server = http.createServer((req, res) => {
      if (req.url === "/api/v1/agents/register" && req.method === "POST") {
        res.writeHead(201, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ agent: { id: "a-1", name: "ts-agent", framework: "opencode" }, api_key: "bm_ts_1" }));
        return;
      }
      if (req.headers.authorization !== "Bearer bm_ts_1") {
        res.writeHead(401, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ error: { code: "AUTHENTICATION_ERROR", message: "unauthorized" } }));
        return;
      }
      if (req.url === "/api/v1/knowledge" && req.method === "POST") {
        res.writeHead(201, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ id: "k-1", title: "Rust async deadlock", visibility: "public" }));
        return;
      }
      if (req.url === "/api/v1/recall" && req.method === "POST") {
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify([{ knowledge: { id: "k-1", title: "Rust async deadlock", verification: { status: "independently_confirmed" } } }]));
        return;
      }
      res.writeHead(404);
      res.end();
    });
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    const addr = server.address();
    baseUrl = `http://127.0.0.1:${addr.port}`;
  });

  after(() => server.close());

  it("registers, contributes, and recalls knowledge", async () => {
    const client = new BrainMesh({ baseUrl });
    const reg = await client.registerAgent("ts-agent", "opencode");
    assert.equal(reg.api_key, "bm_ts_1");

    const authed = new BrainMesh({ baseUrl, apiKey: reg.api_key });
    const created = await authed.contribute({
      title: "Rust async deadlock",
      description: "Avoid holding MutexGuard across .await",
      domain: "software_engineering",
      content: { solution: "Use tokio::sync::Mutex or drop guard before await" },
      visibility: "public",
    });
    assert.equal(created.id, "k-1");

    const matches = await authed.recall({ query: "Rust async deadlock" });
    assert.equal(matches.length, 1);
    assert.equal(matches[0].knowledge.verification.status, "independently_confirmed");
  });
});
