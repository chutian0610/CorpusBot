import { invoke } from '@tauri-apps/api/core';
import { open as openDirectoryDialog } from '@tauri-apps/plugin-dialog';
import type {
  ConnectionTestResult,
  DocumentSummary,
  IngestResult,
  IngestJob,
  IngestRunDetail,
  IngestRunRow,
  LintReport,
  QueryAnswer,
  RawSource,
  SettingsInput,
  SettingsSummary,
  SnapshotResult,
  SnapshotRow,
  TemplateId,
  WikiPage,
  WikiPageSummary,
  WorkspaceStatus,
  WorkspaceSummary,
} from '../types';

export const isDesktopBackend = '__TAURI_INTERNALS__' in window;

const LOCAL_BACKEND_ENABLED =
  import.meta.env.MODE === 'local-backend' || import.meta.env.VITE_CORPUSBOT_BACKEND === 'local';

async function invokeLocalBackend<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const response = await fetch('/api/invoke', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ command, args: args ?? {} }),
  });
  const payload = (await response.json().catch(() => null)) as { error?: string } | null;
  if (!response.ok) {
    throw new Error(payload?.error ?? `Local backend request failed (${response.status})`);
  }
  return payload as T;
}

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (LOCAL_BACKEND_ENABLED) {
    return invokeLocalBackend<T>(command, args);
  }
  if (!isDesktopBackend) {
    throw new Error('The CorpusBot desktop backend is only available in the app.');
  }
  return invoke<T>(command, args);
}

export const api = {
  chooseWorkspaceDirectory: async () => {
    if (!isDesktopBackend) {
      return invokeCommand<string | null>('choose_workspace_directory');
    }

    const selected = await openDirectoryDialog({
      title: 'Choose workspace',
      directory: true,
      multiple: false,
    });
    if (Array.isArray(selected)) return selected[0] ?? null;
    return selected;
  },
  initWorkspace: (root: string, template: TemplateId) =>
    invokeCommand<WorkspaceSummary>('init_workspace', { root, template }),
  openWorkspace: (root: string) => invokeCommand<WorkspaceSummary>('open_workspace', { root }),
  workspaceStatus: (root: string) => invokeCommand<WorkspaceStatus>('workspace_status', { root }),
  listPages: (root: string) => invokeCommand<WikiPageSummary[]>('list_wiki_pages', { root }),
  readPage: (root: string, path: string) =>
    invokeCommand<WikiPage>('read_wiki_page', { root, path }),
  readRawSource: (root: string, sourceVersionId: string) =>
    invokeCommand<RawSource | null>('read_raw_source', { root, sourceVersionId }),
  ingestContent: (root: string, fileName: string, markdown: string) =>
    invokeCommand<IngestResult>('ingest_content', { root, fileName, markdown }),
  startIngestContent: (root: string, fileName: string, markdown: string) =>
    invokeCommand<IngestJob>('start_ingest_content', { root, fileName, markdown }),
  ingestJob: (jobId: string) => invokeCommand<IngestJob | null>('get_ingest_job', { jobId }),
  documents: (root: string) => invokeCommand<DocumentSummary[]>('list_documents', { root }),
  ingestRuns: (root: string, limit = 50) =>
    invokeCommand<IngestRunRow[]>('list_ingest_runs', { root, limit }),
  ingestRun: (root: string, runId: string) =>
    invokeCommand<IngestRunDetail | null>('read_ingest_run', { root, runId }),
  query: (root: string, question: string, limit = 8) =>
    invokeCommand<QueryAnswer>('query', { root, question, limit }),
  lint: (root: string) => invokeCommand<LintReport>('run_lint', { root }),
  createSnapshot: (root: string, message: string) =>
    invokeCommand<SnapshotResult>('create_snapshot', { root, message }),
  history: (root: string, limit = 50) =>
    invokeCommand<SnapshotRow[]>('list_snapshots', { root, limit }),
  restore: (root: string, snapshotId: string, confirmed: boolean) =>
    invokeCommand<{ snapshotId: string; restored: boolean }>('restore_snapshot', {
      root,
      snapshotId,
      confirmed,
    }),
  getSettings: () => invokeCommand<SettingsSummary>('get_settings'),
  saveSettings: (settings: SettingsInput) =>
    invokeCommand<SettingsSummary>('save_settings', { settings }),
  testLLMConnection: (settings: SettingsInput) =>
    invokeCommand<ConnectionTestResult>('test_llm_connection', { settings }),
};
