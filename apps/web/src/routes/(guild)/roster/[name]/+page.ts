import { error, isRedirect, redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { CharacterDetail, Member } from '$lib/types';
import { api, statusFrom } from '$lib/api';
import { characterPath, characterSlug } from '$lib/guild';

/**
 * A character's page, by its name (`/roster/garr-bonecrush`). The lowercase name is the page's
 * own: any other capitalization redirects there for good.
 */
export const load: PageLoad = async ({ params, parent, fetch }) => {
	const { officer } = await parent();
	try {
		const [detail, members] = await Promise.all([
			api.get<CharacterDetail>(`/api/characters/named/${encodeURIComponent(params.name)}`, {
				fetch
			}),
			officer ? api.get<Member[]>('/api/guild/members', { fetch }) : Promise.resolve([])
		]);
		const { first_name, last_name } = detail.character;
		if (params.name !== characterSlug(first_name, last_name)) {
			redirect(308, characterPath(first_name, last_name));
		}
		return { detail, owners: members.map((m) => ({ id: m.id, username: m.username })) };
	} catch (err) {
		if (isRedirect(err)) throw err;
		error(statusFrom(err, 503), 'That character could not be loaded.');
	}
};
