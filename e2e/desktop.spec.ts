import { expect, test } from '@playwright/test';

const IMPORT_FILE = {
  name: 'e2e-import.md',
  mimeType: 'text/markdown',
  buffer: Buffer.from(
    '# E2E import\n\nThis page provides E2E evidence for the CorpusBot desktop workflow.',
  ),
};

test('imports, reads, questions, lints, and restores a workspace', async ({ page }) => {
  await page.goto('/?backend=browser');
  await expect(page.getByRole('heading', { name: 'CorpusBot Workspace' })).toBeVisible();

  await page.getByLabel('Workspace path').fill('/tmp/CorpusBot-browser-e2e');
  await page.getByRole('button', { name: 'Open' }).click();
  await expect(page.getByText('/tmp/CorpusBot-browser-e2e')).toBeVisible();
  await expect(page.getByText('Clean')).toBeVisible();

  await page.getByRole('button', { name: 'Vector database' }).click();
  await expect(page.getByText('wiki/entities/vector-database.md')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Vector database' })).toBeVisible();

  await page.locator('input[type="file"]').setInputFiles(IMPORT_FILE);
  await expect(page.getByRole('button', { name: 'E2E import' })).toBeVisible({ timeout: 10_000 });
  await page.getByRole('button', { name: 'E2E import' }).click();
  await expect(page.getByText('This page provides E2E evidence')).toBeVisible();

  await page.getByRole('button', { name: 'Chat', exact: true }).click();
  await page.getByPlaceholder('Ask this workspace').fill('What does the E2E evidence say?');
  await page.getByRole('button', { name: 'Ask' }).click();
  await expect(page.getByText('The imported page answers')).toBeVisible();
  await expect(page.getByText('wiki/entities/e2e-import.md')).toBeVisible();

  await page.getByRole('button', { name: 'Health' }).click();
  await expect(page.getByText('Pages: 3')).toBeVisible();
  await expect(page.getByText('Errors: 0')).toBeVisible();
  await expect(page.getByText('Warnings: 1')).toBeVisible();

  await page.getByRole('button', { name: 'History' }).click();
  const baseline = page.locator('li', { hasText: 'initial browser workspace' });
  await baseline.getByRole('button', { name: 'Restore' }).click();
  await expect(page.getByText('Restore workspace to')).toBeVisible();
  await page.getByRole('button', { name: 'Confirm restore' }).click();

  await page.getByRole('button', { name: 'Wiki', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Vector database' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'E2E import' })).toHaveCount(0);
  await expect(page.getByText('Clean')).toBeVisible();
});

test('stays on setup with a clear error when desktop IPC is unavailable', async ({ page }) => {
  await page.goto('/');
  await page.getByLabel('Workspace path').fill('/tmp/CorpusBot-unavailable');
  await page.getByRole('button', { name: 'Open' }).click();

  await expect(page.getByRole('alert')).toContainText(
    'The CorpusBot desktop backend is only available in the app.',
  );
  await expect(page.getByRole('heading', { name: 'CorpusBot Workspace' })).toBeVisible();
});

test('settings prompt for optional provider values instead of showing defaults', async ({
  page,
}) => {
  await page.goto('/?backend=browser');
  await page.getByLabel('Workspace path').fill('/tmp/CorpusBot-browser-e2e');
  await page.getByRole('button', { name: 'Open' }).click();
  await page.getByRole('button', { name: 'Settings' }).click();

  const baseUrl = page.getByLabel('Base URL');
  const model = page.getByLabel('Model');
  await expect(baseUrl).toHaveValue('');
  await expect(model).toHaveValue('');
  await expect(baseUrl).toHaveAttribute('placeholder', 'https://api.openai.com/v1');
  await expect(model).toHaveAttribute('placeholder', 'gpt-4o-mini');
  await expect(
    page.getByText('Leave Base URL and Model blank to use the built-in defaults.'),
  ).toBeVisible();
});
