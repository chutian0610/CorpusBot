import { useEffect, useRef, useState } from 'react';
import {
  BookOpenText,
  CircleAlert,
  CircleCheck,
  FileText,
  FolderOpen,
  History,
  Inbox,
  MessagesSquare,
  Settings,
  ShieldCheck,
  TriangleAlert,
} from 'lucide-react';
import { IngestView } from './components/IngestView';
import { DocumentsView } from './components/DocumentsView';
import { MarkdownPreview } from './components/MarkdownPreview';
import { PageMetadata } from './components/PageMetadata';
import { api } from './lib/api';
import { WorkspaceSetup } from './components/WorkspaceSetup';
import { useWorkspaceStore } from './store/useWorkspaceStore';
import type { ConnectionTestResult, WorkspaceStatus } from './types';

const navigation = [
  { id: 'wiki', label: 'Wiki', icon: BookOpenText },
  { id: 'documents', label: 'Documents', icon: FileText },
  { id: 'ingest', label: 'Ingest', icon: Inbox },
  { id: 'chat', label: 'Chat', icon: MessagesSquare },
  { id: 'lint', label: 'Health', icon: ShieldCheck },
  { id: 'history', label: 'History', icon: History },
  { id: 'settings', label: 'Settings', icon: Settings },
] as const;

function statusLabel(status?: {
  dirtyPaths: string[];
  unsafeState?: string | null;
  recoveryPending: boolean;
}) {
  if (!status) return 'Unknown';
  if (status.recoveryPending) return 'Recovery pending';
  if (status.unsafeState) return 'Git blocked';
  if (status.dirtyPaths?.length) return 'Dirty';
  return 'Clean';
}

function statusAccent(status?: WorkspaceStatus) {
  if (!status) return 'text-stone-500';
  if (status.unsafeState) return 'text-red-600';
  if (status.recoveryPending) return 'text-amber-600';
  if (status.dirtyPaths.length) return 'text-amber-600';
  return 'text-stone-600';
}

function workspaceName(root: string) {
  const name = root.split(/[\\/]/).filter(Boolean).at(-1);
  return name || root || 'Workspace';
}

