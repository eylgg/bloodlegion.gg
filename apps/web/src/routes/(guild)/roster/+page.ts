import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GuildCharacter, Member, NotAddedMember } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { officer, user } = await parent();
	try {
		const [characters, members, notAdded] = await Promise.all([
			api.get<GuildCharacter[]>('/api/characters', { fetch }),
			// Officers pick from them who plays a character, and the administrator sees the
			// accounts with no characters yet.
			api.get<Member[]>('/api/guild/members', { fetch }),
			// The roster members uploads could not add, for the administrator to add by hand.
			user.is_superuser
				? api.get<NotAddedMember[]>('/api/admin/rosters/not-added', { fetch })
				: ([] as NotAddedMember[])
		]);
		return {
			characters,
			notAdded,
			owners: officer ? members.map((m) => ({ id: m.id, username: m.username })) : [],
			members: members.map(({ id, username, guild_rank }) => ({ id, username, guild_rank }))
		};
	} catch (err) {
		error(statusFrom(err, 503), 'The roster is unavailable right now.');
	}
};
