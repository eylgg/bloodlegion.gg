import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { LoginOptions } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ fetch }) => {
	try {
		const options = await api.get<LoginOptions>('/api/auth/providers', { fetch });
		return { options };
	} catch (err) {
		// An unreachable backend is the one failure the logo-only site expects: say so plainly.
		error(statusFrom(err, 503), 'Sign-in is unavailable right now.');
	}
};
