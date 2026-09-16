import { useState } from 'react';
import { FolderOpen, Plus } from 'lucide-react';
import { useWorkspaceStore } from '../store/useWorkspaceStore';
import type { TemplateId } from '../types';

export function WorkspaceSetup() {
  const root = useWorkspaceStore((state) => state.root);
  const recentWorkspaces = useWorkspaceStore((state) => state.recentWorkspaces);
  const [selectedRoot, setSelectedRoot] = useState(root);
  const [template, setTemplate] = useState<TemplateId>('research');
  const initialize = useWorkspaceStore((state) => state.initialize);
  const open = useWorkspaceStore((state) => state.open);
  const loading = useWorkspaceStore((state) => state.loading);
  const error = useWorkspaceStore((state) => state.error);

  return (
    <div className="flex min-h-screen items-center justify-center bg-paper p-6 text-ink">
      <form
        className="w-full max-w-xl space-y-5 rounded-lg border border-stone-200 bg-white p-6"
        onSubmit={(event) => {
          event.preventDefault();
          if (selectedRoot.trim()) void open(selectedRoot.trim());
        }}
      >
        <div>
          <h1 className="text-xl font-semibold">CorpusBot Workspace</h1>
          <p className="mt-1 text-sm text-stone-500">
            Open an existing CorpusBot workspace or create a fresh one.
          </p>
        </div>
        {error ? (
          <div
            role="alert"
            className="rounded-md border border-red-200 bg-red-50 p-3 text-sm text-red-700"
          >
            {error}
          </div>
        ) : null}
        <label className="block space-y-2">
          <span className="text-sm font-medium">Workspace path</span>
          <input
            value={selectedRoot}
            onChange={(event) => setSelectedRoot(event.target.value)}
            placeholder="/Users/you/Documents/research-wiki"
            className="w-full rounded-md border border-stone-300 px-3 py-2 text-sm"
            required
          />
        </label>
        <fieldset className="space-y-2">
          <legend className="text-sm font-medium">Template</legend>
          <div className="grid grid-cols-2 gap-2">
            {(['research', 'generic'] as TemplateId[]).map((option) => (
              <label
                key={option}
                className={`cursor-pointer rounded-md border px-3 py-2 text-sm capitalize ${
                  template === option ? 'border-moss bg-moss/10 text-moss' : 'border-stone-300'
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
        <div className="flex gap-2">
          <button
            type="submit"
            disabled={loading}
            className="inline-flex items-center gap-2 rounded-md bg-moss px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          >
            <FolderOpen className="size-4" />
            Open
          </button>
          <button
            type="button"
            disabled={loading || !root.trim()}
            onClick={() => {
              if (root.trim()) void initialize(root.trim(), template);
            }}
            className="inline-flex items-center gap-2 rounded-md border border-stone-300 px-3 py-2 text-sm font-medium disabled:opacity-50"
          >
            <Plus className="size-4" />
            Create
          </button>
        </div>
        {recentWorkspaces.length > 0 ? (
          <div className="space-y-2">
            <p className="text-sm font-medium">Recent workspaces</p>
            <ul className="space-y-1">
              {recentWorkspaces.map((recentRoot) => (
                <li key={recentRoot}>
                  <button
                    type="button"
                    disabled={loading}
                    className="w-full truncate rounded-md border border-stone-200 px-3 py-2 text-left text-sm hover:bg-stone-100 disabled:opacity-50"
                    title={recentRoot}
                    onClick={() => void open(recentRoot)}
                  >
                    {recentRoot}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </form>
    </div>
  );
}
