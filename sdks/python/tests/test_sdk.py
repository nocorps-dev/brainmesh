from __future__ import annotations

import json
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer

from brainmesh import BrainMesh, BrainMeshError


class _MockBrainMeshHandler(BaseHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        return

    def do_POST(self) -> None:
        if self.path == "/api/v1/agents/register":
            self._json(201, {
                "agent": {"id": "agent-1", "name": "py-agent", "framework": "langgraph"},
                "api_key": "bm_test_123",
            })
            return

        if self.headers.get("Authorization") != "Bearer bm_test_123":
            self._json(401, {"error": {"code": "AUTHENTICATION_ERROR", "message": "unauthorized"}})
            return

        if self.path == "/api/v1/knowledge":
            self._json(201, {
                "id": "k-1",
                "category": "solution",
                "type": "verified_solution",
                "title": "Fix PostgreSQL connection",
                "description": "Use service hostname",
                "domain": "software_engineering",
                "tags": ["postgresql", "docker"],
                "content": {"solution": "Use db hostname"},
                "verification": {"status": "unverified", "evidence": []},
                "provenance": {"agent_id": "agent-1", "framework": "langgraph", "contributor_id": "agent-1"},
                "visibility": "public",
                "version": 1,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
            })
        elif self.path == "/api/v1/recall":
            self._json(200, [{
                "knowledge": {
                    "id": "k-1",
                    "category": "solution",
                    "type": "verified_solution",
                    "title": "Fix PostgreSQL connection",
                    "description": "Use service hostname",
                    "domain": "software_engineering",
                    "tags": ["postgresql"],
                    "content": {"solution": "Use db hostname"},
                    "verification": {"status": "independently_confirmed", "evidence": []},
                    "provenance": {"agent_id": "agent-1", "framework": "langgraph", "contributor_id": "agent-1"},
                    "visibility": "public",
                    "version": 1,
                },
                "score": {"total_score": 1.4},
                "safety_notice": "UNTRUSTED EXTERNAL KNOWLEDGE",
            }])
        elif self.path == "/api/v1/knowledge/k-1/verify":
            self._json(200, {
                "id": "v-1",
                "knowledge_id": "k-1",
                "verifier_agent_id": "agent-2",
                "status": "independently_confirmed",
                "method": "integration_test",
            })
        else:
            self._json(404, {"error": {"code": "NOT_FOUND", "message": "not found"}})

    def _json(self, code: int, payload: object) -> None:
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(payload).encode())


class TestPythonBrainMeshSdk(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.server = HTTPServer(("127.0.0.1", 0), _MockBrainMeshHandler)
        cls.port = cls.server.server_address[1]
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls) -> None:
        cls.server.shutdown()
        cls.server.server_close()

    def test_register_contribute_recall_verify(self) -> None:
        client = BrainMesh(f"http://127.0.0.1:{self.port}")
        reg = client.register_agent("py-agent", "langgraph")
        self.assertEqual(reg.api_key, "bm_test_123")

        authed = BrainMesh(f"http://127.0.0.1:{self.port}", api_key=reg.api_key)
        k = authed.contribute(
            title="Fix PostgreSQL connection",
            description="Use service hostname",
            domain="software_engineering",
            content={"solution": "Use db hostname"},
            visibility="public",
        )
        self.assertEqual(k.id, "k-1")

        matches = authed.recall("PostgreSQL connection")
        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0].verification.status, "independently_confirmed")

        ver = authed.verify("k-1")
        self.assertEqual(ver.status, "independently_confirmed")

    def test_auth_error(self) -> None:
        unauth = BrainMesh(f"http://127.0.0.1:{self.port}")
        with self.assertRaises(BrainMeshError) as ctx:
            unauth.recall("test")
        self.assertEqual(ctx.exception.status_code, 401)


if __name__ == "__main__":
    unittest.main()
