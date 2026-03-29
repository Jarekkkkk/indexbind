import {
  loadNativeModule,
  type NativeBuildStats,
  type NativeBuildDocument,
  type NativeIncrementalBuildStats,
  type NativeBuildOptions,
  type NativeCanonicalBuildStats,
} from './native.js';

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface BuildDocument {
  docId?: string;
  sourcePath?: string;
  relativePath: string;
  canonicalUrl?: string;
  title?: string;
  summary?: string;
  content: string;
  metadata?: Record<string, JsonValue>;
}

export interface BuildCanonicalBundleOptions {
  embeddingBackend?: 'hashing' | 'model2vec';
  hashingDimensions?: number;
  model?: string;
  batchSize?: number;
  sourceRootId?: string;
  sourceRootPath?: string;
  targetTokens?: number;
  overlapTokens?: number;
}

export interface CanonicalBuildStats {
  documentCount: number;
  chunkCount: number;
  vectorDimensions: number;
}

export interface BuildStats {
  documentCount: number;
  chunkCount: number;
}

export interface IncrementalBuildStats {
  scannedDocumentCount: number;
  newDocumentCount: number;
  changedDocumentCount: number;
  unchangedDocumentCount: number;
  removedDocumentCount: number;
  activeDocumentCount: number;
  activeChunkCount: number;
}

export async function buildCanonicalBundle(
  outputDir: string,
  documents: BuildDocument[],
  options: BuildCanonicalBundleOptions = {},
): Promise<CanonicalBuildStats> {
  const module = loadNativeModule();
  const nativeDocuments = documents.map(mapBuildDocument);
  const nativeOptions: NativeBuildOptions = {
    embeddingBackend: options.embeddingBackend,
    hashingDimensions: options.hashingDimensions,
    model: options.model,
    batchSize: options.batchSize,
    sourceRootId: options.sourceRootId,
    sourceRootPath: options.sourceRootPath,
    targetTokens: options.targetTokens,
    overlapTokens: options.overlapTokens,
  };
  return mapBuildStats(module.buildCanonicalBundle(outputDir, nativeDocuments, nativeOptions));
}

export async function updateBuildCache(
  cachePath: string,
  documents: BuildDocument[],
  options: BuildCanonicalBundleOptions = {},
  removedRelativePaths: string[] = [],
): Promise<IncrementalBuildStats> {
  const module = loadNativeModule();
  const nativeDocuments = documents.map(mapBuildDocument);
  const nativeOptions: NativeBuildOptions = {
    embeddingBackend: options.embeddingBackend,
    hashingDimensions: options.hashingDimensions,
    model: options.model,
    batchSize: options.batchSize,
    sourceRootId: options.sourceRootId,
    sourceRootPath: options.sourceRootPath,
    targetTokens: options.targetTokens,
    overlapTokens: options.overlapTokens,
  };
  return mapIncrementalBuildStats(
    module.updateBuildCacheFromDocuments(
      cachePath,
      nativeDocuments,
      removedRelativePaths,
      nativeOptions,
    ),
  );
}

export async function exportArtifactFromBuildCache(
  cachePath: string,
  outputPath: string,
): Promise<BuildStats> {
  const module = loadNativeModule();
  return mapPlainBuildStats(module.exportArtifactFromCache(cachePath, outputPath));
}

export async function exportCanonicalBundleFromBuildCache(
  cachePath: string,
  outputDir: string,
): Promise<CanonicalBuildStats> {
  const module = loadNativeModule();
  return mapBuildStats(module.exportCanonicalBundleFromCache(cachePath, outputDir));
}

function mapBuildDocument(document: BuildDocument): NativeBuildDocument {
  return {
    docId: document.docId,
    sourcePath: document.sourcePath,
    relativePath: document.relativePath,
    canonicalUrl: document.canonicalUrl,
    title: document.title,
    summary: document.summary,
    content: document.content,
    metadataJson: JSON.stringify(document.metadata ?? {}),
  };
}

function mapBuildStats(stats: NativeCanonicalBuildStats): CanonicalBuildStats {
  return {
    documentCount: stats.documentCount,
    chunkCount: stats.chunkCount,
    vectorDimensions: stats.vectorDimensions,
  };
}

function mapPlainBuildStats(stats: NativeBuildStats): BuildStats {
  return {
    documentCount: stats.documentCount,
    chunkCount: stats.chunkCount,
  };
}

function mapIncrementalBuildStats(stats: NativeIncrementalBuildStats): IncrementalBuildStats {
  return {
    scannedDocumentCount: stats.scannedDocumentCount,
    newDocumentCount: stats.newDocumentCount,
    changedDocumentCount: stats.changedDocumentCount,
    unchangedDocumentCount: stats.unchangedDocumentCount,
    removedDocumentCount: stats.removedDocumentCount,
    activeDocumentCount: stats.activeDocumentCount,
    activeChunkCount: stats.activeChunkCount,
  };
}
