import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const repoRoot = process.cwd();
const nodeCommand = process.execPath;
const cliPath = path.join(repoRoot, 'dist/cli.js');

ensureBuiltArtifacts();

assertHelp(['--help'], 'usage:');
assertHelp(['build', '--help'], 'indexbind build [input-dir] [output-file]');
assertHelp(['build-bundle', '--help'], 'indexbind build-bundle [input-dir] [output-dir]');
assertHelp(['update-cache', '--help'], 'indexbind update-cache [input-dir] [cache-file]');
assertHelp(['export-artifact', '--help'], 'indexbind export-artifact <output-file>');
assertHelp(['export-bundle', '--help'], 'indexbind export-bundle <output-dir>');
assertHelp(['inspect', '--help'], 'indexbind inspect <artifact-file>');
assertHelp(['benchmark', '--help'], 'indexbind benchmark <artifact-file> <queries-json>');
assertHelp(['search', '--help'], 'indexbind search <artifact-file> <query>');

const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'indexbind-cli-smoke-'));
const docsDir = path.join(tempDir, 'docs');
const defaultArtifactPath = path.join(docsDir, '.indexbind', 'index.sqlite');
const explicitArtifactPath = path.join(tempDir, 'explicit.sqlite');
const cachePath = path.join(docsDir, '.indexbind', 'build-cache.sqlite');
const exportedArtifactPath = path.join(tempDir, 'from-cache.sqlite');
fs.mkdirSync(docsDir, { recursive: true });
fs.writeFileSync(path.join(docsDir, 'rust.md'), '# Rust Guide\n\nRust retrieval guide for local search.\n');
fs.writeFileSync(path.join(docsDir, '.hidden.md'), '# Hidden\n\nShould not be indexed.\n');
fs.mkdirSync(path.join(docsDir, 'node_modules', 'pkg'), { recursive: true });
fs.writeFileSync(
  path.join(docsDir, 'node_modules', 'pkg', 'README.md'),
  '# Dependency\n\nShould not be indexed.\n',
);
fs.writeFileSync(path.join(docsDir, '.gitignore'), 'ignored.md\nnested/*\n!nested/keep.md\n');
fs.mkdirSync(path.join(docsDir, 'nested'), { recursive: true });
fs.writeFileSync(path.join(docsDir, 'ignored.md'), '# Ignored\n\nIgnored by gitignore.\n');
fs.writeFileSync(path.join(docsDir, 'nested', 'skip.md'), '# Skip\n\nIgnored by gitignore.\n');
fs.writeFileSync(path.join(docsDir, 'nested', 'keep.md'), '# Keep\n\nAllowed by negation.\n');
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

runCli(['build', docsDir, '--backend', 'hashing']);
if (!fs.existsSync(defaultArtifactPath)) {
  throw new Error(`Expected default artifact at ${defaultArtifactPath}`);
}
runCli(['build', docsDir, explicitArtifactPath, '--backend', 'hashing']);
const vectorSearchResult = JSON.parse(
  captureCli(['search', explicitArtifactPath, 'rust guide', '--mode', 'vector', '--top-k', '1']),
);
if (vectorSearchResult.options?.mode !== 'vector') {
  throw new Error(`Expected vector mode search output, got ${JSON.stringify(vectorSearchResult)}`);
}

const lexicalSearchResult = JSON.parse(
  captureCli(['search', explicitArtifactPath, 'rust guide', '--mode', 'lexical', '--top-k', '1']),
);
if (lexicalSearchResult.options?.mode !== 'lexical') {
  throw new Error(`Expected lexical mode search output, got ${JSON.stringify(lexicalSearchResult)}`);
}
if (lexicalSearchResult.hits.some((hit) => hit.relativePath !== 'rust.md')) {
  throw new Error(`Expected ignored files to stay out of the index, got ${JSON.stringify(lexicalSearchResult)}`);
}
if (lexicalSearchResult.hits[0]?.canonicalUrl !== 'https://example.com/rust') {
  throw new Error(`Expected build convention canonicalUrl, got ${JSON.stringify(lexicalSearchResult)}`);
}

const conventionSearchResult = JSON.parse(captureCli(['search', explicitArtifactPath, 'btc']));
if (
  conventionSearchResult.query !== 'bitcoin' ||
  conventionSearchResult.hitCount !== 1 ||
  conventionSearchResult.hits[0]?.relativePath !== 'rust.md'
) {
  throw new Error(`Expected search convention defaults and query rewrite, got ${JSON.stringify(conventionSearchResult)}`);
}

const overrideSearchResult = JSON.parse(
  captureCli(['search', explicitArtifactPath, 'btc', '--metadata', 'is_default_searchable=false']),
);
if (overrideSearchResult.hits[0]?.relativePath !== 'private.md') {
  throw new Error(`Expected explicit metadata flag to override default search profile, got ${JSON.stringify(overrideSearchResult)}`);
}

