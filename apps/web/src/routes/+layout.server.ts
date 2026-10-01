import type { LayoutServerLoad } from './$types';
import type { User } from '$lib/types';
import { api, ApiError } from '$lib/api';

/**
 * Resolves the signed-in user for every page, or `null` when anonymous. An unreachable backend
 * also counts as anonymous: the site is a logo and a login button, and both should still render
 * while the API is down or not deployed at all.
 */
export const load: LayoutServerLoad = async ({ fetch }) => {
	try {
		const user = await api.get<User>('/api/users/self', { fetch });
		return { user };
	} catch (err) {
		if (err instanceof ApiError && (err.status === 401 || err.status === 0)) {
			return { user: null };
		}
		console.error('loading the session user failed', err);
		return { user: null };
	}
};
