import type { BrainMesh } from "../client.js";
import type { RecallMatch } from "../models.js";

export function createOpenCodeRecallHandler(client: BrainMesh) {
  return async (query: string, domain?: string): Promise<RecallMatch[]> => {
    return client.recall({ query, domain, limit: 10 });
  };
}

export function createOpenClawRecallHandler(client: BrainMesh) {
  return async (query: string): Promise<string> => {
    const matches = await client.recall({ query, limit: 5 });
    return matches
      .map(
        (m) =>
          `[${m.knowledge.verification.status}] ${m.knowledge.title} — ${m.knowledge.description} (${m.safety_notice})`
      )
      .join("\n");
  };
}

export function createLangChainJsRetriever(client: BrainMesh) {
  return {
    async getRelevantDocuments(query: string) {
      const matches = await client.recall({ query, limit: 5 });
      return matches.map((m) => ({
        pageContent: `[${m.safety_notice}]\n${m.knowledge.title}: ${m.knowledge.description}`,
        metadata: {
          id: m.knowledge.id,
          verificationStatus: m.knowledge.verification.status,
          score: m.score.total_score,
        },
      }));
    },
  };
}
