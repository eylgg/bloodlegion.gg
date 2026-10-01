import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Registration } from '$lib/types';
import { api, ApiError, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ fetch }) => {
	try {
		const registration = await api.get<Registration>('/api/auth/oauth2/registration', { fetch });
		return { registration };
	} catch (err) {
		if (err instanceof ApiError && err.status === 404) {
			error(404, 'There is no sign-in waiting to be completed. Start again from the sign-in page.');
		}
		error(statusFrom(err, 503), 'Sign-in is unavailable right now.');
	}
};
