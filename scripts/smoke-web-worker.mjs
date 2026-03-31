import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { Worker } from 'node:worker_threads';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const repoRoot = process.cwd();
const nodeCommand = process.execPath;
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'indexbind-web-worker-'));
const fixtureDocs = path.join(repoRoot, 'fixtures/benchmark/basic/docs');
const expectedTopHit = 'guides/rust.md';

ensureBuiltArtifacts();

const cases = [
  {
    name: 'hashing',
    backendArg: 'hashing',
    bundleDir: path.join(tempDir, 'hashing.bundle'),
  },
  {
    name: 'model2vec',
    backendArg: 'minishlab/potion-base-2M',
    bundleDir: path.join(tempDir, 'model2vec.bundle'),
  },
];

for (const testCase of cases) {
  run(
    nodeCommand,
    [
      path.join(repoRoot, 'dist/cli.js'),
      'build-bundle',
      fixtureDocs,
      testCase.bundleDir,
      '--backend',
      testCase.backendArg,
    ],
    repoRoot,
  );
}

const server = createStaticServer(tempDir);
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

try {
  const address = server.address();
  if (!address || typeof address === 'string') {
    throw new Error('Failed to resolve worker smoke server address');
  }
  const baseUrl = `http://127.0.0.1:${address.port}`;

  for (const testCase of cases) {
    const workerResult = await runWorkerSmoke(
      pathToFileURL(path.join(repoRoot, 'dist/web.js')).href,
      `${baseUrl}/${path.basename(testCase.bundleDir)}/`,
    );

    if (!workerResult.topHit) {
      throw new Error(`[${testCase.name}] expected at least one hit`);
    }
    if (workerResult.topHit !== expectedTopHit) {
      throw new Error(
        `[${testCase.name}] expected top hit ${expectedTopHit}, got ${workerResult.topHit}`,
      );
    }

    console.log(
      JSON.stringify(
        {
          case: testCase.name,
          runtime: 'worker',
          topHit: workerResult.topHit,
          score: workerResult.score,
        },
        null,
        2,
      ),
    );
  }
} finally {
  await new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

function ensureBuiltArtifacts() {
  const requiredFiles = [
    path.join(repoRoot, 'dist/cli.js'),
    path.join(repoRoot, 'dist/web.js'),
    path.join(repoRoot, 'dist/wasm/indexbind_wasm.js'),
    path.join(repoRoot, 'dist/wasm/indexbind_wasm_bg.wasm'),
  ];

  for (const file of requiredFiles) {
    if (!fs.existsSync(file)) {
      throw new Error(`Missing built artifact: ${file}. Run npm run build first.`);
    }
  }
}

function createStaticServer(rootDir) {
  return http.createServer((request, response) => {
    const requestPath = new URL(request.url, 'http://127.0.0.1').pathname;
    const filePath = path.join(rootDir, requestPath);
    const normalized = path.normalize(filePath);
    if (!normalized.startsWith(rootDir)) {
      response.writeHead(403).end('forbidden');
      return;
    }

    let targetPath = normalized;
    if (fs.existsSync(targetPath) && fs.statSync(targetPath).isDirectory()) {
      response.writeHead(403).end('directory listing disabled');
      return;
    }
    if (!fs.existsSync(targetPath)) {
      response.writeHead(404).end('not found');
      return;
    }

    response.writeHead(200, {
      'content-type': contentTypeFor(targetPath),
      'cache-control': 'no-store',
    });
    fs.createReadStream(targetPath).pipe(response);
  });
}

function contentTypeFor(filePath) {
  if (filePath.endsWith('.json')) return 'application/json';
  if (filePath.endsWith('.wasm')) return 'application/wasm';
  if (filePath.endsWith('.bin') || filePath.endsWith('.safetensors')) return 'application/octet-stream';
  return 'text/plain; charset=utf-8';
}

function runWorkerSmoke(webModuleUrl, bundleUrl) {
  return new Promise((resolve, reject) => {
    const worker = new Worker(
      `
        import { parentPort, workerData } from 'node:worker_threads';

        const { openWebIndex } = await import(workerData.webModuleUrl);

        try {
          const index = await openWebIndex(workerData.bundleUrl);
          const hits = await index.search('rust guide');
          parentPort.postMessage({
            ok: true,
            topHit: hits[0]?.relativePath,
            score: hits[0]?.score,
          });
        } catch (error) {
          parentPort.postMessage({
            ok: false,
            error: error instanceof Error ? error.stack ?? error.message : String(error),
          });
        }
      `,
      {
        eval: true,
        type: 'module',
        workerData: { webModuleUrl, bundleUrl },
      },
    );

    worker.once('message', (message) => {
      worker.terminate().catch(() => {});
      if (!message?.ok) {
        reject(new Error(message?.error ?? 'worker smoke failed'));
        return;
      }
      resolve(message);
    });
    worker.once('error', reject);
    worker.once('exit', (code) => {
      if (code !== 0) {
        reject(new Error(`worker exited with code ${code}`));
      }
    });
  });
}

function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit',
    env: process.env,
  });

  if (result.status !== 0) {
    throw new Error(`Command failed: ${command} ${args.join(' ')}`);
  }
}
