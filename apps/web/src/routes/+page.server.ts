import type { PageServerLoad } from './$types';
import type { Announcement, LoginOptions, LoginProvider } from '$lib/types';
import { api } from '$lib/api';

/**
 * The ways to sign in, for the front page's button, and the latest Discord announcements. An
 * unreachable backend means none of either, and the page falls back to a plain link to the sign-in
 * page, which then says what is wrong.
 */
export const load: PageServerLoad = async ({ fetch }) => {
	const [providers, announcements] = await Promise.all([
		api
			.get<LoginOptions>('/api/auth/providers', { fetch })
			.then((options) => options.providers)
			.catch((): LoginProvider[] => []),
		api.get<Announcement[]>('/api/announcements', { fetch }).catch((): Announcement[] => [])
	]);
	return { providers, announcements };
};
