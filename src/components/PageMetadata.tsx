import { CalendarClock, Link2, Tag, UserRound } from 'lucide-react';
import type { WikiPage } from '../types';
import type { ReactNode } from 'react';

function cleanWikilink(value: string) {
  return value.replace(/^\[\[/, '').replace(/\]\]$/, '');
}

function MetadataItem({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs font-medium text-stone-500">{label}</dt>
      <dd className="mt-1 min-w-0 text-sm">{value}</dd>
    </div>
  );
}

function Chip({ children }: { children: ReactNode }) {
  return (
    <span className="max-w-full truncate rounded-md bg-stone-100 px-2 py-1 text-xs text-stone-700">
      {children}
    </span>
  );
}

export function PageMetadata({ page }: { page: WikiPage }) {
  return (
    <section aria-labelledby="page-metadata-heading" className="space-y-3">
      <h3 id="page-metadata-heading" className="text-base font-semibold">
        Metadata
      </h3>
      <dl className="grid gap-4 rounded-md border border-stone-200 bg-white p-4 sm:grid-cols-2 lg:grid-cols-3">
        <MetadataItem label="Type" value={<span className="capitalize">{page.pageType}</span>} />
        <MetadataItem
          label="Created"
          value={
            <span className="flex items-center gap-1.5">
              <CalendarClock className="size-3.5 text-stone-500" />
              {page.createdAt}
            </span>
          }
        />
        <MetadataItem
          label="Updated"
          value={
            <span className="flex items-center gap-1.5">
              <CalendarClock className="size-3.5 text-stone-500" />
              {page.updatedAt}
            </span>
          }
        />

        <MetadataItem
          label="Tags"
          value={
            page.tags.length ? (
              <span className="flex flex-wrap gap-1.5">
                {page.tags.map((tag) => (
                  <Chip key={tag}>
                    <span className="flex items-center gap-1">
                      <Tag className="size-3" />
                      {tag}
                    </span>
                  </Chip>
                ))}
              </span>
            ) : (
              <span className="text-stone-500">None</span>
            )
          }
        />
        <MetadataItem
          label="Aliases"
          value={
            page.aliases.length ? (
              <span className="flex flex-wrap gap-1.5">
                {page.aliases.map((alias) => (
                  <Chip key={alias}>{alias}</Chip>
                ))}
              </span>
            ) : (
              <span className="text-stone-500">None</span>
            )
          }
        />
        <MetadataItem
          label="Related"
          value={
            page.related.length ? (
              <span className="flex flex-wrap gap-1.5">
                {page.related.map((related) => (
                  <Chip key={related}>
                    <span className="flex items-center gap-1">
                      <Link2 className="size-3" />
                      {cleanWikilink(related).split('|')[1] ?? cleanWikilink(related).split('|')[0]}
                    </span>
                  </Chip>
                ))}
              </span>
            ) : (
              <span className="text-stone-500">None</span>
            )
          }
        />

        <div className="min-w-0 sm:col-span-2 lg:col-span-3">
          <dt className="text-xs font-medium text-stone-500">Sources</dt>
          <dd className="mt-1 min-w-0">
            {page.sourceReferences.length ? (
              <span className="flex flex-wrap gap-1.5">
                {page.sourceReferences.map((source) => (
                  <Chip key={source.sourceVersionId}>
                    <span className="flex items-center gap-1">
                      <UserRound className="size-3" />
                      {source.title}
                      <span className="font-mono text-stone-500">({source.sourceVersionId})</span>
                    </span>
                  </Chip>
                ))}
              </span>
            ) : (
              <span className="text-sm text-stone-500">None</span>
            )}
          </dd>
        </div>
      </dl>
    </section>
  );
}
