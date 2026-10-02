import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { BossDetail } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ params, fetch }) => {
	try {
		return { detail: await api.get<BossDetail>(`/api/bosses/${params.id}`, { fetch }) };
	} catch (err) {
		error(statusFrom(err, 503), 'That boss could not be loaded.');
	}
};
