import { useState } from 'react';
import { ArrowRight, Clock3, FolderOpen, Plus } from 'lucide-react';
import { useWorkspaceStore } from '../store/useWorkspaceStore';
import type { TemplateId } from '../types';

export function WorkspaceSetup() {
  const root = useWorkspaceStore((state) => state.root);
  const recentWorkspaces = useWorkspaceStore((state) => state.recentWorkspaces);
  const [newRoot, setNewRoot] = useState('');
  const [template, setTemplate] = useState<TemplateId>('research');
  const initialize = useWorkspaceStore((state) => state.initialize);
  const open = useWorkspaceStore((state) => state.open);
  const loading = useWorkspaceStore((state) => state.loading);
  const error = useWorkspaceStore((state) => state.error);

  const lastWorkspace = recentWorkspaces[0] ?? root;
  const earlierWorkspaces = recentWorkspaces.filter((workspace) => workspace !== lastWorkspace);

  return (
    <div className="min-h-screen bg-paper text-ink">
      <div className="mx-auto w-full max-w-6xl px-6 py-10 lg:px-10 lg:py-16">
        <header className="max-w-3xl">
          <p className="text-xs font-medium tracking-widest text-moss uppercase">
            Local knowledge engine
          </p>
          <h1 className="mt-3 text-3xl font-semibold lg:text-4xl">Choose a workspace</h1>
          <p className="mt-3 max-w-2xl text-sm leading-6 text-stone-600">
            Continue an existing research library, or create a dedicated workspace for a new
            subject.
          </p>
        </header>

        {error ? (
          <div
            role="alert"
            className="mt-8 rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700"
          >
            {error}
          </div>
        ) : null}

        <div className="mt-8 grid items-start gap-6 lg:grid-cols-[minmax(0, 1fr)_400px]">
          <section
            aria-labelledby="recent-workspaces-heading"
            className="order-2 rounded-lg border border-stone-200 bg-white lg:order-1"
          >
            <div className="border-b border-stone-200 p-5">
              <h2
                id="recent-workspaces-heading"
                className="flex items-center gap-2 text-base font-semibold"
              >
                <Clock3 className="size-4 text-stone-500" />
                Recent workspaces
              </h2>
              <p className="mt-1 text-sm text-stone-500">
                Choose a library to reopen. The most recent path appears first.
              </p>
            </div>

            {lastWorkspace ? (
              <div className="p-5">
                <div className="flex flex-col gap-3 rounded-md border border-moss/20 bg-moss/5 p-4 sm:flex-row sm:items-center sm:justify-between">
                  <div className="min-w-0">
                    <p className="text-xs font-medium text-moss uppercase">Last opened</p>
                    <p className="mt-1 truncate font-mono text-sm" title={lastWorkspace}>
                      {lastWorkspace}
                    </p>
                  </div>
                  <button
                    type="button"
                    disabled={loading}
                    onClick={() => void open(lastWorkspace)}
                    className="inline-flex shrink-0 items-center justify-center gap-2 rounded-md bg-moss px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
                  >
                    Open
                    <ArrowRight className="size-4" />
                  </button>
                </div>

                {earlierWorkspaces.length > 0 ? (
                  <ul className="mt-5 divide-y divide-stone-200">
                    {earlierWorkspaces.map((workspace, index) => (
                      <li key={workspace}>
                        <button
                          type="button"
                          disabled={loading}
                          onClick={() => void open(workspace)}
                          className="flex w-full items-center gap-4 py-3 text-left hover:bg-stone-50 disabled:opacity-50"
                        >
                          <span className="w-6 font-mono text-xs text-stone-400">
                            {String(index + 1).padStart(2, '0')}
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="block truncate font-mono text-sm">{workspace}</span>
                            <span className="mt-1 block text-xs text-stone-500">
                              Open this workspace
                            </span>
                          </span>
                          <ArrowRight className="size-4 shrink-0 text-stone-400" />
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="mt-4 text-sm text-stone-500">No earlier workspaces.</p>
                )}
              </div>
            ) : (
              <div className="p-5">
                <p className="text-sm text-stone-500">
                  No workspaces yet. Create your first workspace to get started.
                </p>
              </div>
            )}
          </section>

          <section
            aria-labelledby="new-workspace-heading"
            className="order-1 rounded-lg border border-stone-200 bg-white lg:order-2"
          >
            <div className="border-b border-stone-200 p-5">
              <h2
                id="new-workspace-heading"
                className="flex items-center gap-2 text-base font-semibold"
              >
                <FolderOpen className="size-4 text-stone-500" />
                Open or create
              </h2>
              <p className="mt-1 text-sm text-stone-500">
                Open an existing path, or create a fresh workspace.
              </p>
            </div>

            <form
              className="space-y-5 p-5"
              onSubmit={(event) => {
                event.preventDefault();
                if (newRoot.trim()) void open(newRoot.trim());
              }}
            >
              <label className="block space-y-2">
                <span className="text-sm font-medium">Workspace path</span>
                <input
                  value={newRoot}
                  onChange={(event) => setNewRoot(event.target.value)}
                  placeholder="/Users/you/Documents/research-wiki"
                  className="w-full rounded-md border border-stone-300 px-3 py-2 font-mono text-sm"
                  required
                />
              </label>

              <fieldset className="space-y-2">
                <legend className="text-sm font-medium">Template for new workspaces</legend>
                <div className="grid grid-cols-2 gap-2">
                  {(['research', 'generic'] as TemplateId[]).map((option) => (
                    <label
                      key={option}
                      className={`cursor-pointer rounded-md border px-3 py-2 text-sm capitalize ${
                        template === option
                          ? 'border-moss bg-moss/10 text-moss'
                          : 'border-stone-300'
                      }`}
                    >
                      <input
                        type="radio"
                        name="template"
                        value={option}
                        checked={template === option}
                        onChange={() => setTemplate(option)}
                        className="sr-only"
                      />
                      {option}
                    </label>
                  ))}
                </div>
              </fieldset>

              <div className="flex flex-col gap-2">
                <button
                  type="submit"
                  disabled={loading || !newRoot.trim()}
                  className="inline-flex items-center justify-center gap-2 rounded-md bg-moss px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
                >
                  <FolderOpen className="size-4" />
                  Open path
                </button>
                <button
                  type="button"
                  disabled={loading || !newRoot.trim()}
                  onClick={() => {
                    if (newRoot.trim()) void initialize(newRoot.trim(), template);
                  }}
                  className="inline-flex items-center justify-center gap-2 rounded-md border border-stone-300 px-3 py-2 text-sm font-medium disabled:opacity-50"
                >
                  <Plus className="size-4" />
                  Create workspace
                </button>
              </div>
            </form>
          </section>
        </div>
      </div>
    </div>
  );
}
