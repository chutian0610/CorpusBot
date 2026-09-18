import { create } from 'zustand';
import { api } from '../lib/api';
import type {
  DocumentSummary,
  IngestRunDetail,
  IngestRunRow,
  IngestJob,
  LintReport,
  QueryAnswer,
  SettingsInput,
  SettingsSummary,
  SnapshotResult,
  SnapshotRow,
  RawSource,
  TemplateId,
  WikiPage,
  WikiPageSummary,
  WorkspaceStatus,
  WorkspaceSummary,
} from '../types';

type ViewId = 'wiki' | 'documents' | 'ingest' | 'chat' | 'lint' | 'history' | 'settings';

const LAST_WORKSPACE_KEY = 'corpusbot.root';
const RECENT_WORKSPACES_KEY = 'corpusbot.recentWorkspaces';
const MAX_RECENT_WORKSPACES = 5;

let workspaceOperationId = 0;
let workspaceOperationQueue: Promise<void> = Promise.resolve();

function enqueueWorkspaceOperation(operation: () => Promise<void>): Promise<void> {
  const operationId = ++workspaceOperationId;
  const task = workspaceOperationQueue.then(async () => {
    if (operationId === workspaceOperationId) {
      await operation();
    }
  });
  workspaceOperationQueue = task.then(
    () => {},
    () => {},
  );
  return task;
}

function readLastWorkspace(): string {
  return localStorage.getItem(LAST_WORKSPACE_KEY) ?? '';
}

function readRecentWorkspaces(): string[] {
  try {
    const value = JSON.parse(localStorage.getItem(RECENT_WORKSPACES_KEY) ?? '[]');
    if (!Array.isArray(value)) return [];
    return value.filter((path): path is string => typeof path === 'string' && path !== '');
  } catch {
    return [];
  }
}

function rememberWorkspace(root: string): string[] {
  const recentWorkspaces = [root, ...readRecentWorkspaces().filter((path) => path !== root)].slice(
    0,
    MAX_RECENT_WORKSPACES,
  );
  localStorage.setItem(LAST_WORKSPACE_KEY, root);
  localStorage.setItem(RECENT_WORKSPACES_KEY, JSON.stringify(recentWorkspaces));
  return recentWorkspaces;
}

function forgetWorkspace(root: string): string[] {
  const recentWorkspaces = readRecentWorkspaces().filter((path) => path !== root);
  localStorage.setItem(RECENT_WORKSPACES_KEY, JSON.stringify(recentWorkspaces));
  if (readLastWorkspace() === root) {
    localStorage.setItem(LAST_WORKSPACE_KEY, recentWorkspaces[0] ?? '');
  }
  return recentWorkspaces;
}

