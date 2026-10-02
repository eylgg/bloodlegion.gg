import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Boss, LootEntry } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** The filters the page passes through to `GET /api/loot`, kept in the URL so a view can be shared. */
const FILTERS = ['zone', 'boss_id', 'class', 'quality'] as const;

export const load: PageLoad = async ({ url, fetch }) => {
	const query = new URLSearchParams();
	for (const key of FILTERS) {
		const value = url.searchParams.get(key);
		if (value) query.set(key, value);
	}
	try {
		const [loot, bosses] = await Promise.all([
			api.get<LootEntry[]>(`/api/loot?${query}`, { fetch }),
			api.get<Boss[]>('/api/bosses', { fetch })
		]);
		return { loot, bosses };
	} catch (err) {
		error(statusFrom(err, 503), 'The loot history is unavailable right now.');
	}
};
