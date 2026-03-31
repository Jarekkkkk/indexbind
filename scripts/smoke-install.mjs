import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const rootPackageDir = process.env.ROOT_PACKAGE_DIR;
const nativePackageDir = process.env.NATIVE_PACKAGE_DIR;
const artifactPath = process.env.ARTIFACT_PATH;
const expectedTopHit = process.env.EXPECTED_TOP_HIT ?? 'guides/rust.md';

if (!rootPackageDir || !nativePackageDir || !artifactPath) {
  throw new Error('ROOT_PACKAGE_DIR, NATIVE_PACKAGE_DIR, and ARTIFACT_PATH are required');
}

const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const nodeCommand = process.execPath;
const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'indexbind-smoke-'));
const packDir = path.join(tempDir, 'packs');

fs.mkdirSync(packDir, { recursive: true });

const rootTarball = pack(rootPackageDir, packDir);
const nativeTarball = pack(nativePackageDir, packDir);

run(npmCommand, ['init', '-y'], tempDir);
run(npmCommand, ['install', rootTarball, nativeTarball], tempDir);

const docsDir = path.join(tempDir, 'docs');
fs.mkdirSync(docsDir, { recursive: true });
fs.writeFileSync(
  path.join(docsDir, 'rust.md'),
  '# Rust Guide\n\nRust retrieval guide for local bitcoin search.\n',
);
fs.writeFileSync(path.join(docsDir, '.hidden.md'), '# Hidden\n\nShould not be indexed.\n');
fs.writeFileSync(
  path.join(docsDir, 'private.md'),
  '# Private Note\n\nBitcoin planning note that should stay out of default search.\n',
);
fs.writeFileSync(
  path.join(docsDir, 'skip.md'),
  '# Skip Me\n\nThis file should be excluded by the build convention.\n',
);
fs.writeFileSync(
  path.join(docsDir, 'indexbind.build.js'),
  `module.exports = {
  includeDocument(relativePath) {
    return relativePath !== 'skip.md';
  },
  transformDocument(document) {
    const isDefault = document.relativePath === 'rust.md';
    return {
      ...document,
      canonicalUrl: 'https://example.com/' + document.relativePath.replace(/\\.md$/i, ''),
      metadata: {
        ...(document.metadata || {}),
        is_default_searchable: String(isDefault),
        directory_weight: isDefault ? 2 : 0.1,
      },
    };
  },
};
`,
);
fs.writeFileSync(
  path.join(docsDir, 'indexbind.search.js'),
  `module.exports = {
  profiles: {
    default: {
      metadata: { is_default_searchable: 'true' },
      scoreAdjustment: { metadataNumericMultiplier: 'directory_weight' },
    },
  },
  transformQuery(query) {
    return { query: query.replace(/btc/ig, 'bitcoin') };
  },
};
`,
);

const cliArtifactPath = path.join(docsDir, '.indexbind', 'index.sqlite');
const buildOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'build', docsDir, '--backend', 'hashing'],
  tempDir,
);
const buildStats = JSON.parse(buildOutput);
if (buildStats.documentCount !== 2 || buildStats.chunkCount < 2) {
  throw new Error(`Unexpected build stats: ${buildOutput}`);
}

const inspectOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'inspect', cliArtifactPath],
  tempDir,
);
const inspectInfo = JSON.parse(inspectOutput);
if (inspectInfo.documentCount !== 2) {
  throw new Error(`Unexpected inspect output: ${inspectOutput}`);
}

const vectorSearchOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'search', cliArtifactPath, 'rust guide', '--mode', 'vector', '--top-k', '3'],
  tempDir,
);
const vectorSearchResult = JSON.parse(vectorSearchOutput);
if (
  vectorSearchResult.options?.mode !== 'vector' ||
  vectorSearchResult.hitCount !== 1 ||
  vectorSearchResult.hits[0]?.relativePath !== 'rust.md'
) {
  throw new Error(`Unexpected vector CLI search output: ${vectorSearchOutput}`);
}

const lexicalSearchOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'search', cliArtifactPath, 'rust guide', '--mode', 'lexical', '--top-k', '3'],
  tempDir,
);
const lexicalSearchResult = JSON.parse(lexicalSearchOutput);
if (
  lexicalSearchResult.options?.mode !== 'lexical' ||
  lexicalSearchResult.hitCount !== 1 ||
  lexicalSearchResult.hits[0]?.relativePath !== 'rust.md'
) {
  throw new Error(`Unexpected lexical CLI search output: ${lexicalSearchOutput}`);
}

const conventionSearchOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'search', cliArtifactPath, 'btc'],
  tempDir,
);
const conventionSearchResult = JSON.parse(conventionSearchOutput);
if (
  conventionSearchResult.query !== 'bitcoin' ||
  conventionSearchResult.hitCount !== 1 ||
  conventionSearchResult.hits[0]?.relativePath !== 'rust.md'
) {
  throw new Error(`Unexpected convention CLI search output: ${conventionSearchOutput}`);
}

const overrideSearchOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'search', cliArtifactPath, 'btc', '--metadata', 'is_default_searchable=false'],
  tempDir,
);
const overrideSearchResult = JSON.parse(overrideSearchOutput);
if (overrideSearchResult.hits[0]?.relativePath !== 'private.md') {
  throw new Error(`Unexpected explicit override CLI search output: ${overrideSearchOutput}`);
}

const helpResult = spawnSync(
  npmCommand,
  ['exec', '--', 'indexbind', 'search', '--help'],
  {
    cwd: tempDir,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: {
      ...process.env,
      NPM_CONFIG_LOGLEVEL: 'silent',
    },
    encoding: 'utf8',
  },
);

if (helpResult.status !== 0 || !`${helpResult.stdout}${helpResult.stderr}`.includes('indexbind search <artifact-file> <query>')) {
  throw new Error(`Unexpected CLI help output: ${helpResult.stdout}${helpResult.stderr}`);
}

const cachePath = path.join(docsDir, '.indexbind', 'build-cache.sqlite');
const updateCacheOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'update-cache', docsDir, '--backend', 'hashing'],
  tempDir,
);
const updateCacheStats = JSON.parse(updateCacheOutput);
if (updateCacheStats.activeDocumentCount !== 2) {
  throw new Error(`Unexpected update-cache output: ${updateCacheOutput}`);
}

const exportedArtifactPath = path.join(tempDir, 'from-cache.sqlite');
const exportOutput = capture(
  npmCommand,
  ['exec', '--', 'indexbind', 'export-artifact', exportedArtifactPath, '--cache-file', cachePath],
  tempDir,
);
const exportStats = JSON.parse(exportOutput);
if (exportStats.documentCount !== 2 || !fs.existsSync(exportedArtifactPath)) {
  throw new Error(`Unexpected export-artifact output: ${exportOutput}`);
}

const verifyScript = path.join(tempDir, 'verify.mjs');
fs.writeFileSync(
  verifyScript,
  `
import { openIndex } from 'indexbind';

const index = await openIndex(${JSON.stringify(artifactPath)});
const hits = await index.search('rust guide', {
  reranker: { candidatePoolSize: 25 },
});

if (!hits[0]) {
  throw new Error('No hits returned from smoke test query');
}

if (hits[0].relativePath !== ${JSON.stringify(expectedTopHit)}) {
  throw new Error(\`Expected top hit ${expectedTopHit}, received \${hits[0].relativePath}\`);
}

console.log(JSON.stringify({
  topHit: hits[0].relativePath,
  score: hits[0].score,
}, null, 2));
`,
);

run(nodeCommand, [verifyScript], tempDir);

function pack(packageDir, destination) {
  const result = spawnSync(
    npmCommand,
    ['pack', '.', '--pack-destination', destination],
    {
      cwd: packageDir,
      stdio: ['ignore', 'pipe', 'inherit'],
      env: process.env,
      encoding: 'utf8',
    },
  );

  if (result.status !== 0) {
    throw new Error(`Failed to pack ${packageDir}`);
  }

  const tarball = result.stdout
    .trim()
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .at(-1);

  if (!tarball) {
    throw new Error(`Could not determine tarball name for ${packageDir}`);
  }

  return path.join(destination, tarball);
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

function capture(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: ['ignore', 'pipe', 'inherit'],
    env: {
      ...process.env,
      ...(command === npmCommand ? { NPM_CONFIG_LOGLEVEL: 'silent' } : {}),
    },
    encoding: 'utf8',
  });

  if (result.status !== 0) {
    throw new Error(`Command failed: ${command} ${args.join(' ')}`);
  }

  return result.stdout.trim();
}
