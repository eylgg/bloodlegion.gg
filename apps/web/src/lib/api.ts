/**
 * Minimal typed client for the Blood Legion API.
 *
 * The backend reports every failure as RFC 9457 `application/problem+json`;
 * this wraps `fetch` so callers get the parsed body on success and a thrown
 * {@link ApiError} carrying the problem details on failure.
 *
 * Load functions must pass SvelteKit's `fetch` so server-side calls forward
 * cookies; client-side event handlers can omit it.
 */

export type Problem = {
	status: number;
	title: string;
	detail: string;
};

export class ApiError extends Error {
	readonly problem: Problem;

	constructor(problem: Problem) {
		super(problem.detail || problem.title);
		this.name = 'ApiError';
		this.problem = problem;
	}

	get status(): number {
		return this.problem.status;
	}
}

type Fetch = typeof globalThis.fetch;

type RequestOptions = {
	fetch?: Fetch;
	body?: unknown;
};

// Deliberately avoids reading response headers: SvelteKit only exposes
// allowlisted headers on fetches it serializes for hydration, so header
// reads inside universal loads throw during SSR. Body text is always
// available.
async function problemFrom(response: Response): Promise<Problem> {
	const fallback: Problem = {
		status: response.status,
		title: response.statusText || `HTTP ${response.status}`,
		detail: ''
	};
	try {
		const body = JSON.parse(await response.text());
		return {
			status: typeof body.status === 'number' ? body.status : fallback.status,
			title: typeof body.title === 'string' ? body.title : fallback.title,
			detail: typeof body.detail === 'string' ? body.detail : fallback.detail
		};
	} catch {
		return fallback;
	}
}

async function request<T>(method: string, path: string, options: RequestOptions): Promise<T> {
	const fetcher = options.fetch ?? globalThis.fetch;
	const init: RequestInit = { method };
	if (options.body !== undefined) {
		init.headers = { 'content-type': 'application/json' };
		init.body = JSON.stringify(options.body);
	}

	let response: Response;
	try {
		response = await fetcher(path, init);
	} catch (err) {
		// A superseded navigation (e.g. the browser back button) aborts its
		// in-flight load fetches. Let that AbortError propagate so SvelteKit can
		// cancel the navigation quietly, instead of masking it as a network error
		// that the loads then escalate into a spurious 500 page.
		if (err != null && (err as { name?: unknown }).name === 'AbortError') {
			throw err;
		}
		throw new ApiError({
			status: 0,
			title: 'Network Error',
			detail: 'Unable to reach the server. Please check your connection.'
		});
	}

	if (!response.ok) {
		throw new ApiError(await problemFrom(response));
	}
	// Some endpoints respond with no body (204, or 200 from cookie-only
	// handlers like login).
	const text = await response.text();
	return text ? (JSON.parse(text) as T) : (undefined as T);
}

export const api = {
	get: <T>(path: string, options: RequestOptions = {}) => request<T>('GET', path, options),
	post: <T>(path: string, body?: unknown, options: RequestOptions = {}) =>
		request<T>('POST', path, { ...options, body }),
	patch: <T>(path: string, body?: unknown, options: RequestOptions = {}) =>
		request<T>('PATCH', path, { ...options, body }),
	put: <T>(path: string, body?: unknown, options: RequestOptions = {}) =>
		request<T>('PUT', path, { ...options, body }),
	del: <T>(path: string, options: RequestOptions = {}) => request<T>('DELETE', path, options)
};

/** A human-readable message for any error {@link api} throws. */
export function errorMessage(error: unknown, fallback: string): string {
	return error instanceof ApiError && error.message ? error.message : fallback;
}

/**
 * The HTTP status to hand SvelteKit's `error()` for an error {@link api} threw.
 * Falls back (default 500) for non-API errors and for statuses outside the
 * 400-599 range `error()` accepts; notably the `status: 0` network failure.
 */
export function statusFrom(error: unknown, fallback = 500): number {
	if (error instanceof ApiError && error.status >= 400 && error.status <= 599) {
		return error.status;
	}
	return fallback;
}
