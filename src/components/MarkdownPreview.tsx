import React from 'react';

function renderInline(text: string, onWikilink?: (target: string) => void) {
  const pieces = text.split(/(\[\[[^\]]+\]\]|\*\*[^*]+\*\*|`[^`]+`)/g).filter(Boolean);
  return pieces.map((piece, index) => {
    const key = `${index}-${piece}`;
    if (piece.startsWith('[[') && piece.endsWith(']]')) {
      const target = piece.slice(2, -2).split('|')[0];
      return (
        <button
          key={key}
          type="button"
          className="rounded bg-moss/10 px-1 text-moss hover:bg-moss/20"
          onClick={() => onWikilink?.(target)}
        >
          {piece.slice(2, -2).split('|')[1] ?? target}
        </button>
      );
    }
    if (piece.startsWith('**') && piece.endsWith('**')) {
      return <strong key={key}>{piece.slice(2, -2)}</strong>;
    }
    if (piece.startsWith('`') && piece.endsWith('`')) {
      return (
        <code key={key} className="rounded bg-stone-100 px-1 py-0.5 text-[0.9em]">
          {piece.slice(1, -1)}
        </code>
      );
    }
    return <React.Fragment key={key}>{piece}</React.Fragment>;
  });
}

export function MarkdownPreview({
  markdown,
  onWikilink,
}: {
  markdown: string;
  onWikilink?: (target: string) => void;
}) {
  const blocks = markdown.trim().split(/\n{2,}/);

  return (
    <article className="space-y-3 text-sm leading-6">
      {blocks.map((block, blockIndex) => {
        const lines = block.split('\n');
        if (block.startsWith('```')) {
          return (
            <pre
              key={blockIndex}
              className="overflow-x-auto rounded-md bg-stone-950 p-3 text-stone-100"
            >
              <code>{block.replace(/^```.*\n?/, '').replace(/```$/, '')}</code>
            </pre>
          );
        }
        if (lines.every((line) => line.trim().startsWith('- '))) {
          return (
            <ul key={blockIndex} className="list-disc space-y-1 pl-5">
              {lines.map((line, lineIndex) => (
                <li key={lineIndex}>{renderInline(line.trim().slice(2), onWikilink)}</li>
              ))}
            </ul>
          );
        }
        const heading = block.match(/^(#{1,4})\s+(.+)$/);
        if (heading) {
          const level = heading[1].length;
          const className =
            level === 1
              ? 'text-xl font-semibold'
              : level === 2
                ? 'text-lg font-semibold'
                : 'text-base font-semibold';
          return (
            <h3 key={blockIndex} className={className}>
              {renderInline(heading[2], onWikilink)}
            </h3>
          );
        }
        return <p key={blockIndex}>{renderInline(block, onWikilink)}</p>;
      })}
    </article>
  );
}
