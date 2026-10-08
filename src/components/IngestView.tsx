import { useEffect, useMemo, useState } from 'react';
import {
  ArrowUpRight,
  CircleCheck,
  CircleAlert,
  Clock3,
  FileCode2,
  RefreshCw,
  TriangleAlert,
} from 'lucide-react';
import { IngestJobProgress, IngestJobStepList } from './IngestJobProgress';
import { useWorkspaceStore } from '../store/useWorkspaceStore';
import type { IngestRunRow } from '../types';

function statusAccent(status: string) {
  if (status === 'committed') return 'bg-moss/10 text-moss';
  if (status === 'failed') return 'bg-red-50 text-red-600';
  return 'bg-amber-50 text-amber-600';
}

function formatBytes(size?: number | null) {
  if (size === undefined || size === null) return 'Unknown size';
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function formatTimestamp(value: string) {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime())
    ? value
    : parsed.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

function runSummary(run: IngestRunRow) {
  const wikiResources = run.touchedResources.filter((resource) =>
    resource.path.startsWith('wiki/'),
  );
  return {
    created: wikiResources.filter((resource) => resource.revisionKind === 'absent').length,
    updated: wikiResources.filter((resource) => resource.revisionKind !== 'absent').length,
  };
}

function outputLimitReached(event: { finishReason?: string | null; truncated?: boolean | null }) {
  return event.finishReason === 'length' || event.truncated === true;
}

function ImportButton({ disabled = false }: { disabled?: boolean }) {
  const importMarkdown = useWorkspaceStore((state) => state.importMarkdown);

  return (
    <label
      className={`inline-flex h-8 shrink-0 items-center gap-2 rounded-md px-3 text-sm font-medium text-white ${
        disabled ? 'cursor-not-allowed bg-moss/50' : 'cursor-pointer bg-moss'
      }`}
    >
      <RefreshCw className="size-4" />
      Import
      <input
        type="file"
        accept=".md,.markdown,text/markdown"
        className="sr-only"
        disabled={disabled}
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (!file) return;
          void file.text().then((markdown) => importMarkdown(file.name, markdown));
          event.target.value = '';
        }}
      />
    </label>
  );
}

