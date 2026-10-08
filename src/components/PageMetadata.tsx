import { BookMarked, CalendarClock, FileText, Link2, Tag } from 'lucide-react';
import type { WikiPage } from '../types';
import { MetadataChip, MetadataField, MetadataPanel, cleanWikilink } from './MetadataPanel';

function formatBytes(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

type RawReference = NonNullable<WikiPage['raw']>;

function RawDetails({ raw }: { raw: RawReference }) {
  return (
    <dl className="grid gap-x-5 gap-y-3 sm:grid-cols-2 xl:grid-cols-4">
      <MetadataField
        label="File"
        value={
          <span className="block truncate" title={raw.originalName}>
            {raw.originalName}
          </span>
        }
      />
      <MetadataField
        label="Captured path"
        value={
          <span className="block break-all font-mono text-xs leading-5" title={raw.path}>
            {raw.path}
          </span>
        }
      />
      <MetadataField
        label="SHA-256"
        value={
          <span className="block break-all font-mono text-xs leading-5" title={raw.sha256}>
            sha256:{raw.sha256}
          </span>
        }
      />
      <MetadataField label="Size" value={formatBytes(raw.size)} />
    </dl>
  );
}

function SourceProvenance({ page }: { page: WikiPage }) {
  const { raw, sourceReferences } = page;
  const rawUnderSource = sourceReferences.length === 1 && raw;

  return (
    <section
      aria-labelledby="source-provenance-heading"
      className="mt-5 border-t border-stone-200 pt-4"
    >
      <h4 id="source-provenance-heading" className="text-xs font-medium text-stone-500">
        Source provenance
      </h4>

      {sourceReferences.length > 0 ? (
        <ol className="mt-3 space-y-4">
          {sourceReferences.map((source) => (
            <li key={source.sourceVersionId} className="min-w-0">
              <h5 className="flex min-w-0 flex-wrap items-center gap-1.5 text-sm font-medium">
                <BookMarked className="size-3.5 shrink-0 text-stone-500" />
                <span className="min-w-0 truncate" title={source.title}>
                  {source.title}
                </span>
              </h5>
              <p className="mt-1 min-w-0">
                <code
                  className="inline-block max-w-full truncate rounded-md bg-stone-100 px-2 py-0.5 font-mono text-xs text-stone-600"
                  title={source.sourceVersionId}
                >
                  {source.sourceVersionId}
                </code>
              </p>
              {rawUnderSource ? (
                <div className="mt-3 border-t border-stone-100 pt-3">
                  <h6 className="flex items-center gap-1.5 text-xs font-medium text-stone-500">
                    <FileText className="size-3 shrink-0" />
                    Captured raw
                  </h6>
                  <div className="mt-2">
                    <RawDetails raw={raw} />
                  </div>
                </div>
              ) : null}
            </li>
          ))}
        </ol>
      ) : null}

      {raw && !rawUnderSource ? (
        <div className="mt-4 border-t border-stone-100 pt-3">
          <h5 className="flex items-center gap-1.5 text-xs font-medium text-stone-500">
            <FileText className="size-3 shrink-0" />
            Captured raw
          </h5>
          <div className="mt-2">
            <RawDetails raw={raw} />
          </div>
        </div>
      ) : null}
    </section>
  );
}

export function PageMetadata({ page }: { page: WikiPage }) {
  const raw = page.raw;

  return (
    <MetadataPanel labelId="page-metadata-heading">
      <dl className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <MetadataField label="Type" value={<span className="capitalize">{page.pageType}</span>} />
        <MetadataField
          label="Created"
          value={
            <span className="flex items-center gap-1.5">
              <CalendarClock className="size-3.5 shrink-0 text-stone-500" />
              {page.createdAt}
            </span>
          }
        />
        <MetadataField
          label="Updated"
          value={
            <span className="flex items-center gap-1.5">
              <CalendarClock className="size-3.5 shrink-0 text-stone-500" />
              {page.updatedAt}
            </span>
          }
        />
        {page.tags.length > 0 ? (
          <MetadataField
            label="Tags"
            value={
              <span className="flex flex-wrap gap-1.5">
                {page.tags.map((tag) => (
                  <MetadataChip key={tag} icon={<Tag className="size-3 shrink-0" />}>
                    {tag}
                  </MetadataChip>
                ))}
              </span>
            }
          />
        ) : null}
        {page.aliases.length > 0 ? (
          <MetadataField
            label="Aliases"
            value={
              <span className="flex flex-wrap gap-1.5">
                {page.aliases.map((alias) => (
                  <MetadataChip key={alias}>{alias}</MetadataChip>
                ))}
              </span>
            }
          />
        ) : null}
        {page.related.length > 0 ? (
          <MetadataField
            label="Related"
            value={
              <span className="flex flex-wrap gap-1.5">
                {page.related.map((related) => (
                  <MetadataChip key={related} icon={<Link2 className="size-3 shrink-0" />}>
                    {cleanWikilink(related)}
                  </MetadataChip>
                ))}
              </span>
            }
          />
        ) : null}
      </dl>
      {page.sourceReferences.length > 0 || raw ? <SourceProvenance page={page} /> : null}
    </MetadataPanel>
  );
}
