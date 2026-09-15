import type {
  IngestResult,
  LintReport,
  QueryAnswer,
  SettingsSummary,
  SnapshotResult,
  SnapshotRow,
  TemplateId,
  WikiPage,
  WikiPageSummary,
  WorkspaceStatus,
  WorkspaceSummary,
} from '../types';

type CommandArgs = Record<string, unknown>;

type BrowserPage = WikiPage;

const ROOT = '/tmp/CorpusBot-browser-e2e';
const BASELINE_ID = 'browser-baseline';

function page(input: Omit<BrowserPage, 'sha256'>): BrowserPage {
  return { ...input, sha256: input.path };
}

const indexPage = page({
  path: 'wiki/index.md',
  title: 'Index',
  pageType: 'index',
  updatedAt: '2026-09-15',
  related: [],
  sources: [],
  markdown: '# Index\n\nA generated catalog of Wiki pages.',
});

const vectorPage = page({
  path: 'wiki/entities/vector-database.md',
  title: 'Vector database',
  pageType: 'entity',
  updatedAt: '2026-09-15',
  related: [],
  sources: [],
  markdown:
    '# Vector database\n\nA vector database stores embeddings and answers similarity queries.\n',
});

type BrowserState = {
  pages: Map<string, BrowserPage>;
  history: SnapshotRow[];
};

const state: BrowserState = {
  pages: new Map([
    [indexPage.path, indexPage],
    [vectorPage.path, vectorPage],
  ]),
  history: [
    {
      snapshotId: BASELINE_ID,
      message: 'initial browser workspace',
      createdAt: Date.parse('2026-09-15T09:00:00Z'),
    },
  ],
};

function snapshot(message: string): SnapshotRow {
  return {
    snapshotId: `browser-${state.history.length + 1}-${message.replaceAll(' ', '-')}`,
    message,
    createdAt: Date.now(),
  };
}

function status(): WorkspaceStatus {
  return {
    root: ROOT,
    template: 'research',
    headSnapshotId: state.history[0]?.snapshotId,
    dirtyPaths: [],
    unsafeState: null,
    recoveryPending: false,
    pageCount: state.pages.size,
  };
}

function summary(): WorkspaceSummary {
  return {
    root: ROOT,
    template: 'research',
    headSnapshotId: state.history[0]?.snapshotId,
  };
}

function pages(): WikiPageSummary[] {
  return [...state.pages.values()]
    .map(({ markdown: _markdown, ...summary }) => summary)
    .sort((left, right) => left.path.localeCompare(right.path));
}

function readPage(path: string): BrowserPage {
  const selected = state.pages.get(path);
  if (!selected) throw new Error(`Wiki page not found: ${path}`);
  return selected;
}

function ingest(fileName: string, markdown: string): IngestResult {
  const slug = fileName
    .toLowerCase()
    .replaceAll('.md', '')
    .replaceAll(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');
  const path = `wiki/entities/${slug || 'imported-page'}.md`;
  const imported = page({
    path,
    title: 'E2E import',
    pageType: 'entity',
    updatedAt: '2026-09-15',
    related: [],
    sources: [fileName],
    markdown,
  });
  state.pages.set(path, imported);
  state.history.unshift(snapshot(`ingest ${fileName}`));

  return {
    status: 'committed',
    runId: 'browser-ingest-run',
    sourceId: 'browser-source',
    sourceVersionId: 'browser-source-version',
    sourcePage: `wiki/sources/browser-source-version.md`,
    createdPaths: [path],
    updatedPaths: [],
    snapshotId: state.history[0].snapshotId,
    manifestId: 'browser-manifest',
  };
}

function query(question: string): QueryAnswer {
  const imported = [...state.pages.values()].find((candidate) =>
    candidate.path.startsWith('wiki/entities/e2e-import.'),
  );
  if (!imported) throw new Error('browser-e2e import must run before query');
  return {
    answer: `The imported page answers: ${question}`,
    citations: [
      {
        number: 1,
        path: imported.path,
        title: imported.title,
        quote: 'E2E evidence',
        resourceRevision: { kind: 'content', value: { sha256: 'e2e-evidence' } },
      },
    ],
    revisionManifestId: 'browser-manifest',
    warnings: [],
    insufficientEvidence: false,
  };
}

function lint(): LintReport {
  return {
    generatedAt: '2026-09-15T09:00:00Z',
    template: 'research',
    revisionManifestId: 'browser-manifest',
    summary: {
      pages: state.pages.size,
      errors: 0,
      warnings: 1,
    },
    issues: [
      {
        code: 'ORPHAN_PAGE',
        severity: 'warning',
        path: 'wiki/entities/e2e-import.md',
        message: 'The imported page has no inbound wikilink.',
        fixHint: 'Link it from an overview page.',
      },
    ],
  };
}

function restore(snapshotId: string): { snapshotId: string; restored: boolean } {
  if (snapshotId !== BASELINE_ID) throw new Error(`Unknown browser snapshot: ${snapshotId}`);
  state.history.unshift(snapshot(`pre-restore ${snapshotId}`));
  state.pages = new Map([
    [indexPage.path, indexPage],
    [vectorPage.path, vectorPage],
  ]);
  state.history.unshift(snapshot(`restore ${snapshotId}`));
  return { snapshotId, restored: true };
}

function createSnapshot(message: string): SnapshotResult {
  const row = snapshot(message);
  state.history.unshift(row);
  return {
    result: 'created',
    snapshotId: row.snapshotId,
    manifestId: 'browser-manifest',
    workspaceChangedAfterCapture: false,
  };
}

export async function invokeBrowserCommand<T>(command: string, args: CommandArgs = {}): Promise<T> {
  switch (command) {
    case 'init_workspace': {
      const template = args.template as TemplateId;
      if (template !== 'research' && template !== 'generic') {
        throw new Error(`unknown template: ${template}`);
      }
      return summary() as T;
    }
    case 'open_workspace':
    case 'workspace_status':
      return status() as T;
    case 'list_wiki_pages':
      return pages() as T;
    case 'read_wiki_page':
      return readPage(args.path as string) as T;
    case 'ingest_content':
      return ingest(args.fileName as string, args.markdown as string) as T;
    case 'query':
      return query(args.question as string) as T;
    case 'run_lint':
      return lint() as T;
    case 'create_snapshot':
      return createSnapshot(args.message as string) as T;
    case 'list_snapshots':
      return state.history as T;
    case 'restore_snapshot':
      return restore(args.snapshotId as string) as T;
    case 'get_settings': {
      const value: SettingsSummary = {
        baseUrl: 'https://browser-e2e.invalid/v1',
        model: 'browser-e2e-model',
        hasApiKey: false,
        baseUrlSource: 'settings',
        modelSource: 'settings',
        apiKeySource: null,
        environmentOverrides: [],
        gitAuthorName: 'CorpusBot E2E',
        gitAuthorEmail: 'e2e@corpusbot.invalid',
      };
      return value as T;
    }
    case 'save_settings':
      return args.settings as SettingsSummary as T;
    default:
      throw new Error(`browser backend does not support command: ${command}`);
  }
}
