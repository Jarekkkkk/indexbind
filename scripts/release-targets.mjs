export const ROOT_PACKAGE_NAME = 'indexbind';

export const TARGETS = [
  {
    key: 'darwin-arm64',
    packageName: '@indexbind/native-darwin-arm64',
    artifactName: 'indexbind.darwin-arm64.node',
    runner: 'macos-latest',
    os: 'darwin',
    arch: 'arm64',
  },
  {
    key: 'darwin-x64',
    packageName: '@indexbind/native-darwin-x64',
    artifactName: 'indexbind.darwin-x64.node',
    runner: 'macos-13',
    os: 'darwin',
    arch: 'x64',
  },
  {
    key: 'linux-x64-gnu',
    packageName: '@indexbind/native-linux-x64-gnu',
    artifactName: 'indexbind.linux-x64.node',
    runner: 'ubuntu-24.04',
    os: 'linux',
    arch: 'x64',
  },
];

export const OPTIONAL_DEPENDENCIES = Object.fromEntries(
  TARGETS.map((target) => [target.packageName, process.env.RELEASE_VERSION ?? '0.1.0']),
);

export function getTargetByKey(key) {
  const target = TARGETS.find((candidate) => candidate.key === key);
  if (!target) {
    throw new Error(`Unsupported target: ${key}`);
  }
  return target;
}
