import {
  loadNativeModule,
  type NativeArtifactInfo,
  type NativeBestMatch,
  type NativeDocumentHit,
  type NativeIndex,
  type NativeOpenIndexOptions,
  type NativeSearchOptions,
} from './native.js';
import {
  applySearchConvention,
  loadSearchConvention,
  sourceRootPathFromArtifactInfo,
} from './repo-conventions.js';

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface SearchOptions {
  topK?: number;
  mode?: 'hybrid' | 'vector' | 'lexical';
  minScore?: number;
  reranker?: RerankerOptions;
  relativePathPrefix?: string;
  metadata?: Record<string, string>;
  scoreAdjustment?: ScoreAdjustmentOptions;
}

export interface RerankerOptions {
  kind?: 'embedding-v1' | 'heuristic-v1';
  candidatePoolSize?: number;
}

export interface ScoreAdjustmentOptions {
  metadataNumericMultiplier?: string;
}

export interface BestMatch {
  chunkId: number;
  excerpt: string;
  headingPath: string[];
  charStart: number;
  charEnd: number;
  score: number;
}

export interface DocumentHit {
  docId: string;
  relativePath: string;
  canonicalUrl?: string;
  title?: string;
  summary?: string;
  metadata: Record<string, JsonValue>;
  score: number;
  bestMatch: BestMatch;
}

export interface ArtifactInfo {
  schemaVersion: string;
  builtAt: string;
  embeddingBackend: unknown;
  lexicalTokenizer: string;
  sourceRoot: unknown;
  documentCount: number;
  chunkCount: number;
}

export interface OpenIndexOptions {
  modeProfile?: 'hybrid' | 'lexical';
  applySearchConvention?: boolean;
}

export class Index {
  readonly #nativeIndex: NativeIndex;
  readonly #modeProfile: 'hybrid' | 'lexical';
  readonly #artifactPath: string;
  readonly #artifactInfo: ArtifactInfo;
  readonly #searchConventionPromise: Promise<Awaited<ReturnType<typeof loadSearchConvention>>>;

  private constructor(
    nativeIndex: NativeIndex,
    artifactPath: string,
    artifactInfo: ArtifactInfo,
    modeProfile: 'hybrid' | 'lexical',
    searchConventionPromise: Promise<Awaited<ReturnType<typeof loadSearchConvention>>>,
  ) {
    this.#nativeIndex = nativeIndex;
    this.#artifactPath = artifactPath;
    this.#artifactInfo = artifactInfo;
    this.#modeProfile = modeProfile;
    this.#searchConventionPromise = searchConventionPromise;
  }

  static async open(artifactPath: string, options: OpenIndexOptions = {}): Promise<Index> {
    const module = loadNativeModule();
    const modeProfile = options.modeProfile ?? 'hybrid';
    const nativeOptions: NativeOpenIndexOptions = {
      modeProfile,
    };
    const nativeIndex = module.NativeIndex.open(artifactPath, nativeOptions);
    const artifactInfo = mapArtifactInfo(nativeIndex.info());
    const sourceRootPath = sourceRootPathFromArtifactInfo(artifactInfo);
    const searchConventionPromise = options.applySearchConvention === false
      ? Promise.resolve(null)
      : sourceRootPath
      ? loadSearchConvention(sourceRootPath)
      : Promise.resolve(null);
    return new Index(nativeIndex, artifactPath, artifactInfo, modeProfile, searchConventionPromise);
  }

  info(): ArtifactInfo {
    return this.#artifactInfo;
  }

  async search(query: string, options: SearchOptions = {}): Promise<DocumentHit[]> {
    assertNoLegacyHybridOption(options);
    const searchConvention = await this.#searchConventionPromise;
    const resolved = await applySearchConvention(query, options, searchConvention, {
      artifactPath: this.#artifactPath,
      sourceRootPath: sourceRootPathFromArtifactInfo(this.#artifactInfo) ?? '.',
      artifactInfo: this.#artifactInfo,
    });
    const nativeOptions: NativeSearchOptions = {
      topK: resolved.options.topK,
      mode: resolved.options.mode ?? this.#modeProfile,
      minScore: resolved.options.minScore,
      reranker: resolved.options.reranker,
      relativePathPrefix: resolved.options.relativePathPrefix,
      metadata: resolved.options.metadata,
      scoreAdjustment: resolved.options.scoreAdjustment,
    };
    return this.#nativeIndex.search(resolved.query, nativeOptions).map(mapHit);
  }
}

export function openIndex(artifactPath: string, options: OpenIndexOptions = {}): Promise<Index> {
  return Index.open(artifactPath, options);
}

function assertNoLegacyHybridOption(options: SearchOptions): void {
  if (options && typeof options === 'object' && Object.prototype.hasOwnProperty.call(options, 'hybrid')) {
    throw new Error(
      'Search option "hybrid" has been removed. Use mode: "hybrid", "vector", or "lexical" instead.',
    );
  }
}

function mapArtifactInfo(info: NativeArtifactInfo): ArtifactInfo {
  return {
    schemaVersion: info.schemaVersion,
    builtAt: info.builtAt,
    embeddingBackend: JSON.parse(info.embeddingBackend),
    lexicalTokenizer: info.lexicalTokenizer,
    sourceRoot: JSON.parse(info.sourceRoot),
    documentCount: info.documentCount,
    chunkCount: info.chunkCount,
  };
}

function mapHit(hit: NativeDocumentHit): DocumentHit {
  return {
    docId: hit.docId,
    relativePath: hit.relativePath,
    canonicalUrl: hit.canonicalUrl,
    title: hit.title,
    summary: hit.summary,
    metadata: JSON.parse(hit.metadata) as Record<string, JsonValue>,
    score: hit.score,
    bestMatch: mapBestMatch(hit.bestMatch),
  };
}

function mapBestMatch(bestMatch: NativeBestMatch): BestMatch {
  return {
    chunkId: bestMatch.chunkId,
    excerpt: bestMatch.excerpt,
    headingPath: bestMatch.headingPath,
    charStart: bestMatch.charStart,
    charEnd: bestMatch.charEnd,
    score: bestMatch.score,
  };
}
