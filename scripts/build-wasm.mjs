import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';

const root = process.cwd();
const wasmTarget = 'wasm32-unknown-unknown';
const wasmWebOutDir = resolve(root, 'dist', 'wasm');
const wasmBundlerOutDir = resolve(root, 'dist', 'wasm-bundler');
const wasmArtifact = resolve(
  root,
  'target',
  wasmTarget,
  'release',
  'indexbind_wasm.wasm',
);

mkdirSync(dirname(wasmArtifact), { recursive: true });
mkdirSync(wasmWebOutDir, { recursive: true });
mkdirSync(wasmBundlerOutDir, { recursive: true });

execFileSync(
  'cargo',
  ['build', '-p', 'indexbind-wasm', '--target', wasmTarget, '--release'],
  { stdio: 'inherit' },
);

execFileSync(
  'wasm-bindgen',
  ['--target', 'web', '--out-dir', wasmWebOutDir, wasmArtifact],
  { stdio: 'inherit' },
);

execFileSync(
  'wasm-bindgen',
  ['--target', 'bundler', '--out-dir', wasmBundlerOutDir, wasmArtifact],
  { stdio: 'inherit' },
);
