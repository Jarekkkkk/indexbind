import fs from 'node:fs';
import path from 'node:path';

const packageJsonPath = path.resolve('package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
const expectedVersion = process.env.RELEASE_VERSION ?? packageJson.version;
const gitRef = process.env.GITHUB_REF ?? '';
const refVersion = gitRef.startsWith('refs/tags/v') ? gitRef.slice('refs/tags/v'.length) : undefined;

if (packageJson.version !== expectedVersion) {
  throw new Error(`package.json version ${packageJson.version} does not match expected release version ${expectedVersion}`);
}

if (refVersion && refVersion !== packageJson.version) {
  throw new Error(`Git tag version ${refVersion} does not match package.json version ${packageJson.version}`);
}

console.log(`Verified release version ${packageJson.version}`);
