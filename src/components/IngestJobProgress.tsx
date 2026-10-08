import { CircleCheck, LoaderCircle } from 'lucide-react';
import type { IngestJob } from '../types';

export const INGEST_STAGES = [
  { id: 'prepare', label: 'Prepare workspace' },
  { id: 'analyze', label: 'Analyze source' },
  { id: 'draft', label: 'Generate drafts' },
  { id: 'commit', label: 'Commit wiki' },
] as const;

export function ingestProgress(job: Pick<IngestJob, 'status' | 'stage'>) {
  if (job.status === 'queued') {
    return { index: -1, label: INGEST_STAGES[0].label };
  }
  const index = Math.max(
    0,
    INGEST_STAGES.findIndex((stage) => stage.id === job.stage),
  );
  return { index, label: INGEST_STAGES[index].label };
}

export function IngestJobProgress({ job }: { job: IngestJob }) {
  const progress = ingestProgress(job);

  return (
    <div aria-live="polite">
      <div className="flex gap-1" aria-hidden="true">
        {INGEST_STAGES.map((stage, index) => (
          <span
            key={stage.id}
            className={`h-1 flex-1 rounded-full ${
              index <= progress.index ? 'bg-moss' : 'bg-stone-200'
            }`}
          />
        ))}
      </div>
      <div className="mt-2 flex items-center gap-1.5 text-xs text-stone-600">
        <LoaderCircle className="size-3 animate-spin" />
        <span className="min-w-0 truncate font-medium">{progress.label}</span>
        <span className="shrink-0 text-stone-400">
          {progress.index + 1}/{INGEST_STAGES.length}
        </span>
      </div>
    </div>
  );
}

export function IngestJobStepList({ job }: { job: IngestJob }) {
  const progress = ingestProgress(job);

  return (
    <ol className="mt-4 space-y-2">
      {INGEST_STAGES.map((stage, index) => {
        const isComplete = index < progress.index;
        const isActive = index === progress.index;

        return (
          <li
            key={stage.id}
            aria-current={isActive ? 'step' : undefined}
            className="flex items-center gap-2 text-sm"
          >
            {isComplete ? (
              <CircleCheck className="size-4 shrink-0 text-moss" />
            ) : isActive ? (
              <LoaderCircle className="size-4 shrink-0 animate-spin text-moss" />
            ) : (
              <span
                aria-hidden="true"
                className="size-4 shrink-0 rounded-full border border-stone-300"
              />
            )}
            <span
              className={
                isComplete
                  ? 'text-stone-600'
                  : isActive
                    ? 'font-medium text-stone-900'
                    : 'text-stone-500'
              }
            >
              {stage.label}
            </span>
          </li>
        );
      })}
    </ol>
  );
}
