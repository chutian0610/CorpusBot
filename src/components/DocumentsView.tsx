import { useEffect, useMemo } from 'react';
import { FileText } from 'lucide-react';
import { FileTree, type FileTreeItem } from './FileTree';
import { MarkdownPreview } from './MarkdownPreview';
import { PageMetadata } from './PageMetadata';
import { useWorkspaceStore } from '../store/useWorkspaceStore';

export function DocumentsView() {
  const documents = useWorkspaceStore((state) => state.documents);
  const selectedPath = useWorkspaceStore((state) => state.selectedPath);
  const selectedPage = useWorkspaceStore((state) => state.selectedPage);
  const loading = useWorkspaceStore((state) => state.documentsLoading);
  const loadDocuments = useWorkspaceStore((state) => state.loadDocuments);
  const selectPage = useWorkspaceStore((state) => state.selectPage);
  const rawSource = useWorkspaceStore((state) => state.rawSource);
  const rawSourceLoading = useWorkspaceStore((state) => state.rawSourceLoading);
  const loadRawSource = useWorkspaceStore((state) => state.loadRawSource);

  useEffect(() => {
    void loadDocuments();
  }, [loadDocuments]);

  useEffect(() => {
    const paths = new Set(
      documents.flatMap((document) => [
        document.sourcePage,
        ...(document.rawPath ? [document.rawPath] : []),
        ...document.pages.map((page) => page.path),
      ]),
    );
    if ((!selectedPath || !paths.has(selectedPath)) && documents.length > 0) {
      void selectPage(documents[0].sourcePage);
    }
  }, [documents, selectPage, selectedPath]);

  const fileItems: FileTreeItem[] = documents.flatMap((document) => [
    {
      path: document.sourcePage,
      label: document.sourcePage.split('/').at(-1) ?? document.sourcePage,
    },
    ...document.pages.map((page) => ({
      path: page.path,
      label: page.path.split('/').at(-1) ?? page.path,
    })),
    ...(document.rawPath
      ? [
          {
            path: document.rawPath,
            label: document.rawPath.split('/').at(-1) ?? document.rawPath,
          },
        ]
      : []),
  ]);

  const rawPaths = useMemo(
    () =>
      new Map<string, string>(
        documents
          .filter((document) => document.rawPath)
          .map((document) => [document.rawPath as string, document.sourceVersionId]),
      ),
    [documents],
  );

  const selectedDocument = documents.find(
    (document) =>
      document.sourcePage === selectedPath ||
      document.rawPath === selectedPath ||
      document.pages.some((page) => page.path === selectedPath),
  );
  const selectedDocumentPage = selectedDocument?.pages.find((page) => page.path === selectedPath);
  const selectedSource =
    selectedDocument?.sourcePage === selectedPath ? selectedDocument : undefined;
  const selectedIsRaw = selectedDocument?.rawPath === selectedPath;
  const selectedRaw = rawSource && rawSource.path === selectedPath ? rawSource : undefined;

  const selectTreeItem = (item: FileTreeItem) => {
    const sourceVersionId = rawPaths.get(item.path);
    if (sourceVersionId) {
      void loadRawSource(sourceVersionId, item.path);
    } else {
      void selectPage(item.path);
    }
  };

  useEffect(() => {
    const sourceVersionId = selectedPath ? rawPaths.get(selectedPath) : undefined;
    if (selectedPath && sourceVersionId && rawSource?.path !== selectedPath) {
      void loadRawSource(sourceVersionId, selectedPath);
    }
  }, [loadRawSource, rawPaths, rawSource?.path, selectedPath]);

  return (
    <div className="grid min-h-0 flex-1 lg:grid-cols-[20rem_minmax(0,1fr)]">
      <aside className="hidden min-h-0 min-w-0 flex-col border-r border-stone-200 bg-white lg:flex">
        <header className="flex h-12 shrink-0 items-center border-b border-stone-200 px-4">
          <h2 className="text-sm font-semibold">Documents</h2>
        </header>
        <div className="min-h-0 flex-1 overflow-auto p-2">
          {loading && documents.length === 0 ? (
            <p className="p-4 text-sm text-stone-500">Loading documents</p>
          ) : documents.length === 0 ? (
            <p className="p-4 text-sm text-stone-500">No imported documents yet.</p>
          ) : (
            <FileTree items={fileItems} selectedPath={selectedPath} onSelectFile={selectTreeItem} />
          )}
        </div>
      </aside>

      <section className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] bg-paper">
        <div className="flex h-12 shrink-0 items-center gap-3 border-b border-stone-200 bg-white px-4">
          <FileText className="size-4 shrink-0 text-stone-500" />
          <span className="min-w-0 truncate font-mono text-xs text-stone-500" title={selectedPath}>
            {selectedPath ?? 'No document selected'}
          </span>
        </div>

        <div className="min-h-0 overflow-auto p-6">
          {!selectedDocument ? (
            <p className="text-sm text-stone-500">Select a document to inspect its structure.</p>
          ) : selectedIsRaw && rawSourceLoading ? (
            <p className="text-sm text-stone-500">Loading raw source</p>
          ) : (
            <div className="mx-auto max-w-5xl space-y-6">
              <div>
                <h2 className="text-xl font-semibold">
                  {selectedRaw?.originalName ??
                    selectedSource?.title ??
                    selectedDocumentPage?.title ??
                    selectedPath}
                </h2>
                <p className="mt-1 text-sm text-stone-500">
                  {selectedRaw
                    ? `Raw source · ${selectedRaw.size} bytes`
                    : selectedSource
                      ? `Source version ${selectedDocument.sourceVersionId}`
                      : `${selectedDocumentPage?.pageType ?? 'Page'} · ${selectedPath}`}
                </p>
              </div>

              <section aria-labelledby="document-structure-heading" className="space-y-3">
                <h3 id="document-structure-heading" className="text-base font-semibold">
                  Extracted structure
                </h3>
                {selectedDocument.pages.length === 0 ? (
                  <p className="text-sm text-stone-500">
                    No entity or concept pages were extracted from this document.
                  </p>
                ) : (
                  <ul className="overflow-hidden rounded-md border border-stone-200 bg-white">
                    {selectedDocument.pages.map((page) => (
                      <li key={page.path} className="border-b border-stone-100 last:border-b-0">
                        <button
                          type="button"
                          className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm hover:bg-stone-50"
                          onClick={() => void selectPage(page.path)}
                        >
                          <span className="min-w-0">
                            <span className="block truncate font-medium">{page.title}</span>
                            <span
                              className="block truncate text-xs text-stone-500"
                              title={page.path}
                            >
                              {page.path}
                            </span>
                          </span>
                          <span className="shrink-0 rounded bg-stone-100 px-1.5 py-0.5 text-xs capitalize text-stone-600">
                            {page.pageType}
                          </span>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </section>

              {selectedIsRaw ? (
                <section aria-labelledby="raw-source-heading" className="space-y-3">
                  <h3 id="raw-source-heading" className="text-base font-semibold">
                    Raw source
                  </h3>
                  {rawSourceLoading ? (
                    <p className="text-sm text-stone-500">Loading raw source</p>
                  ) : selectedRaw ? (
                    <div className="rounded-md border border-stone-200 bg-white p-4">
                      <MarkdownPreview markdown={selectedRaw.markdown} />
                    </div>
                  ) : (
                    <p className="text-sm text-red-600">Raw source unavailable.</p>
                  )}
                </section>
              ) : selectedSource ? (
                <section aria-labelledby="document-source-heading" className="space-y-3">
                  <h3 id="document-source-heading" className="text-base font-semibold">
                    Source page
                  </h3>
                  <div className="rounded-md border border-stone-200 bg-white p-4">
                    <MarkdownPreview markdown={selectedPage?.body ?? ''} />
                  </div>
                </section>
              ) : null}

              {!selectedIsRaw && selectedPage ? <PageMetadata page={selectedPage} /> : null}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}
