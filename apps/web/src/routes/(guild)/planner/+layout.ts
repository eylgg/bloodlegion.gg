import { error } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

/** The planner, raids and loot, is for officers. */
export const load: LayoutLoad = async ({ parent }) => {
	const { officer } = await parent();
	if (!officer) error(403, 'The planner is for officers.');
};
