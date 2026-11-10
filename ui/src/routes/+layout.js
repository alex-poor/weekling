// Pure client-side SPA: no SSR, no prerendering. The server (Rust) only serves
// the static bundle and the JSON API; all routing happens in the browser.
export const ssr = false;
export const prerender = false;
