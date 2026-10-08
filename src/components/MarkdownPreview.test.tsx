import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { MarkdownPreview } from './MarkdownPreview';

function render(markdown: string) {
  return renderToStaticMarkup(createElement(MarkdownPreview, { markdown }));
}

describe('MarkdownPreview', () => {
  it('renders an ATX heading followed directly by a paragraph', () => {
    const markup = render('## Priority\nEvidence is ordered by strength.');

    expect(markup).toContain('<h2 class="mt-6 text-lg font-semibold">Priority</h2>');
    expect(markup).toContain('<p class="whitespace-pre-wrap">Evidence is ordered by strength.</p>');
  });

  it('renders GFM tables', () => {
    const markup = render('| Tool | Role |\n| --- | --- |\n| tcpdump | evidence |');

    expect(markup).toContain('<table');
    expect(markup).toContain('<th');
  });

  it('renders wiki links as interactive buttons', () => {
    const markup = render('See [[wiki/entities/Raft.md|Raft]].');

    expect(markup).toContain('data-wikilink-target="wiki/entities/Raft.md"');
    expect(markup).toContain('<button type="button"');
    expect(markup).toContain('Raft');
  });

  it('renders captured raw links as interactive raw buttons', () => {
    const markup = render('Original file: [TCP guide.md](<raw/hash/TCP guide.md>).');

    expect(markup).toContain('data-corpusbot-raw-link="raw/hash/TCP guide.md"');
    expect(markup).toContain('<button type="button"');
    expect(markup).toContain('TCP guide.md');
  });

  it('renders raw wikilinks as interactive resource buttons', () => {
    const markup = render('Original file: [[raw/hash/TCP guide.md|TCP guide.md]].');

    expect(markup).toContain('data-wikilink-target="raw/hash/TCP guide.md"');
    expect(markup).toContain('<button type="button"');
    expect(markup).toContain('TCP guide.md');
  });

  it('does not convert wikilinks inside inline code', () => {
    const markup = render('Use `[[Raft]]` as a literal example.');

    expect(markup).not.toContain('data-corpusbot-wikilink');
    expect(markup).toContain('[[Raft]]');
  });
});
