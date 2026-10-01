import type { HandleFetch } from '@sveltejs/kit';
import { env } from '$env/dynamic/private';

/**
 * Server-side loads call the API with same-origin paths (`/api/...`); this forwards them to the
 * backend, carrying the browser's cookie so the session resolves. In production and in local
 * development the reverse proxy (Caddy) does the same for the browser's own requests.
 */
const BACKEND_URL = env.BACKEND_URL ?? 'http://localhost:8080';
const PROXIED_PREFIXES = ['/api', '/auth', '/.well-known'];

function isProxied(pathname: string): boolean {
	return PROXIED_PREFIXES.some((p) => pathname === p || pathname.startsWith(p + '/'));
}

export const handleFetch: HandleFetch = async ({ event, request, fetch }) => {
	const url = new URL(request.url);
	const sameOrigin = url.origin === event.url.origin;
	if (sameOrigin && isProxied(url.pathname)) {
		const backend = new URL(BACKEND_URL);
		url.protocol = backend.protocol;
		url.host = backend.host;
		request = new Request(url, request);
		const cookie = event.request.headers.get('cookie');
		if (cookie) request.headers.set('cookie', cookie);
		request.headers.set('origin', event.url.origin);
	}
	return fetch(request);
};
