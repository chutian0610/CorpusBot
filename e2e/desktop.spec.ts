import { expect, test, type Page } from '@playwright/test';

const IMPORT_FILE = {
  name: 'e2e-import.md',
  mimeType: 'text/markdown',
  buffer: Buffer.from(
    '# E2E import\n\nThis page provides E2E evidence for the CorpusBot desktop workflow.',
  ),
};

async function openWorkspace(page: Page, root: string) {
  await page.getByRole('button', { name: 'Open workspace' }).click();
  await page.getByLabel('Workspace path').fill(root);
  await page.getByRole('button', { name: 'Open', exact: true }).click();
}

async function initializeWorkspace(root: string) {
  const response = await fetch('http://127.0.0.1:1422/api/invoke', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      command: 'init_workspace',
      args: { root, template: 'research' },
    }),
  });
  if (!response.ok) {
    const reopen = await fetch('http://127.0.0.1:1422/api/invoke', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ command: 'open_workspace', args: { root } }),
    });
    if (!reopen.ok) {
      throw new Error(`Failed to initialize ${root}: ${response.status}`);
    }
  }
}

test('imports, reads, questions, lints, and restores a workspace', async ({ page }) => {
  const root = '/tmp/CorpusBot-local-e2e';
  await initializeWorkspace(root);
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Choose a workspace' })).toBeVisible();

  await openWorkspace(page, root);
  await expect(page.getByText(root)).toBeVisible();
  await expect(page.getByText('Clean')).toBeVisible();

  await page.getByRole('button', { name: 'Ingest' }).click();
  await expect(page.getByRole('heading', { name: 'Ingest', exact: true })).toBeVisible();
  await page.locator('input[type="file"]').setInputFiles(IMPORT_FILE);
  await expect(page.getByText('e2e-import.md', { exact: true })).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('This page provides E2E evidence').first()).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByText('Workflow events')).toBeVisible();
  await expect(page.getByText('analyze', { exact: true }).first()).toBeVisible();

  await page.getByRole('button', { name: 'Chat', exact: true }).click();
  await page.getByPlaceholder('Ask this workspace').fill('What does the E2E evidence say?');
  await page.getByRole('button', { name: 'Ask' }).click();
  await expect(page.getByText('The imported page answers')).toBeVisible();
  await expect(page.getByText('wiki/entities/e2e-import.md')).toBeVisible();

  await page.getByRole('button', { name: 'Documents' }).click();
  await expect(page.getByRole('button', { name: /ver_\w+\.md/ })).toBeVisible();
  await page.locator('[data-testid^="file-tree:raw/"][data-testid$="/e2e-import.md"]').click();
  await expect(page.getByRole('heading', { name: 'Raw source' })).toBeVisible();
  await expect(page.getByText('This page provides E2E evidence')).toBeVisible();
  await page
    .getByRole('region', { name: 'Extracted structure' })
    .getByRole('button', { name: /E2E import/ })
    .click();
  await expect(page.getByRole('heading', { name: 'E2E import' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Metadata' })).toBeVisible();
  await expect(page.getByText('Type')).toBeVisible();

  await page.getByRole('button', { name: 'Health' }).click();
  await expect(page.getByText('Content pages: 3')).toBeVisible();
  await expect(page.getByText('Errors: 0')).toBeVisible();
  await expect(page.getByText('Warnings:')).toBeVisible();

  await page.getByRole('button', { name: 'History' }).click();
  const baseline = page.locator('li', { hasText: 'init research' }).first();
  await baseline.getByRole('button', { name: 'Restore' }).click();
  await expect(page.getByText('Restore workspace to')).toBeVisible();
  await page.getByRole('button', { name: 'Confirm restore' }).click();

  await page.getByRole('button', { name: 'Wiki', exact: true }).click();
  await expect(page.getByText('No pages yet')).toBeVisible();
  await expect(page.getByRole('button', { name: 'E2E import' })).toHaveCount(0);
  await expect(page.getByText('Clean')).toBeVisible();
});

test('stays in the launcher when the workspace is unavailable', async ({ page }) => {
  await page.goto('/');
  await openWorkspace(page, '/tmp/CorpusBot-unavailable');

  await expect(page.getByRole('alert')).toContainText('No such file or directory');
  await expect(page.getByRole('heading', { name: 'Choose a workspace' })).toBeVisible();
});

test('settings prompt for optional provider values instead of showing defaults', async ({
  page,
}) => {
  const root = '/tmp/CorpusBot-local-e2e';
  await initializeWorkspace(root);
  await page.goto('/');
  await openWorkspace(page, root);
  await page.getByRole('button', { name: 'Settings' }).click();

  const baseUrl = page.getByLabel('Base URL');
  const model = page.getByLabel('Model', { exact: true });
  await expect(baseUrl).toHaveValue('');
  await expect(model).toHaveValue('');
  await expect(baseUrl).toHaveAttribute('placeholder', 'https://api.openai.com/v1');
  await expect(model).toHaveAttribute('placeholder', 'gpt-4o-mini');
  await expect(
    page.getByText('Leave Base URL and Model blank to use the built-in defaults.'),
  ).toBeVisible();

  await page.getByRole('button', { name: 'Test connection' }).click();
  await expect(page.getByRole('alert')).toContainText('API key is empty');

  await page.getByLabel('API key').fill('browser-test-key');
  await page.getByRole('button', { name: 'Test connection' }).click();
  await expect(page.getByText(/Connected to fake-model/)).toBeVisible();
});

test('lets the user choose between recent and new workspaces', async ({ page }) => {
  const root = '/tmp/CorpusBot-local-remembered';
  await initializeWorkspace(root);
  await page.goto('/');
  await openWorkspace(page, root);
  await expect(page.getByText(root).first()).toBeVisible();
  await expect(page.getByText('Clean')).toBeVisible();

  await page.getByRole('button', { name: 'Change workspace' }).click();
  await expect(page.getByRole('heading', { name: 'Choose a workspace' })).toBeVisible();
  await expect(page.getByText(root).first()).toBeVisible();
  await expect(page.getByRole('button', { name: 'New workspace' })).toBeVisible();

  await page.getByText(root).first().click();
  await expect(page.getByText(root).first()).toBeVisible();
  await expect(page.getByText('Clean')).toBeVisible();
});

test('filters remembered workspaces by path', async ({ page }) => {
  const workspaces = [
    '/tmp/CorpusBot-search-last',
    '/tmp/CorpusBot-search-alpha',
    '/tmp/CorpusBot-search-beta',
  ];
  await page.addInitScript((paths) => {
    localStorage.setItem('corpusbot.root', paths[0]);
    localStorage.setItem('corpusbot.recentWorkspaces', JSON.stringify(paths));
  }, workspaces);

  await page.goto('/');
  await expect(page.getByText(workspaces[0])).toBeVisible();
  await expect(page.getByText(workspaces[1])).toBeVisible();
  await expect(page.getByText(workspaces[2])).toBeVisible();

  await page.getByLabel('Search workspaces').fill('alpha');
  await expect(page.getByText(workspaces[0])).toHaveCount(0);
  await expect(page.getByText(workspaces[1])).toBeVisible();
  await expect(page.getByText(workspaces[2])).toHaveCount(0);

  await page.getByLabel('Search workspaces').fill('missing-workspace');
  await expect(page.getByText('No workspaces match this search.')).toBeVisible();
});

test('creates a workspace from the action dialog', async ({ page }) => {
  const root = '/tmp/CorpusBot-local-created';
  await page.goto('/');

  await page.getByRole('button', { name: 'New workspace' }).click();
  await expect(page.getByRole('heading', { name: 'Create workspace' })).toBeVisible();
  await page.getByLabel('Workspace path').fill(root);
  await page.getByText('generic', { exact: true }).click();
  await page.getByRole('button', { name: 'Create', exact: true }).click();

  await expect(page.getByText(root).first()).toBeVisible();
  await expect(page.getByText('Clean')).toBeVisible();
});

test('removes workspaces from the recent list', async ({ page }) => {
  const workspaces = [
    '/tmp/CorpusBot-remove-last',
    '/tmp/CorpusBot-remove-alpha',
    '/tmp/CorpusBot-remove-beta',
  ];
  await page.addInitScript((paths) => {
    localStorage.setItem('corpusbot.root', paths[0]);
    localStorage.setItem('corpusbot.recentWorkspaces', JSON.stringify(paths));
  }, workspaces);

  await page.goto('/');
  await page.getByLabel(`Remove ${workspaces[1]} from recent workspaces`).click();
  await expect(page.getByText(workspaces[1])).toHaveCount(0);
  await expect(page.getByText(workspaces[0])).toBeVisible();
  await expect(page.getByText(workspaces[2])).toBeVisible();

  await page.getByLabel(`Remove ${workspaces[0]} from recent workspaces`).click();
  await expect(page.getByText(workspaces[0])).toHaveCount(0);
  await expect(page.getByText(workspaces[2])).toBeVisible();

  await page.getByLabel(`Remove ${workspaces[2]} from recent workspaces`).click();
  const emptyState = page.getByTestId('recent-workspaces-empty');
  await expect(emptyState).toHaveCount(1);
  await expect(emptyState).toBeVisible();
  await expect(
    emptyState.getByText('No workspaces yet. Create your first workspace to get started.'),
  ).toBeVisible();
});
