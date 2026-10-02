import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GuildCharacter, Member } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { officer } = await parent();
	try {
		const [characters, members] = await Promise.all([
			api.get<GuildCharacter[]>('/api/characters', { fetch }),
			// Only officers pick who plays a character.
			officer ? api.get<Member[]>('/api/guild/members', { fetch }) : Promise.resolve([])
		]);
		return { characters, owners: members.map((m) => ({ id: m.id, username: m.username })) };
	} catch (err) {
		error(statusFrom(err, 503), 'The characters are unavailable right now.');
	}
};