type WorkspaceState = {
  activeView: ViewId;
  root: string;
  recentWorkspaces: string[];
  initialized: boolean;
  loading: boolean;
  busyMessage: string;
  error?: string;
  summary?: WorkspaceSummary;
  status?: WorkspaceStatus;
  pages: WikiPageSummary[];
  documents: DocumentSummary[];
  documentsLoading: boolean;
  selectedRawSourceVersion?: string;
  rawSource?: RawSource;
  rawSourceLoading: boolean;
  ingestJobs: IngestJob[];
  selectedPath?: string;
  selectedPage?: WikiPage;
  ingestRuns: IngestRunRow[];
  selectedIngestRunId?: string;
  ingestRunDetail?: IngestRunDetail;
  ingestLoading: boolean;
  ingestDetailLoading: boolean;
  question: string;
  answer?: QueryAnswer;
  answering: boolean;
  lintReport?: LintReport;
  snapshots: SnapshotRow[];
  snapshotResult?: SnapshotResult;
  settings?: SettingsSummary;
  settingsForm: SettingsInput;
  selectedSnapshotId?: string;
  setActiveView: (view: ViewId) => void;
  setError: (error?: string) => void;
  setQuestion: (question: string) => void;
  setSelectedSnapshotId: (snapshotId?: string) => void;
  updateSettingsForm: (settings: Partial<SettingsInput>) => void;
  initialize: (root: string, template: TemplateId) => Promise<void>;
  open: (root: string) => Promise<void>;
  removeRecentWorkspace: (root: string) => void;
  returnToWorkspaceSetup: () => void;
  refresh: () => Promise<void>;
  selectPage: (path: string) => Promise<void>;
  importMarkdown: (fileName: string, markdown: string) => Promise<void>;
  watchIngestJob: (jobId: string) => Promise<void>;
  loadDocuments: () => Promise<void>;
  loadRawSource: (sourceVersionId: string, path: string) => Promise<void>;
  loadIngestRuns: () => Promise<void>;
  selectIngestRun: (runId: string) => Promise<void>;
  ask: () => Promise<void>;
  lint: () => Promise<void>;
  loadHistory: () => Promise<void>;
  createSnapshot: () => Promise<void>;
  restoreSnapshot: (snapshotId: string) => Promise<void>;
  loadSettings: () => Promise<void>;
  saveSettings: () => Promise<void>;
};

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  activeView: 'wiki',
  root: readLastWorkspace(),
  recentWorkspaces: readRecentWorkspaces(),
  initialized: false,
  loading: false,
  busyMessage: '',
  error: undefined,
  pages: [],
  documents: [],
  documentsLoading: false,
  rawSourceLoading: false,
  rawSource: undefined,
  ingestJobs: [],
  ingestRuns: [],
  ingestLoading: false,
  ingestDetailLoading: false,
  question: '',
  answering: false,
  snapshots: [],
  settingsForm: {
    baseUrl: '',
    model: '',
    apiKey: '',
    gitAuthorName: '',
    gitAuthorEmail: '',
  },

  setActiveView: (activeView) => set({ activeView }),
  setError: (error) => set({ error }),
  setQuestion: (question) => set({ question }),
  setSelectedSnapshotId: (selectedSnapshotId) => set({ selectedSnapshotId }),
  updateSettingsForm: (settings) =>
    set((state) => ({ settingsForm: { ...state.settingsForm, ...settings } })),

  initialize: (root, template) =>
    enqueueWorkspaceOperation(async () => {
      set({ loading: true, error: undefined });
      try {
        const summary = await api.initWorkspace(root, template);
        set({ root, summary });
        await get().refresh();
        set({ recentWorkspaces: rememberWorkspace(root) });
      } catch (error) {
        set({ error: error instanceof Error ? error.message : String(error) });
      } finally {
        set({ loading: false });
      }
    }),

  open: (root) =>
    enqueueWorkspaceOperation(async () => {
      set({ loading: true, error: undefined });
      try {
        const summary = await api.openWorkspace(root);
        set({ root, summary });
        await get().refresh();
        set({ recentWorkspaces: rememberWorkspace(root) });
      } catch (error) {
        set({ error: error instanceof Error ? error.message : String(error) });
      } finally {
        set({ loading: false });
      }
    }),

  returnToWorkspaceSetup: () => {
    workspaceOperationId += 1;
    set({
      activeView: 'wiki',
      initialized: false,
      status: undefined,
      summary: undefined,
      pages: [],
      selectedPath: undefined,
      selectedPage: undefined,
      documents: [],
      selectedRawSourceVersion: undefined,
      rawSource: undefined,
      ingestJobs: [],
      ingestRuns: [],
      selectedIngestRunId: undefined,
      ingestRunDetail: undefined,
      ingestLoading: false,
      ingestDetailLoading: false,
      answer: undefined,
      lintReport: undefined,
      snapshots: [],
      selectedSnapshotId: undefined,
      error: undefined,
    });
  },

  removeRecentWorkspace: (root) => {
    const recentWorkspaces = forgetWorkspace(root);
    set({ recentWorkspaces, root: recentWorkspaces[0] ?? '' });
  },

  refresh: async () => {
    const { root } = get();
    if (!root) return;
    set({ loading: true });
    try {
      const [summary, status, pages] = await Promise.all([
        api.openWorkspace(root),
        api.workspaceStatus(root),
        api.listPages(root),
      ]);
      set({ summary, status, pages, initialized: true, error: undefined });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  selectPage: async (path) => {
    const { root } = get();
    set({ selectedPath: path, loading: true });
    try {
      const selectedPage = await api.readPage(root, path);
      set({ selectedPage });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  importMarkdown: async (fileName, markdown) => {
    const { root } = get();
    set({ error: undefined });
    try {
      const job = await api.startIngestContent(root, fileName, markdown);
      set((state) => ({ ingestJobs: [job, ...state.ingestJobs] }));
      void get().watchIngestJob(job.jobId);
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },

  watchIngestJob: async (jobId) => {
    const isActive = (job?: IngestJob) =>
      Boolean(job && ['queued', 'running'].includes(job.status));

    let current = await api.ingestJob(jobId);
    if (!current) return;
    set((state) => ({
      ingestJobs: state.ingestJobs.map((job) => (job.jobId === jobId ? current! : job)),
    }));

    while (isActive(current)) {
      await new Promise((resolve) => setTimeout(resolve, 350));
      try {
        current = await api.ingestJob(jobId);
      } catch (error) {
        set({ error: error instanceof Error ? error.message : String(error) });
        return;
      }
      if (!current) return;
      set((state) => ({
        ingestJobs: state.ingestJobs.map((job) => (job.jobId === jobId ? current! : job)),
      }));
    }

    if (current.status === 'failed') {
      set({ error: current.error ?? `Import failed: ${current.fileName}` });
      return;
    }

    try {
      await get().refresh();
      await get().loadIngestRuns();
      await get().loadDocuments();
      const result = current.result;
      if (result?.status === 'committed') {
        set({ selectedIngestRunId: result.runId });
        await get().selectIngestRun(result.runId);
      }
      set((state) => ({
        ingestJobs: state.ingestJobs.filter(
          (job) =>
            job.jobId !== jobId ||
            !result ||
            result.status !== 'committed' ||
            !state.ingestRuns.some((run) => run.runId === result.runId),
        ),
      }));
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },

  loadDocuments: async () => {
    const { root } = get();
    set({ documentsLoading: true });
    try {
      const documents = await api.documents(root);
      set({ documents });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ documentsLoading: false });
    }
  },

  loadRawSource: async (sourceVersionId, path) => {
    const { root } = get();
    set({ selectedPath: path, selectedRawSourceVersion: sourceVersionId, rawSourceLoading: true });
    try {
      const rawSource = await api.readRawSource(root, sourceVersionId);
      if (!rawSource) throw new Error(`Raw source not found: ${sourceVersionId}`);
      set({ rawSource });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ rawSourceLoading: false });
    }
  },

  loadIngestRuns: async () => {
    const { root } = get();
    set({ ingestLoading: true });
    try {
      const ingestRuns = await api.ingestRuns(root, 50);
      set({ ingestRuns });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ ingestLoading: false });
    }
  },

  selectIngestRun: async (runId) => {
    const { root } = get();
    set({ selectedIngestRunId: runId, ingestDetailLoading: true });
    try {
      const ingestRunDetail = await api.ingestRun(root, runId);
      if (!ingestRunDetail) throw new Error(`Ingest run not found: ${runId}`);
      set({ ingestRunDetail });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ ingestDetailLoading: false });
    }
  },

  ask: async () => {
    const { root, question } = get();
    if (!question.trim()) return;
    set({ answering: true, error: undefined, answer: undefined });
    try {
      const answer = await api.query(root, question, 8);
      set({ answer });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ answering: false });
    }
  },

  lint: async () => {
    const { root } = get();
    set({ loading: true, error: undefined });
    try {
      const lintReport = await api.lint(root);
      set({ lintReport });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  loadHistory: async () => {
    const { root } = get();
    set({ loading: true });
    try {
      const snapshots = await api.history(root, 50);
      set({ snapshots });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  createSnapshot: async () => {
    const { root } = get();
    set({ loading: true, error: undefined });
    try {
      const snapshotResult = await api.createSnapshot(root, 'manual snapshot');
      set({ snapshotResult });
      await get().refresh();
      await get().loadHistory();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  restoreSnapshot: async (snapshotId) => {
    const { root } = get();
    set({ loading: true, error: undefined });
    try {
      await api.restore(root, snapshotId, true);
      await get().refresh();
      await get().loadHistory();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  loadSettings: async () => {
    set({ loading: true });
    try {
      const settings = await api.getSettings();
      set({
        settings,
        settingsForm: {
          baseUrl: settings.baseUrl ?? '',
          model: settings.model ?? '',
          apiKey: '',
          gitAuthorName: settings.gitAuthorName ?? '',
          gitAuthorEmail: settings.gitAuthorEmail ?? '',
        },
      });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  saveSettings: async () => {
    const { settingsForm } = get();
    set({ loading: true, error: undefined });
    try {
      const settings = await api.saveSettings(settingsForm);
      set({ settings, settingsForm: { ...settingsForm, apiKey: '' } });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },
}));
