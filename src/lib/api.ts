import { invoke } from '@tauri-apps/api/core';
import type {
  IngestResult,
  LintReport,
  QueryAnswer,
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

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isDesktopBackend) {
    throw new Error('The CorpusBot desktop backend is only available in the app.');
  }
  return invoke<T>(command, args);
}

export const api = {
  initWorkspace: (root: string, template: TemplateId) =>
    invokeCommand<WorkspaceSummary>('init_workspace', { root, template }),
  openWorkspace: (root: string) => invokeCommand<WorkspaceSummary>('open_workspace', { root }),
  workspaceStatus: (root: string) => invokeCommand<WorkspaceStatus>('workspace_status', { root }),
  listPages: (root: string) => invokeCommand<WikiPageSummary[]>('list_wiki_pages', { root }),
  readPage: (root: string, path: string) =>
    invokeCommand<WikiPage>('read_wiki_page', { root, path }),
  ingestContent: (root: string, fileName: string, markdown: string) =>
    invokeCommand<IngestResult>('ingest_content', { root, fileName, markdown }),
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
};
