import { create } from 'zustand';
import { api } from '../lib/api';
import type {
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

type ViewId = 'wiki' | 'chat' | 'lint' | 'history' | 'settings';

type WorkspaceState = {
  activeView: ViewId;
  root: string;
  initialized: boolean;
  loading: boolean;
  busyMessage: string;
  error?: string;
  summary?: WorkspaceSummary;
  status?: WorkspaceStatus;
  pages: WikiPageSummary[];
  selectedPath?: string;
  selectedPage?: WikiPage;
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
  refresh: () => Promise<void>;
  selectPage: (path: string) => Promise<void>;
  importMarkdown: (fileName: string, markdown: string) => Promise<void>;
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
  root: localStorage.getItem('corpusbot.root') ?? '',
  initialized: false,
  loading: false,
  busyMessage: '',
  error: undefined,
  pages: [],
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

  initialize: async (root, template) => {
    set({ loading: true, error: undefined });
    try {
      const summary = await api.initWorkspace(root, template);
      localStorage.setItem('corpusbot.root', root);
      set({ root, summary, initialized: true });
      await get().refresh();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
  },

  open: async (root) => {
    set({ loading: true, error: undefined });
    try {
      const summary = await api.openWorkspace(root);
      localStorage.setItem('corpusbot.root', root);
      set({ root, summary, initialized: true });
      await get().refresh();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false });
    }
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
    set({ loading: true, busyMessage: `Importing ${fileName}`, error: undefined });
    try {
      await api.ingestContent(root, fileName, markdown);
      await get().refresh();
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ loading: false, busyMessage: '' });
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
          baseUrl: settings.baseUrl,
          model: settings.model,
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
