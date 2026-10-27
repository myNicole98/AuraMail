export type WindowRoute = {
  compose: boolean;
  params: URLSearchParams;
};

const COMPOSE_PREFIX = "compose";

/**
 * Native compose windows are opened with a hash route. Query strings are not
 * reliable in WebView2 when a secondary window is created, so keep the query
 * form only for browser previews and older links.
 */
export function readWindowRoute(search: string, hash: string): WindowRoute {
  const hashValue = hash.startsWith("#") ? hash.slice(1) : hash;
  if (hashValue === COMPOSE_PREFIX || hashValue.startsWith(`${COMPOSE_PREFIX}?`)) {
    const query = hashValue.slice(COMPOSE_PREFIX.length).replace(/^\?/, "");
    return { compose: true, params: new URLSearchParams(query) };
  }

  const params = new URLSearchParams(search);
  return { compose: params.get("window") === "compose", params };
}
