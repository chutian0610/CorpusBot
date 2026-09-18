import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';
import type { TemplateId } from '../types';

export type WorkspaceAction = 'open' | 'create';

type WorkspaceActionDialogProps = {
  mode: WorkspaceAction;
  path: string;
  template: TemplateId;
  loading: boolean;
  onPathChange: (path: string) => void;
  onTemplateChange: (template: TemplateId) => void;
  onClose: () => void;
  onSubmit: (mode: WorkspaceAction, path: string) => void;
};

export function WorkspaceActionDialog({
  mode,
  path,
  template,
  loading,
  onPathChange,
  onTemplateChange,
  onClose,
  onSubmit,
}: WorkspaceActionDialogProps) {
  const dialogRef = useRef<HTMLDialogElement>(null);
  const pathInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (mode && !dialog.open) {
      dialog.showModal();
      pathInputRef.current?.focus();
    }
  }, [mode]);

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    const handleCancel = (event: Event) => {
      event.preventDefault();
      onClose();
    };

    dialog.addEventListener('cancel', handleCancel);
    return () => dialog.removeEventListener('cancel', handleCancel);
  }, [onClose]);

  const title = mode === 'open' ? 'Open workspace' : 'Create workspace';
  const description =
    mode === 'open'
      ? 'Choose an existing CorpusBot workspace folder.'
      : 'Choose a folder path for the new workspace.';

  return (
    <dialog
      ref={dialogRef}
      aria-labelledby="workspace-action-title"
      aria-describedby="workspace-action-description"
      className="m-auto w-[min(calc(100vw-2rem),34rem)] rounded-lg border border-stone-200 bg-white p-0 text-ink shadow-xl"
    >
      <form
        className="flex min-h-0 flex-col"
        onSubmit={(event) => {
          event.preventDefault();
          const trimmedPath = path.trim();
          if (trimmedPath) onSubmit(mode, trimmedPath);
        }}
      >
        <header className="flex shrink-0 items-start justify-between gap-3 border-b border-stone-200 px-5 py-4">
          <div className="min-w-0">
            <h2 id="workspace-action-title" className="text-base font-semibold">
              {title}
            </h2>
            <p id="workspace-action-description" className="mt-1 text-sm text-stone-500">
              {description}
            </p>
          </div>
          <button
            type="button"
            aria-label="Close dialog"
            title="Close dialog"
            className="flex size-8 shrink-0 items-center justify-center rounded-md hover:bg-stone-100"
            onClick={onClose}
          >
            <X className="size-4" />
          </button>
        </header>

        <div className="space-y-4 px-5 py-4">
          <label className="block space-y-1 text-sm">
            <span className="font-medium">Workspace path</span>
            <input
              ref={pathInputRef}
              value={path}
              onChange={(event) => onPathChange(event.target.value)}
              placeholder="/Users/you/Documents/research-wiki"
              className="w-full rounded-md border border-stone-300 px-3 py-2 font-mono"
              required
            />
          </label>

          {mode === 'create' ? (
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
                      name="workspace-template"
                      value={option}
                      checked={template === option}
                      onChange={() => onTemplateChange(option)}
                      className="sr-only"
                    />
                    {option}
                  </label>
                ))}
              </div>
            </fieldset>
          ) : null}
        </div>

        <footer className="flex shrink-0 items-center justify-end gap-2 border-t border-stone-200 px-5 py-4">
          <button
            type="button"
            className="rounded-md border border-stone-300 px-3 py-2 text-sm font-medium hover:bg-stone-100"
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={loading || !path.trim()}
            className="rounded-md bg-moss px-3 py-2 text-sm font-medium text-white disabled:opacity-50"
          >
            {mode === 'open' ? 'Open' : 'Create'}
          </button>
        </footer>
      </form>
    </dialog>
  );
}
