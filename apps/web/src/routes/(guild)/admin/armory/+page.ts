import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

/** The armory check: superusers only; it runs from the page, since it calls Blizzard. */
export const load: PageLoad = async ({ parent }) => {
	const { user } = await parent();
	if (!user.is_superuser) error(403, 'Only admins check the characters against the armory.');
	return {};
};
