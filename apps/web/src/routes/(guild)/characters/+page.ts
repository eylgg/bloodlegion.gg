import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GuildCharacter, Member, Rank } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { officer } = await parent();
	try {
		const [characters, members] = await Promise.all([
			api.get<GuildCharacter[]>('/api/characters', { fetch }),
			// For each player's rank; officers also pick from them who plays a character.
			api.get<Member[]>('/api/guild/members', { fetch })
		]);
		return {
			characters,
			ranks: Object.fromEntries(members.map((m) => [m.id, m.guild_rank])) as Record<number, Rank>,
			owners: officer ? members.map((m) => ({ id: m.id, username: m.username })) : []
		};
	} catch (err) {
		error(statusFrom(err, 503), 'The characters are unavailable right now.');
	}
};