runCli(['update-cache', docsDir, '--backend', 'hashing']);
if (!fs.existsSync(cachePath)) {
  throw new Error(`Expected default cache at ${cachePath}`);
}
runCli(['export-artifact', exportedArtifactPath, '--cache-file', cachePath]);
if (!fs.existsSync(exportedArtifactPath)) {
  throw new Error(`Expected exported artifact at ${exportedArtifactPath}`);
}

const literalQueryResult = JSON.parse(captureCli(['search', explicitArtifactPath, '--', '--help']));
if (literalQueryResult.query !== '--help') {
  throw new Error(`Expected literal query --help, got ${JSON.stringify(literalQueryResult)}`);
}

assertFailure(
  ['search', explicitArtifactPath, 'rust guide', '--hybrid', 'true'],
  'The --hybrid flag has been removed.',
);
assertFailure(['build', docsDir, explicitArtifactPath, 'extra-arg'], 'indexbind build [input-dir] [output-file]');

if (lexicalSearchResult.query !== 'rust guide') {
  throw new Error(`Expected search query rust guide, got ${JSON.stringify(lexicalSearchResult)}`);
}

const { openIndex } = await import(pathToFileURL(path.join(repoRoot, 'dist/index.js')).href);
const index = await openIndex(explicitArtifactPath);
const conventionHits = await index.search('btc');
if (!conventionHits[0] || conventionHits[0].relativePath !== 'rust.md') {
  throw new Error(`Expected Node API search convention hit, got ${JSON.stringify(conventionHits)}`);
}
const lexicalHits = await index.search('rust guide', { mode: 'lexical' });
if (!lexicalHits[0] || lexicalHits[0].relativePath !== 'rust.md') {
  throw new Error(`Expected lexical mode API search hit, got ${JSON.stringify(lexicalHits)}`);
}
const apiHits = await index.search('rust guide', { mode: 'vector' });
if (!apiHits[0] || apiHits[0].relativePath !== 'rust.md') {
  throw new Error(`Expected vector mode API search hit, got ${JSON.stringify(apiHits)}`);
}

const lexicalProfileIndex = await openIndex(explicitArtifactPath, { modeProfile: 'lexical' });
const lexicalProfileHits = await lexicalProfileIndex.search('rust guide');
if (!lexicalProfileHits[0] || lexicalProfileHits[0].relativePath !== 'rust.md') {
  throw new Error(
    `Expected lexical profile API search hit, got ${JSON.stringify(lexicalProfileHits)}`,
  );
}
let sawLexicalProfileModeError = false;
try {
  await lexicalProfileIndex.search('rust guide', { mode: 'vector' });
} catch (error) {
  if (
    error instanceof Error &&
    error.message.includes('this index was opened with modeProfile: "lexical"')
  ) {
    sawLexicalProfileModeError = true;
  } else {
    throw error;
  }
}
if (!sawLexicalProfileModeError) {
  throw new Error('Expected lexical profile to reject vector mode');
}

let sawLegacyHybridError = false;
try {
  await index.search('rust guide', { hybrid: true });
} catch (error) {
  if (
    error instanceof Error &&
    error.message.includes('Search option "hybrid" has been removed.')
  ) {
    sawLegacyHybridError = true;
  } else {
    throw error;
  }
}
if (!sawLegacyHybridError) {
  throw new Error('Expected Node API to reject the legacy hybrid option');
}

assertFailure([], 'usage:');

console.log('CLI help smoke passed');

function ensureBuiltArtifacts() {
  if (!fs.existsSync(cliPath)) {
    throw new Error(`Missing built CLI: ${cliPath}. Run npm run build first.`);
  }
}

function assertHelp(args, expectedText) {
  const result = spawnSync(nodeCommand, [cliPath, ...args], {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    encoding: 'utf8',
  });

  if (result.status !== 0) {
    throw new Error(`Expected help to exit 0 for ${args.join(' ') || '<no-args>'}`);
  }
  const output = `${result.stdout}${result.stderr}`;
  if (!output.includes(expectedText)) {
    throw new Error(`Expected help output for ${args.join(' ')} to include ${expectedText}`);
  }
}

function runCli(args) {
  const result = spawnSync(nodeCommand, [cliPath, ...args], {
    cwd: repoRoot,
    stdio: 'inherit',
    encoding: 'utf8',
  });

  if (result.status !== 0) {
    throw new Error(`Command failed: ${args.join(' ')}`);
  }
}

function captureCli(args) {
  const result = spawnSync(nodeCommand, [cliPath, ...args], {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'inherit'],
    encoding: 'utf8',
  });

  if (result.status !== 0) {
    throw new Error(`Command failed: ${args.join(' ')}`);
  }

  return result.stdout.trim();
}


function assertFailure(args, expectedText) {
  const result = spawnSync(nodeCommand, [cliPath, ...args], {
    cwd: repoRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    encoding: 'utf8',
  });

  if (result.status === 0) {
    throw new Error(`Expected failure for ${args.join(' ') || '<no-args>'}`);
  }
  const output = `${result.stdout}${result.stderr}`;
  if (!output.includes(expectedText)) {
    throw new Error(`Expected failure output for ${args.join(' ') || '<no-args>'} to include ${expectedText}`);
  }
}
