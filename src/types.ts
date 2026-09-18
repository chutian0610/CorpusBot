export type TemplateId = 'generic' | 'research';

export type WorkspaceSummary = {
  root: string;
  template: string;
  headSnapshotId?: string | null;
};

export type WorkspaceStatus = {
  root: string;
  template: string;
  headSnapshotId?: string | null;
  dirtyPaths: string[];
  unsafeState?: string | null;
  recoveryPending: boolean;
  pageCount: number;
};

export type WikiPageSummary = {
  path: string;
  title: string;
  pageType: string;
  sha256: string;
  updatedAt: string;
};

export type WikiPage = WikiPageSummary & {
  createdAt: string;
  updatedAt: string;
  tags: string[];
  aliases: string[];
  related: string[];
  sources: string[];
  sourceReferences: { sourceVersionId: string; title: string }[];
  markdown: string;
  body: string;
};

export type IngestResult =
  | {
      status: 'committed';
      runId: string;
      sourceId: string;
      sourceVersionId: string;
      sourcePage: string;
      createdPaths: string[];
      updatedPaths: string[];
      snapshotId: string;
      manifestId: string;
    }
  | { status: 'duplicate'; sourceId: string; sourceVersionId: string };

export type TouchedResource = {
  path: string;
  revisionKind: 'absent' | 'content' | 'generation' | string;
  sha256?: string | null;
};

export type IngestRunRow = {
  runId: string;
  sourceId: string;
  status: string;
  baselineSnapshotId: string;
  baselineManifestId: string;
  touchedResources: TouchedResource[];
  createdAt: string;
  finishedAt?: string | null;
  originalName?: string | null;
  sourceVersionId?: string | null;
  sha256?: string | null;
  size?: number | null;
};

export type IngestAuditEvent = {
  eventId: string;
  runId: string;
  node: string;
  attempt: number;
  status: string;
  inputManifestId?: string | null;
  outputRef?: string | null;
  promptTemplateId?: string | null;
  promptHash?: string | null;
  provider?: string | null;
  model?: string | null;
  latencyMs?: number | null;
  tokensIn?: number | null;
  tokensOut?: number | null;
  decision?: string | null;
  errorCode?: string | null;
};

export type IngestRunDetail = IngestRunRow & {
  sourcePage?: string | null;
  sourcePageMarkdown?: string | null;
  originalMarkdown?: string | null;
  originalMarkdownTruncated: boolean;
  events: IngestAuditEvent[];
};

export type IngestJob = {
  jobId: string;
  fileName: string;
  status: 'queued' | 'running' | 'succeeded' | 'failed' | string;
  createdAtMs: number;
  updatedAtMs: number;
  result?: IngestResult | null;
  error?: string | null;
};

export type DocumentPage = {
  path: string;
  title: string;
  pageType: string;
  updatedAt: string;
};

export type DocumentSummary = {
  sourcePage: string;
  sourceVersionId: string;
  rawPath?: string | null;
  originalName?: string | null;
  size?: number | null;
  title: string;
  updatedAt: string;
  pages: DocumentPage[];
};

export type RawSource = {
  sourceVersionId: string;
  path: string;
  originalName: string;
  size: number;
  markdown: string;
};

export type Citation = {
  number: number;
  path: string;
  title: string;
  quote: string;
  resourceRevision: unknown;
};

export type QueryAnswer = {
  answer: string;
  citations: Citation[];
  revisionManifestId: string;
  warnings: string[];
  insufficientEvidence: boolean;
};

export type Severity = 'error' | 'warning';

export type LintIssue = {
  code: string;
  severity: Severity;
  path: string;
  message: string;
  fixHint: string;
};

export type LintReport = {
  generatedAt: string;
  template: string;
  revisionManifestId: string;
  summary: { pages: number; errors: number; warnings: number };
  issues: LintIssue[];
};

export type SnapshotRow = {
  snapshotId: string;
  message: string;
  createdAt: number;
};

export type SnapshotResult = {
  result: 'created' | 'already_clean';
  snapshotId: string;
  manifestId: string;
  workspaceChangedAfterCapture: boolean;
};

export type SettingsSummary = {
  baseUrl?: string | null;
  model?: string | null;
  hasApiKey: boolean;
  gitAuthorName?: string | null;
  gitAuthorEmail?: string | null;
};

export type SettingsInput = {
  baseUrl?: string | null;
  model?: string | null;
  apiKey?: string | null;
  gitAuthorName?: string | null;
  gitAuthorEmail?: string | null;
};

export type ConnectionTestResult = {
  provider: string;
  model: string;
  latencyMs: number;
  responseId?: string | null;
};