export function IngestView() {
  const runs = useWorkspaceStore((state) => state.ingestRuns);
  const jobs = useWorkspaceStore((state) => state.ingestJobs);
  const selectedId = useWorkspaceStore((state) => state.selectedIngestRunId);
  const detail = useWorkspaceStore((state) => state.ingestRunDetail);
  const listLoading = useWorkspaceStore((state) => state.ingestLoading);
  const detailLoading = useWorkspaceStore((state) => state.ingestDetailLoading);
  const loadIngestRuns = useWorkspaceStore((state) => state.loadIngestRuns);
  const selectIngestRun = useWorkspaceStore((state) => state.selectIngestRun);
  const selectPage = useWorkspaceStore((state) => state.selectPage);
  const loadRawSource = useWorkspaceStore((state) => state.loadRawSource);
  const setActiveView = useWorkspaceStore((state) => state.setActiveView);
  const [query, setQuery] = useState('');

  useEffect(() => {
    void loadIngestRuns();
  }, [loadIngestRuns]);

  const visibleRuns = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return runs;
    return runs.filter((run) =>
      [run.originalName, run.runId, run.sourceVersionId]
        .filter(Boolean)
        .some((value) => value?.toLowerCase().includes(normalized)),
    );
  }, [query, runs]);
  const activeJobs = jobs.filter((job) => ['queued', 'running'].includes(job.status));
  const importing = activeJobs.length > 0;
  const failedJobs = jobs.filter(
    (job) => job.status === 'failed' && !runs.some((run) => run.runId === job.runId),
  );
  const affectedResources = useMemo(() => {
    if (!detail) return [];
    const resources = [...detail.touchedResources];
    if (detail.sourcePage && !resources.some((item) => item.path === detail.sourcePage)) {
      const rawIndex = resources.findIndex((item) => item.path.startsWith('raw/'));
      resources.splice(rawIndex + 1, 0, {
        path: detail.sourcePage,
        revisionKind: 'absent',
      });
    }
    return resources;
  }, [detail]);

  const activeJob = activeJobs[0];

  const openResource = (path: string, sourceVersionId?: string | null) => {
    setActiveView('documents');
    if (path.startsWith('raw/')) {
      if (!sourceVersionId) return;
      void loadRawSource(sourceVersionId, path);
      return;
    }
    void selectPage(path);
  };

  return (
    <div className="grid min-h-0 flex-1 lg:grid-cols-[24rem_minmax(0,1fr)]">
      <aside className="hidden min-h-0 min-w-0 flex-col border-r border-stone-200 bg-white lg:flex">
        <header className="flex h-12 shrink-0 items-center justify-between gap-2 border-b border-stone-200 px-4">
          <h2 className="text-sm font-semibold">Ingest</h2>
          <ImportButton disabled={importing} />
        </header>
        <div className="shrink-0 border-b border-stone-200 p-2">
          <label className="sr-only" htmlFor="ingest-run-search">
            Filter ingest runs
          </label>
          <input
            id="ingest-run-search"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter imports"
            autoComplete="off"
            className="h-9 w-full rounded-md border border-stone-300 px-3 text-sm outline-none focus:border-moss"
          />
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          {activeJobs.length > 0 ? (
            <ul className="border-b border-stone-200 bg-stone-50">
              {activeJobs.map((job) => (
                <li key={job.jobId} className="px-4 py-3 text-sm">
                  <div className="flex items-center justify-between gap-2">
                    <span className="min-w-0 truncate font-medium" title={job.fileName}>
                      {job.fileName}
                    </span>
                    <span className="shrink-0 rounded bg-amber-100 px-1.5 py-0.5 text-[11px] font-medium capitalize text-amber-700">
                      {job.status}
                    </span>
                  </div>
                  <div className="mt-3">
                    <IngestJobProgress job={job} />
                  </div>
                  <p className="mt-1 text-xs text-stone-500">
                    Ingest is running in the background. You can keep using the app.
                  </p>
                </li>
              ))}
            </ul>
          ) : null}
          {failedJobs.length > 0 ? (
            <div className="border-b border-stone-200 bg-white">
              <h3 className="px-4 pt-3 text-xs font-semibold uppercase tracking-wide text-red-600">
                Failed imports
              </h3>
              <ul className="pb-2">
                {failedJobs.map((job) => (
                  <li key={job.jobId} className="px-4 py-3 text-sm">
                    <div className="flex items-start justify-between gap-2">
                      <span className="min-w-0 truncate font-medium" title={job.fileName}>
                        {job.fileName}
                      </span>
                      <span className="shrink-0 rounded bg-red-50 px-1.5 py-0.5 text-[11px] font-medium capitalize text-red-600">
                        Failed
                      </span>
                    </div>
                    <p
                      className="mt-1 line-clamp-3 text-xs text-red-600"
                      title={job.error ?? 'Import failed'}
                    >
                      {job.error ?? 'Import failed'}
                    </p>
                    <p className="mt-1 text-xs text-stone-500">
                      {formatTimestamp(new Date(job.createdAtMs).toISOString())}
                    </p>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          {listLoading && runs.length === 0 ? (
            <p className="p-4 text-sm text-stone-500">Loading ingest history</p>
          ) : visibleRuns.length === 0 ? (
            <p className="p-4 text-sm text-stone-500">
              {runs.length === 0 ? 'No imports yet.' : 'No imports match this filter.'}
            </p>
          ) : (
            <ul className="divide-y divide-stone-200">
              {visibleRuns.map((run) => {
                const summary = runSummary(run);
                const selected = selectedId === run.runId;
                return (
                  <li key={run.runId}>
                    <button
                      type="button"
                      aria-current={selected ? 'true' : undefined}
                      className={`w-full border-l-2 px-4 py-3 text-left transition-colors ${
                        selected ? 'border-moss bg-moss/5' : 'border-transparent hover:bg-stone-50'
                      }`}
                      onClick={() => void selectIngestRun(run.runId)}
                    >
                      <span className="flex items-center justify-between gap-2">
                        <span
                          className="min-w-0 truncate text-sm font-medium"
                          title={run.originalName ?? run.runId}
                        >
                          {run.originalName ?? run.runId}
                        </span>
                        <span
                          className={`shrink-0 rounded px-1.5 py-0.5 text-[11px] font-medium capitalize ${statusAccent(run.status)}`}
                        >
                          {run.status}
                        </span>
                      </span>
                      <span className="mt-1 flex items-center gap-2 text-xs text-stone-500">
                        <Clock3 className="size-3" />
                        <span className="truncate">{formatTimestamp(run.createdAt)}</span>
                      </span>
                      <span className="mt-1 block truncate text-xs text-stone-500">
                        {summary.created} new · {summary.updated} updated · {formatBytes(run.size)}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </aside>

      <section className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] bg-paper">
        <div className="flex h-12 shrink-0 items-center justify-between gap-3 border-b border-stone-200 bg-white px-4">
          <span
            className="min-w-0 truncate font-mono text-xs text-stone-500"
            title={activeJob?.fileName ?? selectedId}
          >
            {activeJob ? activeJob.fileName : (selectedId ?? 'No ingest selected')}
          </span>
        </div>

        <div className="min-h-0 overflow-auto p-6">
          {activeJobs.length > 0 ? (
            <section
              aria-labelledby="running-ingest-heading"
              className="mx-auto max-w-5xl rounded-md border border-stone-200 bg-white p-4"
            >
              <h3 id="running-ingest-heading" className="text-base font-semibold">
                Running import
              </h3>
              <ul className="mt-3 space-y-4">
                {activeJobs.map((job) => (
                  <li key={job.jobId}>
                    <div className="flex items-center justify-between gap-2">
                      <span className="min-w-0 truncate text-sm font-medium" title={job.fileName}>
                        {job.fileName}
                      </span>
                      <span className="shrink-0 rounded bg-amber-100 px-1.5 py-0.5 text-[11px] font-medium capitalize text-amber-700">
                        {job.status}
                      </span>
                    </div>
                    <IngestJobStepList job={job} />
                  </li>
                ))}
              </ul>
            </section>
          ) : null}

          {detailLoading ? (
            <p className="text-sm text-stone-500">Loading ingest details</p>
          ) : !detail && activeJobs.length === 0 ? (
            <p className="text-sm text-stone-500">Select an import to inspect its details.</p>
          ) : detail ? (
            <div className="mx-auto max-w-5xl space-y-6">
              <div className="flex flex-wrap items-center gap-2">
                <span
                  className={`rounded px-2 py-1 text-xs font-medium capitalize ${statusAccent(detail.status)}`}
                >
                  {detail.status}
                </span>
                <span className="text-sm text-stone-500">
                  Started {formatTimestamp(detail.createdAt)}
                </span>
                {detail.finishedAt ? (
                  <span className="text-sm text-stone-500">
                    Finished {formatTimestamp(detail.finishedAt)}
                  </span>
                ) : null}
              </div>

              {detail.events.some(outputLimitReached) ? (
                <div
                  role="alert"
                  className="flex items-start gap-2 rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-800"
                >
                  <TriangleAlert className="mt-0.5 size-4 shrink-0 text-amber-600" />
                  <span>
                    Output token limit reached. Increase Draft output token limit in Settings or
                    narrow the selected pages, then retry.
                  </span>
                </div>
              ) : null}

              <section aria-labelledby="ingest-files-heading" className="space-y-2">
                <h3 id="ingest-files-heading" className="text-base font-semibold">
                  Affected resources
                </h3>
                <ul className="overflow-hidden rounded-md border border-stone-200 bg-white">
                  {affectedResources.map((resource) => (
                    <li key={resource.path} className="border-b border-stone-100 last:border-b-0">
                      <button
                        type="button"
                        className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm transition-colors hover:bg-stone-50"
                        onClick={() => openResource(resource.path, detail.sourceVersionId)}
                      >
                        <span className="min-w-0 flex-1 truncate font-mono" title={resource.path}>
                          {resource.path}
                        </span>
                        <span
                          className={`shrink-0 rounded px-1.5 py-0.5 text-xs ${
                            resource.revisionKind === 'absent'
                              ? 'bg-moss/10 text-moss'
                              : 'bg-stone-100 text-stone-600'
                          }`}
                        >
                          {resource.revisionKind === 'absent' ? 'New' : 'Updated'}
                        </span>
                        <ArrowUpRight className="size-3.5 shrink-0 text-stone-400" />
                      </button>
                    </li>
                  ))}
                </ul>
              </section>

              <section aria-labelledby="ingest-events-heading" className="space-y-2">
                <h3 id="ingest-events-heading" className="text-base font-semibold">
                  Workflow events
                </h3>
                {detail.events.length === 0 ? (
                  <p className="text-sm text-stone-500">No audit events were recorded.</p>
                ) : (
                  <ul className="space-y-2">
                    {detail.events.map((event) => (
                      <li
                        key={event.eventId}
                        className="rounded-md border border-stone-200 bg-white p-3 text-sm"
                      >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <span className="font-medium capitalize">{event.node}</span>
                          <span className="flex items-center gap-1.5 text-xs text-stone-500">
                            {event.status === 'succeeded' ? (
                              <CircleCheck className="size-3.5 text-moss" />
                            ) : (
                              <CircleAlert className="size-3.5 text-amber-500" />
                            )}
                            <span className="capitalize">{event.status}</span>
                            <span>· attempt {event.attempt}</span>
                          </span>
                        </div>
                        {outputLimitReached(event) ? (
                          <p className="mt-2 text-xs font-medium text-amber-700">
                            Output token limit reached
                          </p>
                        ) : null}
                        <dl className="mt-2 grid gap-x-4 gap-y-1 text-xs text-stone-600 sm:grid-cols-2">
                          {event.model ? <div>Model: {event.model}</div> : null}
                          {event.latencyMs !== undefined && event.latencyMs !== null ? (
                            <div>Latency: {event.latencyMs} ms</div>
                          ) : null}
                          {event.tokensIn !== undefined && event.tokensIn !== null ? (
                            <div>Tokens in: {event.tokensIn}</div>
                          ) : null}
                          {event.tokensOut !== undefined && event.tokensOut !== null ? (
                            <div>Tokens out: {event.tokensOut}</div>
                          ) : null}
                          {event.decision ? <div>Decision: {event.decision}</div> : null}
                          {event.outputRef ? (
                            <div className="truncate font-mono" title={event.outputRef}>
                              <FileCode2 className="mr-1 inline size-3" />
                              {event.outputRef}
                            </div>
                          ) : null}
                          {event.errorCode ? <div>Error: {event.errorCode}</div> : null}
                        </dl>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}
