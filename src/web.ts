export {
  WebIndex,
  openWebIndex,
  type BestMatch,
  type DocumentHit,
  type JsonValue,
  type OpenWebIndexOptions,
  type RerankerOptions,
  type ScoreAdjustmentOptions,
  type SearchOptions,
  type WebArtifactInfo,
} from './web-core.js';

import type { OpenWebIndexOptions, WebIndex } from './web-core.js';

export async function openCloudflareIndex(
  base: string | URL,
  options: OpenWebIndexOptions = {},
): Promise<WebIndex> {
  const cloudflareModule = (await import('./cloudflare.js')) as {
    openWebIndex: (base: string | URL, options?: OpenWebIndexOptions) => Promise<WebIndex>;
  };
  return cloudflareModule.openWebIndex(base, options);
}
