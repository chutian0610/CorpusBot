import { spawn } from 'node:child_process';

const frontendPort = process.env.E2E_FRONTEND_PORT ?? '1420';

await new Promise((resolve, reject) => {
  const build = spawn('cargo', ['build', '--bin', 'corpusbot-server'], {
    stdio: 'inherit',
  });
  build.on('exit', (code) =>
    code === 0 ? resolve() : reject(new Error(`cargo build exited ${code}`)),
  );
});

const backend = spawn(`${process.cwd()}/target/debug/corpusbot-server`, {
  env: {
    ...process.env,
    CORPUSBOT_E2E_LLM: '1',
    CORPUSBOT_LOCAL_BIND: '127.0.0.1:1422',
  },
  stdio: 'inherit',
});

for (let attempt = 0; attempt < 120; attempt += 1) {
  try {
    const response = await fetch('http://127.0.0.1:1422/api/health');
    if (response.ok) break;
  } catch {
    // The backend is still starting.
  }
  await new Promise((resolve) => setTimeout(resolve, 250));
}

const frontend = spawn(
  process.execPath,
  [
    `${process.cwd()}/node_modules/vite/bin/vite.js`,
    '--mode',
    'local-backend',
    '--host',
    '127.0.0.1',
    '--port',
    frontendPort,
    '--strictPort',
  ],
  {
    env: {
      ...process.env,
      CORPUSBOT_API_PROXY_TARGET: 'http://127.0.0.1:1422',
    },
    stdio: 'inherit',
  },
);

const stop = () => {
  frontend.kill('SIGTERM');
  backend.kill('SIGTERM');
};

process.on('SIGINT', () => {
  stop();
  process.exit(130);
});
process.on('SIGTERM', () => {
  stop();
  process.exit(143);
});
frontend.on('exit', () => {
  stop();
});
backend.on('exit', () => {
  frontend.kill('SIGTERM');
});
