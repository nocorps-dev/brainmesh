from __future__ import annotations

from typing import Any, Callable

from ..client import BrainMesh


def langchain_retriever(client: BrainMesh, domain: str | None = None, limit: int = 5) -> Callable[[str], list[dict[str, Any]]]:
    """Returns a callable compatible with LangChain Runnable / retriever chains."""
    def _retrieve(query: str) -> list[dict[str, Any]]:
        matches = client.recall(query=query, domain=domain, limit=limit)
        return [
            {
                "page_content": f"[{m.safety_notice}]\n{m.knowledge.title}: {m.knowledge.description}",
                "metadata": {
                    "id": m.knowledge.id,
                    "verification_status": m.verification.status,
                    "provenance": m.knowledge.provenance.agent_id,
                    "score": m.score.get("total_score", 0.0),
                },
            }
            for m in matches
        ]
    return _retrieve


def langgraph_node(client: BrainMesh, query_key: str = "query") -> Callable[[dict[str, Any]], dict[str, Any]]:
    """Returns a LangGraph state-update node that enriches state with BrainMesh recall."""
    def _node(state: dict[str, Any]) -> dict[str, Any]:
        query = str(state.get(query_key, ""))
        matches = client.recall(query=query) if query else []
        return {
            **state,
            "brainmesh_context": [
                {"id": m.knowledge.id, "title": m.title, "content": m.knowledge.content, "safety": m.safety_notice}
                for m in matches
            ],
        }
    return _node


def crewai_recall_tool(client: BrainMesh) -> Callable[[str], str]:
    """Tool wrapper for CrewAI agents."""
    def _tool(query: str) -> str:
        matches = client.recall(query=query)
        return "\n".join(
            f"- {m.title} [{m.verification.status}] ({m.safety_notice})" for m in matches
        )
    return _tool


def autogen_recall_tool(client: BrainMesh) -> Callable[[str], list[dict[str, Any]]]:
    """Function-calling wrapper for AutoGen agents."""
    def _fn(query: str) -> list[dict[str, Any]]:
        return [
            {
                "id": m.knowledge.id,
                "title": m.title,
                "solution": m.knowledge.content,
                "verification": m.verification.status,
                "safety_notice": m.safety_notice,
            }
            for m in client.recall(query=query)
        ]
    return _fn


def openai_agents_tool(client: BrainMesh) -> dict[str, Any]:
    """Tool schema + handler for OpenAI Agents SDK."""
    return {
        "name": "brainmesh_recall",
        "description": "Search BrainMesh for verified cross-agent knowledge.",
        "handler": autogen_recall_tool(client),
    }
