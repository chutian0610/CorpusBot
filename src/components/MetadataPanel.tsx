import type { ReactNode } from 'react';

export function MetadataChip({ icon, children }: { icon?: ReactNode; children: ReactNode }) {
  return (
    <span className="inline-flex max-w-full items-center gap-1 whitespace-normal rounded-md bg-stone-100 px-2 py-1 text-xs leading-5 text-stone-700">
      {icon}
      {children}
    </span>
  );
}

export function MetadataField({
  label,
  value,
  children,
  wide = false,
}: {
  label: string;
  value?: ReactNode;
  children?: ReactNode;
  wide?: boolean;
}) {
  return (
    <div className={wide ? 'min-w-0 sm:col-span-2 lg:col-span-3' : 'min-w-0'}>
      <dt className="text-xs font-medium text-stone-500">{label}</dt>
      <dd className="mt-1 min-w-0 text-sm leading-6">{children ?? value}</dd>
    </div>
  );
}

export function MetadataNone() {
  return <span className="text-sm text-stone-500">None</span>;
}

export function MetadataPanel({
  labelId,
  action,
  children,
}: {
  labelId: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section aria-labelledby={labelId} className="space-y-3">
      <div className="flex h-8 items-center justify-between gap-3">
        <h3 id={labelId} className="text-base font-semibold">
          Metadata
        </h3>
        {action}
      </div>
      <div className="rounded-md border border-stone-200 bg-white p-4">{children}</div>
    </section>
  );
}

export function cleanWikilink(value: string) {
  return value.replace(/^\[\[/, '').replace(/\]\]$/, '').split('|')[0];
}
