import type { PageServerLoad } from './$types';
import type { LoginOptions } from '$lib/types';
import { api } from '$lib/api';

/**
 * The ways to sign in, for the front page's button. An unreachable backend means none, and the
 * page falls back to a plain link to the sign-in page, which then says what is wrong.
 */
export const load: PageServerLoad = async ({ fetch }) => {
	try {
		const options = await api.get<LoginOptions>('/api/auth/providers', { fetch });
		return { providers: options.providers };
	} catch {
		return { providers: [] };
	}
};
