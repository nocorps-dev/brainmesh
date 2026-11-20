from __future__ import annotations

import asyncio
import json
import urllib.error
import urllib.request
from typing import Any

from .models import (
    AgentRegistration,
    KnowledgeObject,
    RecallMatch,
    VerificationRecord,
)


class BrainMeshError(Exception):
    def __init__(self, message: str, code: str = "INTERNAL_ERROR", status_code: int | None = None):
        super().__init__(message)
        self.code = code
        self.status_code = status_code


class BrainMesh:
    """Synchronous Python SDK client for BrainMesh REST API v1."""

    def __init__(self, base_url: str = "http://127.0.0.1:8080", api_key: str | None = None, timeout: float = 30.0):
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.timeout = timeout

    def _request(self, method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
        url = f"{self.base_url}{path}"
        data = json.dumps(payload).encode("utf-8") if payload is not None else None
        headers = {"Content-Type": "application/json", "Accept": "application/json"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"

        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                raw = resp.read().decode("utf-8")
                return json.loads(raw) if raw else {}
        except urllib.error.HTTPError as exc:
            body = exc.read().decode("utf-8", errors="replace")
            try:
                err = json.loads(body).get("error", {})
                raise BrainMeshError(err.get("message", body), code=err.get("code", "API_ERROR"), status_code=exc.code) from exc
            except json.JSONDecodeError:
                raise BrainMeshError(body, status_code=exc.code) from exc
        except urllib.error.URLError as exc:
            raise BrainMeshError(f"connection error: {exc.reason}", code="CONNECTION_ERROR") from exc

    def register_agent(self, name: str, framework: str, model: str | None = None) -> AgentRegistration:
        payload: dict[str, Any] = {"name": name, "framework": framework}
        if model is not None:
            payload["model"] = model
        return AgentRegistration.from_dict(self._request("POST", "/api/v1/agents/register", payload))

    def contribute(
        self,
        title: str,
        description: str,
        domain: str,
        content: dict[str, Any],
        *,
        category: str = "solution",
        record_type: str = "verified_solution",
        tags: list[str] | None = None,
        visibility: str = "private",
    ) -> KnowledgeObject:
        payload = {
            "category": category,
            "type": record_type,
            "title": title,
            "description": description,
            "domain": domain,
            "tags": tags or [],
            "content": content,
            "visibility": visibility,
        }
        return KnowledgeObject.from_dict(self._request("POST", "/api/v1/knowledge", payload))

    def get_knowledge(self, knowledge_id: str) -> KnowledgeObject:
        return KnowledgeObject.from_dict(self._request("GET", f"/api/v1/knowledge/{knowledge_id}"))

    def recall(
        self,
        query: str,
        *,
        domain: str | None = None,
        framework: str | None = None,
        tags: list[str] | None = None,
        limit: int = 10,
    ) -> list[RecallMatch]:
        payload: dict[str, Any] = {"query": query, "limit": limit, "tags": tags or []}
        if domain is not None:
            payload["domain"] = domain
        if framework is not None:
            payload["framework"] = framework
        data = self._request("POST", "/api/v1/recall", payload)
        return [RecallMatch.from_dict(item) for item in data]

    def verify(
        self,
        knowledge_id: str,
        status: str = "independently_confirmed",
        method: str = "integration_test",
        evidence: list[Any] | None = None,
    ) -> VerificationRecord:
        payload = {
            "status": status,
            "method": method,
            "evidence": evidence or [],
        }
        return VerificationRecord.from_dict(
            self._request("POST", f"/api/v1/knowledge/{knowledge_id}/verify", payload)
        )

    def submit_experience(
        self,
        title: str,
        outcome: str,
        summary: str,
        events: list[Any] | None = None,
        approved_for_publication: bool = False,
    ) -> dict[str, Any]:
        return self._request(
            "POST",
            "/api/v1/experiences",
            {
                "title": title,
                "outcome": outcome,
                "summary": summary,
                "events": events or [],
                "approved_for_publication": approved_for_publication,
            },
        )


class AsyncBrainMesh:
    """Asynchronous Python SDK client for BrainMesh REST API v1."""

    def __init__(self, base_url: str = "http://127.0.0.1:8080", api_key: str | None = None, timeout: float = 30.0):
        self._sync = BrainMesh(base_url=base_url, api_key=api_key, timeout=timeout)

    async def register_agent(self, name: str, framework: str, model: str | None = None) -> AgentRegistration:
        return await asyncio.to_thread(self._sync.register_agent, name, framework, model)

    async def contribute(self, *args: Any, **kwargs: Any) -> KnowledgeObject:
        return await asyncio.to_thread(self._sync.contribute, *args, **kwargs)

    async def get_knowledge(self, knowledge_id: str) -> KnowledgeObject:
        return await asyncio.to_thread(self._sync.get_knowledge, knowledge_id)

    async def recall(self, *args: Any, **kwargs: Any) -> list[RecallMatch]:
        return await asyncio.to_thread(self._sync.recall, *args, **kwargs)

    async def verify(self, *args: Any, **kwargs: Any) -> VerificationRecord:
        return await asyncio.to_thread(self._sync.verify, *args, **kwargs)
