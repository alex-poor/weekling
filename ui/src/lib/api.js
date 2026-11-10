import { session } from './session.svelte.js';

/** Read the double-submit CSRF token the server set as a readable cookie. */
export function csrfToken() {
	const m = document.cookie.match(/(?:^|; )wk_csrf=([^;]+)/);
	return m ? decodeURIComponent(m[1]) : null;
}

/**
 * Call the JSON admin API. Sends the session cookie, attaches the CSRF header on
 * writes, and bounces to /login on 401.
 */
export async function api(path, { method = 'GET', body } = {}) {
	const headers = {};
	if (body !== undefined) headers['Content-Type'] = 'application/json';
	if (method !== 'GET' && method !== 'HEAD') {
		const token = csrfToken();
		if (token) headers['X-CSRF-Token'] = token;
	}
	const res = await fetch('/api' + path, {
		method,
		headers,
		credentials: 'same-origin',
		body: body !== undefined ? JSON.stringify(body) : undefined
	});

	if (res.status === 401) {
		session.user = null;
		if (!location.pathname.startsWith('/login')) location.href = '/login';
		throw new Error('Not signed in');
	}
	if (!res.ok) {
		const text = (await res.text()).trim();
		throw new Error(text || res.statusText);
	}
	if (res.status === 204) return null;
	return res.json();
}
