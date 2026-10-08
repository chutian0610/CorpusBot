import type { Components } from 'react-markdown';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import type { Link, PhrasingContent, Root, Text } from 'mdast';

type MarkdownNode = {
  type?: unknown;
  value?: unknown;
  children?: unknown;
};

type NodeWithChildren = {
  type?: unknown;
  children: MarkdownNode[];
};

function isNodeWithChildren(node: MarkdownNode): node is NodeWithChildren {
  return Array.isArray(node.children);
}

function createWikilink(target: string, label: string): Link {
  const child: Text = { type: 'text', value: label };
  return {
    type: 'link',
    url: target,
    title: null,
    children: [child],
    data: {
      hProperties: {
        'data-corpusbot-wikilink': target,
      },
    },
  };
}

function parseWikilinks(value: string): PhrasingContent[] | null {
  const pattern = /\[\[([^\]\n]+)\]\]/g;
  const nodes: PhrasingContent[] = [];
  let cursor = 0;

  for (const match of value.matchAll(pattern)) {
    const start = match.index;
    if (start === undefined) continue;
    if (start > cursor) nodes.push({ type: 'text', value: value.slice(cursor, start) });

    const rawTarget = match[1];
    const separator = rawTarget.indexOf('|');
    const target = (separator === -1 ? rawTarget : rawTarget.slice(0, separator)).trim();
    const label = separator === -1 ? target : rawTarget.slice(separator + 1).trim();

    if (target && label) {
      nodes.push(createWikilink(target, label));
    } else {
      nodes.push({ type: 'text', value: match[0] });
    }
    cursor = start + match[0].length;
  }

  if (nodes.length === 0) return null;
  if (cursor < value.length) nodes.push({ type: 'text', value: value.slice(cursor) });
  return nodes;
}

function visitPhrasing(node: MarkdownNode): void {
  if (!isNodeWithChildren(node)) return;

  const children = node.children;
  for (let index = 0; index < children.length;) {
    const child = children[index];
    if (child.type === 'text' && typeof child.value === 'string') {
      const parsed = parseWikilinks(child.value);
      if (parsed) {
        children.splice(index, 1, ...parsed);
        index += parsed.length;
        continue;
      }
    }
    visitPhrasing(child);
    index += 1;
  }
}

function remarkWikilinks() {
  return (tree: Root) => {
    visitPhrasing(tree);
  };
}

const markdownComponents: Components = {
  h1: ({ children }) => <h1 className="text-xl font-semibold">{children}</h1>,
  h2: ({ children }) => <h2 className="mt-6 text-lg font-semibold">{children}</h2>,
  h3: ({ children }) => <h3 className="mt-4 text-base font-semibold">{children}</h3>,
  h4: ({ children }) => <h4 className="mt-4 text-sm font-semibold">{children}</h4>,
  p: ({ children }) => <p className="whitespace-pre-wrap">{children}</p>,
  ul: ({ children }) => <ul className="list-disc space-y-1 pl-5">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal space-y-1 pl-5">{children}</ol>,
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-stone-300 pl-3 text-stone-600">{children}</blockquote>
  ),
  pre: ({ children }) => (
    <pre className="overflow-x-auto rounded-md bg-stone-950 p-3 text-stone-100">{children}</pre>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-left">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border-b border-stone-200 px-2 py-1 font-semibold">{children}</th>
  ),
  td: ({ children }) => <td className="border-b border-stone-100 px-2 py-1">{children}</td>,
};

export function MarkdownPreview({
  markdown,
  onWikilink,
  onRawLink,
}: {
  markdown: string;
  onWikilink?: (target: string) => void;
  onRawLink?: (target: string) => void;
}) {
  const components: Components = {
    ...markdownComponents,
    a: ({ node, children, ...props }) => {
      const property = node?.properties['data-corpusbot-wikilink'];
      const target = typeof property === 'string' ? property : undefined;

      const href = typeof props.href === 'string' ? props.href : undefined;
      if (href?.startsWith('raw/')) {
        let rawTarget = href;
        try {
          rawTarget = decodeURI(href);
        } catch {
          rawTarget = href;
        }

        if (target) {
          return (
            <button
              type="button"
              data-wikilink-target={target}
              className="rounded bg-moss/10 px-1 text-moss hover:bg-moss/20"
              onClick={() => onWikilink?.(target)}
            >
              {children}
            </button>
          );
        }

        return (
          <button
            type="button"
            data-corpusbot-raw-link={rawTarget}
            className="rounded bg-stone-100 px-1 text-stone-700 hover:bg-stone-200"
            onClick={() => onRawLink?.(rawTarget)}
          >
            {children}
          </button>
        );
      }

      if (!target) {
        return (
          <a {...props} className="text-moss underline-offset-2 hover:underline">
            {children}
          </a>
        );
      }

      return (
        <button
          type="button"
          data-wikilink-target={target}
          className="rounded bg-moss/10 px-1 text-moss hover:bg-moss/20"
          onClick={() => onWikilink?.(target)}
        >
          {children}
        </button>
      );
    },
    code: ({ children }) => (
      <code className="font-mono text-[0.9em] [&:not(pre code)]:rounded [&:not(pre code)]:bg-stone-100 [&:not(pre code)]:px-1 [&:not(pre code)]:py-0.5">
        {children}
      </code>
    ),
  };

  return (
    <article className="space-y-3 text-sm leading-6">
      <Markdown remarkPlugins={[remarkWikilinks, remarkGfm]} components={components}>
        {markdown}
      </Markdown>
    </article>
  );
}