function WikiView() {
  const pages = useWorkspaceStore((state) => state.pages);
  const selectedPage = useWorkspaceStore((state) => state.selectedPage);
  const selectPage = useWorkspaceStore((state) => state.selectPage);

  return (
    <div className="grid min-h-0 flex-1 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <aside className="hidden min-h-0 min-w-0 flex-col border-r border-stone-200 bg-white lg:flex">
        <header className="flex h-12 shrink-0 items-center border-b border-stone-200 px-4">
          <h2 className="text-sm font-semibold">Pages</h2>
        </header>
        <div className="min-h-0 flex-1 overflow-auto p-2">
          {pages.length === 0 ? (
            <p className="p-4 text-sm text-stone-500">No pages yet</p>
          ) : (
            <ul className="space-y-1">
              {pages.map((page) => (
                <li key={page.path}>
                  <button
                    type="button"
                    aria-current={selectedPage?.path === page.path ? 'true' : undefined}
                    className={`w-full truncate rounded px-3 py-2 text-left text-sm transition-colors ${
                      selectedPage?.path === page.path
                        ? 'bg-moss/10 text-moss'
                        : 'hover:bg-stone-100'
                    }`}
                    onClick={() => void selectPage(page.path)}
                  >
                    {page.title}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </aside>
      <section className="grid min-h-0 min-w-0 flex-1 grid-rows-[auto_minmax(0,1fr)]">
        <div className="flex items-center justify-between border-b border-stone-200 bg-white px-4 py-2">
          <span
            className="min-w-0 truncate font-mono text-xs text-stone-500"
            title={selectedPage?.path}
          >
            {selectedPage ? selectedPage.path : 'No page selected'}
          </span>
        </div>
        <div className="min-h-0 overflow-auto p-6">
          {selectedPage ? (
            <>
              <PageMetadata page={selectedPage} />
              <div className="mt-6 rounded-md border border-stone-200 bg-white p-4">
                <MarkdownPreview markdown={selectedPage.body} />
              </div>
            </>
          ) : (
            <p className="text-sm text-stone-500">No page selected</p>
          )}
        </div>
      </section>
    </div>
  );
}

function ChatView() {
  const question = useWorkspaceStore((state) => state.question);
  const answer = useWorkspaceStore((state) => state.answer);
  const answering = useWorkspaceStore((state) => state.answering);
  const setQuestion = useWorkspaceStore((state) => state.setQuestion);
  const ask = useWorkspaceStore((state) => state.ask);

  return (
    <div className="flex min-h-0 flex-1 flex-col bg-white">
      <div className="min-h-0 flex-1 overflow-auto p-6">
        {answer ? (
          <div className="space-y-4">
            <MarkdownPreview markdown={answer.answer} />
            {answer.citations.length ? (
              <div className="space-y-2 rounded-md border border-stone-200 p-3">
                <p className="text-sm font-medium">Citations</p>
                {answer.citations.map((citation) => (
                  <blockquote key={citation.number} className="border-l-2 border-moss pl-3 text-sm">
                    <p>“{citation.quote}”</p>
                    <p className="mt-1 text-xs text-stone-500">
                      [{citation.number}] {citation.path}
                    </p>
                  </blockquote>
                ))}
              </div>
            ) : null}
            {answer.warnings.length ? (
              <ul className="space-y-1 text-sm text-amber-700">
                {answer.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            ) : null}
          </div>
        ) : (
          <p className="text-sm text-stone-500">Ask a question about this workspace.</p>
        )}
      </div>
      <form
        className="border-t border-stone-200 p-4"
        onSubmit={(event) => {
          event.preventDefault();
          void ask();
        }}
      >
        <div className="flex gap-2">
          <input
            value={question}
            onChange={(event) => setQuestion(event.target.value)}
            placeholder="Ask this workspace"
            className="min-w-0 flex-1 rounded-md border border-stone-300 px-3 py-2 text-sm"
          />
          <button
            type="submit"
            disabled={answering}
            className="rounded-md bg-moss px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          >
            {answering ? 'Asking' : 'Ask'}
          </button>
        </div>
      </form>
    </div>
  );
}

function LintView() {
  const report = useWorkspaceStore((state) => state.lintReport);
  const lint = useWorkspaceStore((state) => state.lint);

  useEffect(() => {
    void lint();
  }, [lint]);

  return (
    <div className="min-h-0 flex-1 overflow-auto p-6">
      {report ? (
        <>
          <div className="flex gap-4 text-sm">
            <span>Content pages: {report.summary.pages}</span>
            <span className="text-red-600">Errors: {report.summary.errors}</span>
            <span className="text-amber-600">Warnings: {report.summary.warnings}</span>
          </div>
          <ul className="mt-4 space-y-2">
            {report.issues.map((issue, index) => (
              <li
                key={`${issue.path}-${issue.code}-${index}`}
                className="rounded-md border p-3 text-sm"
              >
                <div className="flex justify-between gap-3">
                  <span className="font-medium">{issue.code}</span>
                  <span className="text-xs uppercase text-stone-500">{issue.severity}</span>
                </div>
                <p className="mt-1">{issue.message}</p>
                <p className="text-xs text-stone-500">{issue.path}</p>
              </li>
            ))}
          </ul>
        </>
      ) : (
        <p className="text-sm text-stone-500">Loading health report</p>
      )}
    </div>
  );
}

function HistoryView() {
  const snapshots = useWorkspaceStore((state) => state.snapshots);
  const loadHistory = useWorkspaceStore((state) => state.loadHistory);
  const restore = useWorkspaceStore((state) => state.restoreSnapshot);
  const selectedId = useWorkspaceStore((state) => state.selectedSnapshotId);
  const setSelectedId = useWorkspaceStore((state) => state.setSelectedSnapshotId);

  useEffect(() => {
    void loadHistory();
  }, [loadHistory]);

  return (
    <div className="min-h-0 flex-1 overflow-auto p-6">
      {selectedId ? (
        <div className="mb-4 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm">
          Restore workspace to <strong>{selectedId}</strong>? The current tracked content is saved
          first.
          <div className="mt-2 flex gap-2">
            <button
              type="button"
              className="rounded-md bg-red-600 px-3 py-1 text-white"
              onClick={() => {
                void restore(selectedId);
                setSelectedId(undefined);
              }}
            >
              Confirm restore
            </button>
            <button
              type="button"
              className="rounded-md border px-3 py-1"
              onClick={() => setSelectedId(undefined)}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : null}
      <ul className="space-y-2">
        {snapshots.map((snapshot) => (
          <li key={snapshot.snapshotId} className="rounded-md border p-3 text-sm">
            <div className="flex justify-between gap-4">
              <div className="min-w-0">
                <p className="truncate font-medium">{snapshot.message}</p>
                <p className="truncate text-xs text-stone-500">{snapshot.snapshotId}</p>
              </div>
              <button
                type="button"
                className="shrink-0 rounded-md border px-3 py-1 hover:bg-stone-100"
                onClick={() => setSelectedId(snapshot.snapshotId)}
              >
                Restore
              </button>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

function SettingsView() {
  const settingsForm = useWorkspaceStore((state) => state.settingsForm);
  const settings = useWorkspaceStore((state) => state.settings);
  const update = useWorkspaceStore((state) => state.updateSettingsForm);
  const loadSettings = useWorkspaceStore((state) => state.loadSettings);
  const saveSettings = useWorkspaceStore((state) => state.saveSettings);
  const [connectionTest, setConnectionTest] = useState<
    | { status: 'idle' }
    | { status: 'testing' }
    | { status: 'success'; result: ConnectionTestResult }
    | { status: 'error'; message: string }
  >({ status: 'idle' });
  const connectionTestId = useRef(0);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    connectionTestId.current += 1;
    setConnectionTest({ status: 'idle' });
  }, [settingsForm.baseUrl, settingsForm.model, settingsForm.apiKey]);

  const testConnection = async () => {
    const testId = ++connectionTestId.current;
    setConnectionTest({ status: 'testing' });
    try {
      const result = await api.testLLMConnection(settingsForm);
      if (testId === connectionTestId.current) {
        setConnectionTest({ status: 'success', result });
      }
    } catch (error) {
      if (testId === connectionTestId.current) {
        setConnectionTest({
          status: 'error',
          message: error instanceof Error ? error.message : String(error),
        });
      }
    }
  };

  return (
    <form
      className="grid min-h-0 flex-1 grid-rows-[minmax(0,1fr)_auto]"
      onSubmit={(event) => {
        event.preventDefault();
        void saveSettings();
      }}
    >
      <div className="min-h-0 overflow-auto p-6">
        <section aria-labelledby="llm-settings-heading" className="mx-auto max-w-4xl space-y-4">
          <div>
            <h2 id="llm-settings-heading" className="text-base font-semibold">
              Language model
            </h2>
            <p className="mt-1 text-sm text-stone-500">
              Used to analyze sources and answer workspace questions.
            </p>
          </div>

          <label className="block space-y-1 text-sm">
            <span className="font-medium">Base URL</span>
            <input
              value={settingsForm.baseUrl ?? ''}
              onChange={(event) => update({ baseUrl: event.target.value })}
              placeholder="https://api.openai.com/v1"
              className="w-full rounded-md border px-3 py-2"
            />
          </label>
          <div className="grid gap-3 md:grid-cols-2">
            <label className="block space-y-1 text-sm">
              <span className="font-medium">Model</span>
              <input
                value={settingsForm.model ?? ''}
                onChange={(event) => update({ model: event.target.value })}
                placeholder="gpt-4o-mini"
                className="w-full rounded-md border px-3 py-2"
              />
            </label>
            <label className="block space-y-1 text-sm">
              <span className="font-medium">API key</span>
              <input
                value={settingsForm.apiKey ?? ''}
                onChange={(event) => update({ apiKey: event.target.value })}
                placeholder={settings?.hasApiKey ? 'Saved key stays active' : 'Enter API key'}
                type="password"
                className="w-full rounded-md border px-3 py-2"
              />
            </label>
          </div>

          <div className="flex flex-wrap items-center gap-3">
            <button
              type="button"
              disabled={connectionTest.status === 'testing'}
              className="h-9 rounded-md border border-stone-300 px-3 text-sm font-medium hover:bg-stone-100 disabled:opacity-50"
              onClick={() => void testConnection()}
            >
              {connectionTest.status === 'testing' ? 'Testing' : 'Test connection'}
            </button>
            <div className="min-w-0 flex-1" aria-live="polite">
              {connectionTest.status === 'testing' ? (
                <p className="text-xs text-stone-500">Testing connection</p>
              ) : null}
              {connectionTest.status === 'success' ? (
                <p className="flex min-w-0 items-center gap-1.5 text-xs text-moss">
                  <CircleCheck className="size-3.5 shrink-0" />
                  <span
                    className="truncate"
                    title={`${connectionTest.result.model} · ${connectionTest.result.latencyMs} ms`}
                  >
                    Connected to {connectionTest.result.model} · {connectionTest.result.latencyMs}{' '}
                    ms
                  </span>
                </p>
              ) : null}
              {connectionTest.status === 'error' ? (
                <p
                  role="alert"
                  className="flex min-w-0 items-center gap-1.5 text-xs text-red-600"
                  title={connectionTest.message}
                >
                  <TriangleAlert className="size-3.5 shrink-0" />
                  <span className="truncate">{connectionTest.message}</span>
                </p>
              ) : null}
            </div>
          </div>

          <p className="text-xs text-stone-500">
            Leave Base URL and Model blank to use the built-in defaults. Leave API key blank to use
            the saved key.
          </p>
        </section>

        <section
          aria-labelledby="git-settings-heading"
          className="mx-auto mt-6 max-w-4xl border-t border-stone-200 pt-6"
        >
          <h2 id="git-settings-heading" className="text-base font-semibold">
            Git identity
          </h2>
          <div className="mt-4 grid gap-3 md:grid-cols-2">
            <label className="block space-y-1 text-sm">
              <span className="font-medium">Git author name</span>
              <input
                value={settingsForm.gitAuthorName ?? ''}
                onChange={(event) => update({ gitAuthorName: event.target.value })}
                className="w-full rounded-md border px-3 py-2"
              />
            </label>
            <label className="block space-y-1 text-sm">
              <span className="font-medium">Git author email</span>
              <input
                value={settingsForm.gitAuthorEmail ?? ''}
                onChange={(event) => update({ gitAuthorEmail: event.target.value })}
                className="w-full rounded-md border px-3 py-2"
              />
            </label>
          </div>
        </section>
      </div>

      <footer className="flex h-14 shrink-0 items-center justify-end border-t border-stone-200 bg-white px-6">
        <button className="rounded-md bg-moss px-3 py-2 text-sm font-medium text-white">
          Save settings
        </button>
      </footer>
    </form>
  );
}

export default function App() {
  const initialized = useWorkspaceStore((state) => state.initialized);
  const status = useWorkspaceStore((state) => state.status);
  const error = useWorkspaceStore((state) => state.error);
  const activeView = useWorkspaceStore((state) => state.activeView);
  const setActiveView = useWorkspaceStore((state) => state.setActiveView);
  const returnToWorkspaceSetup = useWorkspaceStore((state) => state.returnToWorkspaceSetup);
  const loading = useWorkspaceStore((state) => state.loading);
  const busyMessage = useWorkspaceStore((state) => state.busyMessage);

  if (!initialized) {
    return <WorkspaceSetup />;
  }

  return (
    <main className="flex h-dvh min-h-0 flex-col overflow-hidden bg-paper text-ink">
      <header className="flex h-12 shrink-0 items-center justify-between gap-3 border-b border-stone-200 bg-white px-4">
        <div className="flex min-w-0 items-baseline gap-3">
          <h1 className="min-w-0 truncate text-sm font-semibold">
            {workspaceName(status?.root ?? '')}
          </h1>
          <p className="shrink-0 text-xs text-stone-500">
            {navigation.find(({ id }) => id === activeView)?.label}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            type="button"
            className="inline-flex h-9 items-center gap-2 rounded-md px-3 text-sm font-medium hover:bg-stone-100"
            onClick={() => returnToWorkspaceSetup()}
          >
            <FolderOpen className="size-4" />
            <span className="hidden md:inline">Change workspace</span>
            <span className="md:hidden">Change</span>
          </button>
        </div>
      </header>

      <div className="grid min-h-0 flex-1 grid-rows-[auto_minmax(0,1fr)] lg:grid-cols-[4rem_minmax(0,1fr)] lg:grid-rows-[minmax(0,1fr)]">
        <nav
          aria-label="Primary"
          className="flex shrink-0 items-center gap-1 overflow-x-auto border-b border-stone-200 bg-white px-3 py-2 lg:min-h-0 lg:flex-col lg:overflow-visible lg:border-b-0 lg:border-r lg:px-0 lg:py-4"
        >
          {navigation.map(({ id, label, icon: Icon }) => {
            const selected = activeView === id;
            return (
              <button
                key={id}
                type="button"
                aria-label={label}
                aria-current={selected ? 'page' : undefined}
                title={label}
                onClick={() => {
                  setActiveView(id);
                }}
                className={`flex size-11 items-center justify-center rounded-md ${
                  selected ? 'bg-moss/10 text-moss' : 'text-stone-500 hover:bg-stone-100'
                }`}
              >
                <Icon className="size-5" />
              </button>
            );
          })}
        </nav>

        <section className="flex min-h-0 min-w-0 flex-col">
          {activeView === 'wiki' ? <WikiView /> : null}
          {activeView === 'documents' ? <DocumentsView /> : null}
          {activeView === 'ingest' ? <IngestView /> : null}
          {activeView === 'chat' ? <ChatView /> : null}
          {activeView === 'lint' ? <LintView /> : null}
          {activeView === 'history' ? <HistoryView /> : null}
          {activeView === 'settings' ? <SettingsView /> : null}
        </section>
      </div>

      <WorkspaceStatusBar
        status={status}
        error={error}
        loading={loading}
        busyMessage={busyMessage}
      />
    </main>
  );
}

function WorkspaceStatusBar({
  status,
  error,
  loading,
  busyMessage,
}: {
  status?: WorkspaceStatus;
  error?: string;
  loading: boolean;
  busyMessage: string;
}) {
  const activity = error || busyMessage || (loading ? 'Working' : '');

  return (
    <footer className="flex h-9 shrink-0 items-center gap-3 border-t border-stone-200 bg-white px-3 text-xs text-stone-600">
      <WorkspaceStatusChip status={status} />

      <div className="min-w-0 flex-1">
        {activity ? (
          <div
            role={error ? 'alert' : 'status'}
            aria-live={error ? 'assertive' : 'polite'}
            className={`truncate ${error ? 'text-red-600' : 'text-stone-500'}`}
            title={activity}
          >
            <span className="mr-2 inline-flex size-2 rounded-full bg-current align-middle" />
            {activity}
          </div>
        ) : null}
      </div>

      {status?.pageCount !== undefined ? (
        <span className="shrink-0 tabular-nums">{status.pageCount} pages</span>
      ) : null}
      <span
        className="hidden min-w-0 max-w-[28rem] truncate font-mono md:inline"
        title={status?.root ?? ''}
      >
        {status?.root ?? ''}
      </span>
      <span className="hidden shrink-0 font-mono lg:inline" title={status?.headSnapshotId ?? ''}>
        {status?.headSnapshotId ?? 'no snapshot'}
      </span>
    </footer>
  );
}

function WorkspaceStatusChip({ status }: { status?: WorkspaceStatus }) {
  const dirtyPaths = status?.dirtyPaths ?? [];
  const [showDirtyPaths, setShowDirtyPaths] = useState(false);
  const StatusIcon =
    status?.unsafeState || status?.recoveryPending
      ? TriangleAlert
      : dirtyPaths.length
        ? CircleAlert
        : CircleCheck;

  return (
    <div className="relative shrink-0" role="status" aria-live="polite">
      {dirtyPaths.length ? (
        <button
          type="button"
          aria-expanded={showDirtyPaths}
          className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2 font-medium hover:bg-stone-100 ${statusAccent(status)}`}
          onClick={() => setShowDirtyPaths((visible) => !visible)}
        >
          <StatusIcon className="size-3.5" />
          {statusLabel(status)} ({dirtyPaths.length})
        </button>
      ) : (
        <span
          className={`inline-flex h-7 items-center gap-1.5 rounded-md px-2 font-medium ${statusAccent(status)}`}
        >
          <StatusIcon className="size-3.5" />
          {statusLabel(status)}
        </span>
      )}

      {showDirtyPaths && dirtyPaths.length ? (
        <div className="absolute bottom-10 left-0 z-10 w-[22rem] rounded-md border border-stone-200 bg-white p-3 shadow-lg">
          <p className="text-xs font-semibold text-ink">Uncommitted changes</p>
          <ul className="mt-2 max-h-48 space-y-1 overflow-auto">
            {dirtyPaths.map((path) => (
              <li key={path} className="truncate font-mono text-xs" title={path}>
                {path}
              </li>
            ))}
          </ul>
          <SnapshotButton />
        </div>
      ) : null}
    </div>
  );
}

function SnapshotButton() {
  const createSnapshot = useWorkspaceStore((state) => state.createSnapshot);
  return (
    <button
      type="button"
      className="mt-2 rounded-md border border-stone-300 bg-white px-2 py-1"
      onClick={() => void createSnapshot()}
    >
      Create snapshot
    </button>
  );
}
