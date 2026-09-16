import { useEffect } from 'react';
import {
  BookOpenText,
  FolderOpen,
  History,
  MessagesSquare,
  RefreshCw,
  Settings,
  ShieldCheck,
} from 'lucide-react';
import { MarkdownPreview } from './components/MarkdownPreview';
import { WorkspaceSetup } from './components/WorkspaceSetup';
import { useWorkspaceStore } from './store/useWorkspaceStore';

const navigation = [
  { id: 'wiki', label: 'Wiki', icon: BookOpenText },
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

function WikiView() {
  const pages = useWorkspaceStore((state) => state.pages);
  const selectedPage = useWorkspaceStore((state) => state.selectedPage);
  const selectPage = useWorkspaceStore((state) => state.selectPage);
  const importMarkdown = useWorkspaceStore((state) => state.importMarkdown);

  return (
    <div className="grid min-h-0 flex-1 lg:grid-cols-[18rem_minmax(0,1fr)]">
      <aside className="hidden min-h-0 min-w-0 flex-col border-r border-stone-200 bg-white lg:flex">
        <header className="flex h-12 shrink-0 items-center border-b border-stone-200 px-4">
          <h1 className="text-sm font-semibold">Wiki</h1>
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
                    className="w-full truncate rounded px-3 py-2 text-left text-sm hover:bg-stone-100"
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
          <span className="text-sm text-stone-500">
            {selectedPage ? selectedPage.path : 'No page selected'}
          </span>
          <label className="cursor-pointer rounded-md border border-stone-300 px-3 py-1 text-sm hover:bg-stone-100">
            Import
            <input
              type="file"
              accept=".md,.markdown,text/markdown"
              className="sr-only"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (!file) return;
                void file.text().then((markdown) => importMarkdown(file.name, markdown));
                event.target.value = '';
              }}
            />
          </label>
        </div>
        <div className="min-h-0 overflow-auto p-6">
          {selectedPage ? (
            <>
              <MarkdownPreview markdown={selectedPage.markdown} />
              {selectedPage.sources.length > 0 ? (
                <div className="mt-6 rounded-md border border-stone-200 p-3 text-sm">
                  <p className="font-medium">Sources</p>
                  <ul className="mt-1 list-disc pl-5 text-stone-600">
                    {selectedPage.sources.map((source) => (
                      <li key={source}>{source}</li>
                    ))}
                  </ul>
                </div>
              ) : null}
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
            <span>Pages: {report.summary.pages}</span>
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

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  return (
    <form
      className="min-h-0 flex-1 space-y-4 overflow-auto p-6"
      onSubmit={(event) => {
        event.preventDefault();
        void saveSettings();
      }}
    >
      <label className="block space-y-1 text-sm">
        <span className="font-medium">Base URL</span>
        <input
          value={settingsForm.baseUrl ?? ''}
          onChange={(event) => update({ baseUrl: event.target.value })}
          placeholder="https://api.openai.com/v1"
          className="w-full rounded-md border px-3 py-2"
        />
      </label>
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
      <div className="grid gap-3 sm:grid-cols-2">
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
      <p className="text-xs text-stone-500">
        Leave Base URL and Model blank to use the built-in defaults.
      </p>
      <button className="rounded-md bg-moss px-3 py-2 text-sm font-medium text-white">
        Save settings
      </button>
    </form>
  );
}

export default function App() {
  const initialized = useWorkspaceStore((state) => state.initialized);
  const status = useWorkspaceStore((state) => state.status);
  const error = useWorkspaceStore((state) => state.error);
  const activeView = useWorkspaceStore((state) => state.activeView);
  const setActiveView = useWorkspaceStore((state) => state.setActiveView);
  const refresh = useWorkspaceStore((state) => state.refresh);
  const loadHistory = useWorkspaceStore((state) => state.loadHistory);
  const returnToWorkspaceSetup = useWorkspaceStore((state) => state.returnToWorkspaceSetup);
  const loading = useWorkspaceStore((state) => state.loading);

  if (!initialized) {
    return <WorkspaceSetup />;
  }

  return (
    <main className="flex h-screen min-h-0 flex-col bg-paper text-ink">
      <header className="flex shrink-0 items-center justify-between gap-3 border-b border-stone-200 bg-white px-4 py-2">
        <div className="flex min-w-0 items-center gap-3">
          <span className="font-semibold">CorpusBot</span>
          <span className="truncate rounded px-2 py-1 text-xs text-stone-500">
            {status?.root ?? ''}
          </span>
          <span className="rounded bg-stone-100 px-2 py-1 text-xs">{statusLabel(status)}</span>
          <span className="truncate text-xs text-stone-500">
            {status?.headSnapshotId ?? 'no snapshot'}
          </span>
        </div>
        <button
          type="button"
          aria-label="Change workspace"
          className="flex size-9 items-center justify-center rounded-md hover:bg-stone-100"
          title="Change workspace"
          onClick={() => returnToWorkspaceSetup()}
        >
          <FolderOpen className="size-4" />
        </button>
        <button
          type="button"
          aria-label="Refresh workspace"
          className="flex size-9 items-center justify-center rounded-md hover:bg-stone-100"
          onClick={() => void refresh()}
        >
          <RefreshCw className="size-4" />
        </button>
      </header>

      {error ? (
        <div className="border-b border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          {error}
        </div>
      ) : null}
      {loading ? <div className="h-1 animate-pulse bg-moss" /> : null}

      <div className="grid min-h-0 flex-1 lg:grid-cols-[4rem_18rem_minmax(0,1fr)]">
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
                  if (id === 'history') void loadHistory();
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

        <aside className="hidden min-h-0 min-w-0 flex-col border-r border-stone-200 bg-white lg:flex">
          <header className="flex h-12 shrink-0 items-center border-b border-stone-200 px-4">
            <h1 className="text-sm font-semibold">Workspace</h1>
          </header>
          <div className="min-h-0 flex-1 overflow-auto p-3">
            {status?.dirtyPaths?.length ? (
              <div className="mb-3 rounded-md border border-amber-300 bg-amber-50 p-2 text-xs">
                <p className="font-medium">Uncommitted changes</p>
                <ul className="mt-1 space-y-1">
                  {status.dirtyPaths?.map((path) => (
                    <li key={path} className="truncate">
                      {path}
                    </li>
                  ))}
                </ul>
                <SnapshotButton />
              </div>
            ) : null}
          </div>
        </aside>

        <section className="flex min-h-0 min-w-0 flex-col">
          {activeView === 'wiki' ? <WikiView /> : null}
          {activeView === 'chat' ? <ChatView /> : null}
          {activeView === 'lint' ? <LintView /> : null}
          {activeView === 'history' ? <HistoryView /> : null}
          {activeView === 'settings' ? <SettingsView /> : null}
        </section>
      </div>
    </main>
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
