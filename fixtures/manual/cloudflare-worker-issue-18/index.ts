import { openWebIndex } from '../../../dist/cloudflare.js';

interface Env {
  ASSETS: {
    fetch(request: Request): Promise<Response>;
  };
}

const bundleBaseUrl = 'https://mdorigin-search.invalid/search/index.bundle/';

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === '/api/search') {
      try {
        const query = url.searchParams.get('q') ?? 'rust guide';
        const index = await openVirtualBundleIndex(env);
        const hits = await index.search(query);
        return Response.json({
          query,
          topHit: hits[0]?.relativePath ?? null,
          score: hits[0]?.score ?? null,
          count: hits.length,
        });
      } catch (error) {
        return Response.json(
          {
            error: error instanceof Error ? (error.stack ?? error.message) : String(error),
          },
          { status: 500 },
        );
      }
    }

    if (url.pathname === '/healthz') {
      return new Response('ok');
    }

    return env.ASSETS.fetch(request);
  },
};

async function openVirtualBundleIndex(env: Env) {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (input: RequestInfo | URL, init?: RequestInit) => {
    const requestUrl =
      typeof input === 'string'
        ? input
        : input instanceof URL
          ? input.toString()
          : input.url;
    if (requestUrl.startsWith(bundleBaseUrl)) {
      const relativePath = requestUrl.slice(bundleBaseUrl.length);
      const assetRequest = new Request(new URL(relativePath, bundleBaseUrl), {
        method: 'GET',
      });
      return env.ASSETS.fetch(assetRequest);
    }

    return originalFetch(input as RequestInfo, init);
  };

  try {
    return await openWebIndex(new URL(bundleBaseUrl));
  } finally {
    globalThis.fetch = originalFetch;
  }
}
