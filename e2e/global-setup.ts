import { rmSync } from 'node:fs';

const LOCAL_TEST_WORKSPACES = [
  '/tmp/CorpusBot-local-e2e',
  '/tmp/CorpusBot-local-remembered',
  '/tmp/CorpusBot-local-created',
];

export default function globalSetup() {
  for (const path of LOCAL_TEST_WORKSPACES) {
    rmSync(path, { recursive: true, force: true });
  }
}
