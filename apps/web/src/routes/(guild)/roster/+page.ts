import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GuildCharacter, Member } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { officer } = await parent();
	try {
		const [characters, members] = await Promise.all([
			api.get<GuildCharacter[]>('/api/characters', { fetch }),
			// Each player's rank; officers also pick from them who plays a character, and the
			// administrator sees the accounts with no characters yet.
			api.get<Member[]>('/api/guild/members', { fetch })
		]);
		return {
			characters,
			owners: officer ? members.map((m) => ({ id: m.id, username: m.username })) : [],
			members: members.map(({ id, username, guild_rank }) => ({ id, username, guild_rank }))
		};
	} catch (err) {
		error(statusFrom(err, 503), 'The roster is unavailable right now.');
	}
};
