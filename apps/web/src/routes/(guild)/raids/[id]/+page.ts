import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { Boss, Effect, GuildCharacter, Item, RaidDetail } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ params, parent, fetch }) => {
	const { officer } = await parent();
	try {
		const [detail, bosses, effects, items, characters] = await Promise.all([
			api.get<RaidDetail>(`/api/raids/${params.id}`, { fetch }),
			api.get<Boss[]>('/api/bosses', { fetch }),
			api.get<Effect[]>('/api/raids/effects', { fetch }),
			// What officers pick from when recording attendance and loot.
			officer ? api.get<Item[]>('/api/items', { fetch }) : Promise.resolve([]),
			officer ? api.get<GuildCharacter[]>('/api/characters', { fetch }) : Promise.resolve([])
		]);
		return { detail, bosses, effects, items, characters };
	} catch (err) {
		error(statusFrom(err, 503), 'That raid could not be loaded.');
	}
};
