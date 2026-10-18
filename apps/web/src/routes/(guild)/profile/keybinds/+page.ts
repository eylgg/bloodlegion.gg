import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GuildCharacter } from '$lib/types';
import { api, statusFrom } from '$lib/api';

/** Your characters, for planning each one's keybinds; `?character=` picks one, your main by default. */
export const load: PageLoad = async ({ parent, url, fetch }) => {
	const { user } = await parent();
	try {
		const characters = (await api.get<GuildCharacter[]>('/api/characters', { fetch })).filter(
			(c) => c.user_id === user.id
		);
		const param = Number(url.searchParams.get('character'));
		const character =
			characters.find((c) => c.id === param) ??
			characters.find((c) => c.is_main) ??
			characters[0] ??
			null;
		return { characters, character };
	} catch (err) {
		error(statusFrom(err, 503), 'Your keybinds are unavailable right now.');
	}
};
