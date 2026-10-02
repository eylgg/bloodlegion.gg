import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';
import type { GuildCharacter, LinkedAccount, LoginOptions } from '$lib/types';
import { api, statusFrom } from '$lib/api';

export const load: PageLoad = async ({ parent, fetch }) => {
	const { user } = await parent();
	try {
		const [accounts, options, characters] = await Promise.all([
			api.get<LinkedAccount[]>('/api/auth/linked-accounts', { fetch }),
			api.get<LoginOptions>('/api/auth/providers', { fetch }),
			api.get<GuildCharacter[]>('/api/characters', { fetch })
		]);
		return {
			accounts,
			providers: options.providers.flatMap((p) => (p.kind === 'oauth2' ? [p] : [])),
			characters: characters.filter((c) => c.user_id === user.id)
		};
	} catch (err) {
		error(statusFrom(err, 503), 'Your profile is unavailable right now.');
	}
};
