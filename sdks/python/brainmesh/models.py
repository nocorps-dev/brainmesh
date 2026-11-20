from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class VerificationInfo:
    status: str = "unverified"
    method: str | None = None
    evidence: list[Any] = field(default_factory=list)
    verified_at: str | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "VerificationInfo":
        return cls(
            status=data.get("status", "unverified"),
            method=data.get("method"),
            evidence=data.get("evidence", []),
            verified_at=data.get("verified_at"),
        )


@dataclass
class Provenance:
    agent_id: str
    framework: str
    contributor_id: str
    organization_id: str | None = None
    workspace_id: str | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Provenance":
        return cls(
            agent_id=data["agent_id"],
            framework=data["framework"],
            contributor_id=data["contributor_id"],
            organization_id=data.get("organization_id"),
            workspace_id=data.get("workspace_id"),
        )


@dataclass
class KnowledgeObject:
    id: str
    category: str
    record_type: str
    title: str
    description: str
    domain: str
    tags: list[str]
    content: dict[str, Any]
    verification: VerificationInfo
    provenance: Provenance
    visibility: str
    version: int
    created_at: str
    updated_at: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "KnowledgeObject":
        return cls(
            id=data["id"],
            category=data.get("category", "knowledge"),
            record_type=data.get("type", "knowledge"),
            title=data["title"],
            description=data["description"],
            domain=data["domain"],
            tags=data.get("tags", []),
            content=data.get("content", {}),
            verification=VerificationInfo.from_dict(data.get("verification", {})),
            provenance=Provenance.from_dict(data["provenance"]),
            visibility=data.get("visibility", "private"),
            version=data.get("version", 1),
            created_at=data.get("created_at", ""),
            updated_at=data.get("updated_at", ""),
        )


@dataclass
class RecallMatch:
    knowledge: KnowledgeObject
    score: dict[str, float]
    safety_notice: str

    @property
    def title(self) -> str:
        return self.knowledge.title

    @property
    def verification(self) -> VerificationInfo:
        return self.knowledge.verification

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "RecallMatch":
        return cls(
            knowledge=KnowledgeObject.from_dict(data["knowledge"]),
            score=data.get("score", {}),
            safety_notice=data.get("safety_notice", ""),
        )


@dataclass
class AgentRegistration:
    agent_id: str
    name: str
    framework: str
    api_key: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "AgentRegistration":
        agent = data["agent"]
        return cls(
            agent_id=agent["id"],
            name=agent["name"],
            framework=agent["framework"],
            api_key=data["api_key"],
        )


@dataclass
class VerificationRecord:
    id: str
    knowledge_id: str
    verifier_agent_id: str
    status: str
    method: str

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "VerificationRecord":
        return cls(
            id=data["id"],
            knowledge_id=data["knowledge_id"],
            verifier_agent_id=data["verifier_agent_id"],
            status=data["status"],
            method=data["method"],
        )
