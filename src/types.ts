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
  related: string[];
  sources: string[];
  markdown: string;
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
  baseUrl: string;
  model: string;
  hasApiKey: boolean;
  gitAuthorName?: string | null;
  gitAuthorEmail?: string | null;
};

export type SettingsInput = {
  baseUrl: string;
  model: string;
  apiKey?: string | null;
  gitAuthorName?: string | null;
  gitAuthorEmail?: string | null;
};
