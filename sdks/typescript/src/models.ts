export type Visibility =
  | "private"
  | "workspace"
  | "organization"
  | "trusted_network"
  | "public";

export type VerificationStatus =
  | "unverified"
  | "reported_successful"
  | "independently_confirmed"
  | "verified_under_conditions"
  | "disputed"
  | "outdated"
  | "rejected";

export interface VerificationInfo {
  status: VerificationStatus;
  method?: string | null;
  evidence: unknown[];
  verified_at?: string | null;
}

export interface Provenance {
  agent_id: string;
  framework: string;
  contributor_id: string;
  organization_id?: string | null;
  workspace_id?: string | null;
}

export interface KnowledgeObject {
  id: string;
  category: string;
  type: string;
  title: string;
  description: string;
  domain: string;
  tags: string[];
  content: Record<string, unknown>;
  verification: VerificationInfo;
  provenance: Provenance;
  visibility: Visibility;
  version: number;
  created_at: string;
  updated_at: string;
}

export interface RecallMatch {
  knowledge: KnowledgeObject;
  score: {
    text_score: number;
    verification_boost: number;
    freshness_boost: number;
    total_score: number;
  };
  safety_notice: string;
}

export interface ContributeRequest {
  title: string;
  description: string;
  domain: string;
  content: Record<string, unknown>;
  category?: string;
  type?: string;
  tags?: string[];
  visibility?: Visibility;
}

export interface RecallRequest {
  query: string;
  domain?: string;
  framework?: string;
  tags?: string[];
  limit?: number;
}
