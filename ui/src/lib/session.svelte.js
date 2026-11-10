// Reactive session state shared across the app. Hydrated from /api/me on load
// and set on login; `user` is null when signed out.
export const session = $state({ user: null });
