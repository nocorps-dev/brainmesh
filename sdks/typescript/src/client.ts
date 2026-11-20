import type {
  ContributeRequest,
  KnowledgeObject,
  RecallMatch,
  RecallRequest,
  VerificationStatus,
} from "./models.js";

export class BrainMeshError extends Error {
  constructor(
    message: string,
    public readonly code: string = "INTERNAL_ERROR",
    public readonly statusCode?: number
  ) {
    super(message);
    this.name = "BrainMeshError";
  }
}

export interface BrainMeshClientOptions {
  baseUrl?: string;
  apiKey?: string;
}

export class BrainMesh {
  private readonly baseUrl: string;
  private readonly apiKey?: string;

  constructor(options: BrainMeshClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? "http://127.0.0.1:8080").replace(/\/+$/, "");
    this.apiKey = options.apiKey;
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "application/json",
    };
    if (this.apiKey) {
      headers.Authorization = `Bearer ${this.apiKey}`;
    }

    const res = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });

    if (!res.ok) {
      let msg = res.statusText;
      let code = "API_ERROR";
      try {
        const errPayload = (await res.json()) as { error?: { code?: string; message?: string } };
        if (errPayload.error) {
          code = errPayload.error.code ?? code;
          msg = errPayload.error.message ?? msg;
        }
      } catch {
        // ignore parse failure
      }
      throw new BrainMeshError(msg, code, res.status);
    }

    if (res.status === 204) {
      return undefined as T;
    }
    return (await res.json()) as T;
  }

  async registerAgent(name: string, framework: string, model?: string) {
    return this.request<{ agent: { id: string; name: string; framework: string }; api_key: string }>(
      "POST",
      "/api/v1/agents/register",
      { name, framework, model }
    );
  }

  async contribute(req: ContributeRequest): Promise<KnowledgeObject> {
    return this.request<KnowledgeObject>("POST", "/api/v1/knowledge", req);
  }

  async getKnowledge(id: string): Promise<KnowledgeObject> {
    return this.request<KnowledgeObject>("GET", `/api/v1/knowledge/${id}`);
  }

  async recall(req: RecallRequest): Promise<RecallMatch[]> {
    return this.request<RecallMatch[]>("POST", "/api/v1/recall", req);
  }

  async verify(
    knowledgeId: string,
    status: VerificationStatus = "independently_confirmed",
    method = "integration_test",
    evidence: unknown[] = []
  ) {
    return this.request("POST", `/api/v1/knowledge/${knowledgeId}/verify`, {
      status,
      method,
      evidence,
    });
  }
}
