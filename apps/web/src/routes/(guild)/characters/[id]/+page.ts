import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { CharacterDetail, Member } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ params, parent, fetch }) => {
	const { officer } = await parent();
	try {
		const [detail, members] = await Promise.all([
			api.get<CharacterDetail>(`/api/characters/${params.id}`, { fetch }),
			officer ? api.get<Member[]>('/api/guild/members', { fetch }) : Promise.resolve([])
		]);
		return { detail, owners: members.map((m) => ({ id: m.id, username: m.username })) };
	} catch (err) {
		error(statusFrom(err, 503), 'That character could not be loaded.');
	}
};
