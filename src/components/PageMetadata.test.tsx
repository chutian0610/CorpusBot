import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { WikiPage } from '../types';
import { PageMetadata } from './PageMetadata';

const page: WikiPage = {
  path: 'wiki/sources/ver_abc123.md',
  title: 'TCP 排障入门指南 (abc123)',
  pageType: 'source',
  sha256: 'a'.repeat(64),
  createdAt: '2026-09-24',
  updatedAt: '2026-09-24',
  tags: [],
  aliases: [],
  related: [],
  sources: [],
  sourceReferences: [
    {
      sourceVersionId: 'ver_abc123',
      title: 'TCP 排障入门指南',
    },
  ],
  raw: {
    path: 'raw/hash/TCP 排障入门指南.md',
    originalName: 'TCP 排障入门指南.md',
    sha256: 'b'.repeat(64),
    size: 9398,
  },
  markdown: '',
  body: '',
};

function render() {
  return renderToStaticMarkup(createElement(PageMetadata, { page }));
}

describe('PageMetadata', () => {
  it('omits empty optional metadata instead of showing placeholders', () => {
    const markup = render();

    expect(markup).not.toContain('>None<');
    expect(markup).not.toContain('Aliases');
    expect(markup).not.toContain('Related');
  });

  it('nests captured raw details under the source version', () => {
    const markup = render();

    expect(markup).toContain('Source provenance');
    expect(markup).toContain('Captured raw');
    expect(markup).toContain('TCP 排障入门指南.md');
    expect(markup).toContain(`sha256:${'b'.repeat(64)}`);
    expect(markup).toContain('9.2 KB');
  });
});
