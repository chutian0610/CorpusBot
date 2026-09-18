import { useState } from 'react';
import { Clock3, FolderOpen, Plus, Search, Trash2 } from 'lucide-react';
import { WorkspaceActionDialog, type WorkspaceAction } from './WorkspaceActionDialog';
import { api, isDesktopBackend } from '../lib/api';
import { useWorkspaceStore } from '../store/useWorkspaceStore';
import type { TemplateId } from '../types';

export function WorkspaceSetup() {
  const recentWorkspaces = useWorkspaceStore((state) => state.recentWorkspaces);
  const [template, setTemplate] = useState<TemplateId>('research');
  const [workspaceQuery, setWorkspaceQuery] = useState('');
  const [dialogMode, setDialogMode] = useState<WorkspaceAction | undefined>();
  const [dialogPath, setDialogPath] = useState('');
  const [pickerOpening, setPickerOpening] = useState(false);
  const initialize = useWorkspaceStore((state) => state.initialize);
  const open = useWorkspaceStore((state) => state.open);
  const removeRecentWorkspace = useWorkspaceStore((state) => state.removeRecentWorkspace);
  const setError = useWorkspaceStore((state) => state.setError);
  const loading = useWorkspaceStore((state) => state.loading);
  const error = useWorkspaceStore((state) => state.error);

  const normalizedQuery = workspaceQuery.trim().toLowerCase();
  const visibleWorkspaces = normalizedQuery
    ? recentWorkspaces.filter((workspace) => workspace.toLowerCase().includes(normalizedQuery))
    : recentWorkspaces;

  console.log('WorkspaceSetup recent', recentWorkspaces);

  const submitWorkspaceAction = (mode: WorkspaceAction, path: string) => {
    if (loading) return;
    setDialogMode(undefined);
    if (mode === 'open') {
      void open(path);
    } else {
      void initialize(path, template);
    }
  };

  const openWorkspaceFromPicker = async () => {
    if (loading || pickerOpening) return;
    setPickerOpening(true);
    try {
      const selectedRoot = await api.chooseWorkspaceDirectory();
      if (selectedRoot) await open(selectedRoot);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setPickerOpening(false);
    }
  };

  const handleOpenWorkspace = () => {
    if (isDesktopBackend) {
      void openWorkspaceFromPicker();
      return;
    }
    setDialogMode('open');
  };

  const removeWorkspace = (workspace: string) => {
    if (loading) return;
    removeRecentWorkspace(workspace);
  };

  return (
    <div className="flex h-dvh overflow-hidden bg-paper text-ink">
      <div className="mx-auto flex h-full w-full max-w-6xl flex-col px-6 py-6 lg:px-10">
        <header className="flex shrink-0 flex-wrap items-end justify-between gap-4 border-b border-stone-200 pb-5">
          <div>
            <h1 className="text-2xl font-semibold lg:text-3xl">Choose a workspace</h1>
            <p className="mt-2 text-sm leading-6 text-stone-600">
              Continue a recent library, or start a new one.
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              disabled={pickerOpening || loading}
              className="inline-flex h-10 items-center justify-center gap-2 rounded-md bg-moss px-3 text-sm font-medium text-white"
              onClick={handleOpenWorkspace}
            >
              <FolderOpen className="size-4" />
              {pickerOpening ? 'Opening...' : 'Open workspace'}
            </button>
            <button
              type="button"
              className="inline-flex h-10 items-center justify-center gap-2 rounded-md border border-stone-300 px-3 text-sm font-medium hover:bg-stone-100"
              onClick={() => setDialogMode('create')}
            >
              <Plus className="size-4" />
              New workspace
            </button>
          </div>
        </header>

        {error ? (
          <div
            role="alert"
            className="mt-4 shrink-0 rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700"
          >
            {error}
          </div>
        ) : null}

        <main className="mt-5 flex min-h-0 flex-1 flex-col">
          <section
            aria-labelledby="recent-workspaces-heading"
            className="flex min-h-0 flex-col rounded-lg border border-stone-200 bg-white"
          >
            <div className="shrink-0 border-b border-stone-200 px-5 py-4">
              <h2
                id="recent-workspaces-heading"
                className="flex items-center gap-2 text-base font-semibold"
              >
                <Clock3 className="size-4 text-stone-500" />
                Recent workspaces
              </h2>
              <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
                <p className="text-sm text-stone-500">
                  Reopen a research library. The most recent path appears first.
                </p>
                <div className="relative w-full sm:max-w-80">
                  <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-stone-400" />
                  <label className="sr-only" htmlFor="workspace-search">
                    Search workspaces
                  </label>
                  <input
                    id="workspace-search"
                    type="search"
                    value={workspaceQuery}
                    onChange={(event) => setWorkspaceQuery(event.target.value)}
                    placeholder="Search paths"
                    autoComplete="off"
                    className="h-9 w-full rounded-md border border-stone-300 pl-9 pr-3 text-sm outline-none focus:border-moss"
                  />
                </div>
              </div>
            </div>

            {visibleWorkspaces.length > 0 ? (
              <div className="min-h-0 flex-1 overflow-y-auto">
                <ul className="divide-y divide-stone-200">
                  {visibleWorkspaces.map((workspace, index) => (
                    <li key={workspace}>
                      <div className="flex w-full items-center gap-3 px-5 py-3">
                        <button
                          type="button"
                          disabled={loading}
                          onClick={() => void open(workspace)}
                          className="flex min-w-0 flex-1 items-center gap-4 rounded-md py-1 text-left hover:bg-stone-50 disabled:opacity-50"
                        >
                          <span className="w-6 font-mono text-xs text-stone-400">
                            {String(index + 1).padStart(2, '0')}
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="block truncate font-mono text-sm" title={workspace}>
                              {workspace}
                            </span>
                            <span className="mt-1 flex items-center gap-2 text-xs text-stone-500">
                              {index === 0 ? (
                                <span className="rounded bg-moss/10 px-1.5 py-0.5 font-medium text-moss">
                                  Last opened
                                </span>
                              ) : null}
                              <span>Open this workspace</span>
                            </span>
                          </span>
                        </button>
                        <div className="flex shrink-0 items-center gap-1.5">
                          <button
                            type="button"
                            disabled={loading}
                            aria-label={`Remove ${workspace} from recent workspaces`}
                            title="Remove from recent workspaces"
                            className="flex size-9 items-center justify-center rounded-md border border-stone-300 text-stone-500 hover:border-red-200 hover:bg-red-50 hover:text-red-600 disabled:opacity-50"
                            onClick={() => removeWorkspace(workspace)}
                          >
                            <Trash2 className="size-4" />
                          </button>
                        </div>
                      </div>
                    </li>
                  ))}
                </ul>
              </div>
            ) : recentWorkspaces.length > 0 ? (
              <div className="flex flex-1 items-center px-5">
                <p className="text-sm text-stone-500">No workspaces match this search.</p>
              </div>
            ) : (
              <div
                data-testid="recent-workspaces-empty"
                className="flex min-h-32 flex-1 items-center justify-center px-5"
              >
                <p className="text-sm text-stone-500">
                  No workspaces yet. Create your first workspace to get started.
                </p>
              </div>
            )}
          </section>
        </main>
      </div>

      {dialogMode ? (
        <WorkspaceActionDialog
          mode={dialogMode}
          path={dialogPath}
          template={template}
          loading={loading}
          onPathChange={setDialogPath}
          onTemplateChange={setTemplate}
          onClose={() => setDialogMode(undefined)}
          onSubmit={submitWorkspaceAction}
        />
      ) : null}
    </div>
  );
}
