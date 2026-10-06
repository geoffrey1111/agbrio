export type IntegrityStatus = "VERIFIED" | "MISMATCH" | "NOT_PROVIDED" | "ERROR";
export type CandidateConfidence = "HIGH" | "LOW";

export interface AttachmentCandidate {
  id: string;
  source_message_id: string;
  raw_path: string;
  normalized_path: string | null;
  filename: string;
  extension: string | null;
  exists: boolean;
  is_file: boolean;
  size: number | null;
  declared_sha256: string | null;
  actual_sha256: string | null;
  integrity_status: IntegrityStatus;
  detection_reason: string;
  confidence: CandidateConfidence;
  default_selected: boolean;
  warnings: string[];
}
