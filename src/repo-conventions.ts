import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import type { BuildDocument } from './build.js';
import type { ArtifactInfo, SearchOptions } from './index.js';

export interface BuildConventionContext {
  rootDir: string;
  command: 'build' | 'build-bundle' | 'update-cache';
  sourceRootId: string;
  sourceRootPath: string;
}

export interface SearchConventionContext {
  artifactPath: string;
  sourceRootPath: string;
  artifactInfo: ArtifactInfo;
}

interface BuildConventionModule {
  includeDocument?: (
    relativePath: string,
    context: BuildConventionContext,
  ) => boolean | Promise<boolean>;
  transformDocument?: (
    document: BuildDocument,
    context: BuildConventionContext,
  ) => BuildDocument | null | Promise<BuildDocument | null>;
}

interface SearchConventionModule {
  profiles?: {
    default?: Partial<SearchOptions>;
  };
  transformQuery?: (
    query: string,
    context: SearchConventionContext,
  ) => { query: string } | Promise<{ query: string }>;
}

interface LoadedBuildConvention {
  filePath: string;
  hooks: BuildConventionModule;
}

interface LoadedSearchConvention {
  filePath: string;
  hooks: SearchConventionModule;
}

const BUILD_CONVENTION_FILE = 'indexbind.build.js';
const SEARCH_CONVENTION_FILE = 'indexbind.search.js';

export async function loadBuildConvention(rootDir: string): Promise<LoadedBuildConvention | null> {
  return loadConventionModule(rootDir, BUILD_CONVENTION_FILE);
}

export async function loadSearchConvention(
  rootDir: string,
): Promise<LoadedSearchConvention | null> {
  return loadConventionModule(rootDir, SEARCH_CONVENTION_FILE);
}

export async function applyBuildConvention(
  documents: BuildDocument[],
  convention: LoadedBuildConvention | null,
  context: BuildConventionContext,
): Promise<BuildDocument[]> {
  if (!convention) {
    return documents;
  }

  const transformed: BuildDocument[] = [];
  for (const document of documents) {
    if (convention.hooks.includeDocument) {
      const included = await convention.hooks.includeDocument(document.relativePath, context);
      if (!included) {
        continue;
      }
    }

    let nextDocument = document;
    if (convention.hooks.transformDocument) {
      const candidate = await convention.hooks.transformDocument(document, context);
      if (candidate === null) {
        continue;
      }
      if (!candidate || typeof candidate !== 'object') {
        throw new Error(
          `${convention.filePath}: transformDocument must return a document object or null`,
        );
      }
      if (candidate.relativePath !== document.relativePath) {
        throw new Error(
          `${convention.filePath}: transformDocument must not change relativePath for ${document.relativePath}`,
        );
      }
      nextDocument = {
        ...candidate,
        sourcePath: document.sourcePath,
      };
    }

    transformed.push(nextDocument);
  }

  return transformed;
}

export async function applySearchConvention(
  query: string,
  explicitOptions: SearchOptions,
  convention: LoadedSearchConvention | null,
  context: SearchConventionContext,
): Promise<{ query: string; options: SearchOptions }> {
  if (!convention) {
    return {
      query,
      options: explicitOptions,
    };
  }

  const profile = convention.hooks.profiles?.default;
  assertNoLegacyHybridOption(profile, convention.filePath);
  assertNoLegacyHybridOption(explicitOptions, convention.filePath);

  let effectiveQuery = query;
  if (convention.hooks.transformQuery) {
    const result = await convention.hooks.transformQuery(query, context);
    if (!result || typeof result !== 'object' || typeof result.query !== 'string') {
      throw new Error(`${convention.filePath}: transformQuery must return an object with query`);
    }
    effectiveQuery = result.query;
  }

  return {
    query: effectiveQuery,
    options: mergeSearchOptions(profile, explicitOptions),
  };
}

export function sourceRootPathFromArtifactInfo(info: ArtifactInfo): string | null {
  const sourceRoot = info.sourceRoot;
  if (!sourceRoot || typeof sourceRoot !== 'object') {
    return null;
  }
  const originalPath =
    readStringProperty(sourceRoot as Record<string, unknown>, 'original_path') ??
    readStringProperty(sourceRoot as Record<string, unknown>, 'originalPath');
  return originalPath ? path.resolve(originalPath) : null;
}

export function sourceRootContext(rootDir: string): {
  sourceRootId: string;
  sourceRootPath: string;
} {
  return {
    sourceRootId: 'root',
    sourceRootPath: rootDir,
  };
}

function mergeSearchOptions(
  profile: Partial<SearchOptions> | undefined,
  explicit: SearchOptions,
): SearchOptions {
  if (!profile) {
    return explicit;
  }

  return {
    topK: explicit.topK ?? profile.topK,
    mode: explicit.mode ?? profile.mode,
    minScore: explicit.minScore ?? profile.minScore,
    relativePathPrefix: explicit.relativePathPrefix ?? profile.relativePathPrefix,
    metadata: mergeStringRecord(profile.metadata, explicit.metadata),
    reranker: mergeObject(profile.reranker, explicit.reranker),
    scoreAdjustment: mergeObject(profile.scoreAdjustment, explicit.scoreAdjustment),
  };
}

function mergeStringRecord(
  profile: Record<string, string> | undefined,
  explicit: Record<string, string> | undefined,
): Record<string, string> | undefined {
  if (!profile && !explicit) {
    return undefined;
  }
  return {
    ...(profile ?? {}),
    ...(explicit ?? {}),
  };
}

function mergeObject<T extends object>(profile: T | undefined, explicit: T | undefined): T | undefined {
  if (!profile && !explicit) {
    return undefined;
  }
  return {
    ...(profile ?? {}),
    ...(explicit ?? {}),
  } as T;
}

async function loadConventionModule<T extends object>(
  rootDir: string,
  fileName: string,
): Promise<{ filePath: string; hooks: T } | null> {
  const filePath = path.resolve(rootDir, fileName);
  let stat;
  try {
    stat = await fs.stat(filePath);
  } catch {
    return null;
  }
  const href = `${pathToFileURL(filePath).href}?mtime=${stat.mtimeMs}`;
  let loaded;
  try {
    loaded = (await import(href)) as Record<string, unknown>;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`failed to load ${filePath}: ${message}`);
  }

  const hooks = normalizeModuleShape(loaded) as T;
  return { filePath, hooks };
}

function normalizeModuleShape(module: Record<string, unknown>): Record<string, unknown> {
  const candidate =
    module.default && typeof module.default === 'object'
      ? (module.default as Record<string, unknown>)
      : module;
  return candidate;
}

function assertNoLegacyHybridOption(
  options: Partial<SearchOptions> | undefined,
  filePath: string,
): void {
  if (options && typeof options === 'object' && Object.prototype.hasOwnProperty.call(options, 'hybrid')) {
    throw new Error(
      `${filePath}: search option "hybrid" has been removed. Use mode: "hybrid", "vector", or "lexical" instead.`,
    );
  }
}

function readStringProperty(object: Record<string, unknown>, key: string): string | null {
  const value = object[key];
  return typeof value === 'string' ? value : null;
}
