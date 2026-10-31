import type { Role, Sex } from '$lib/types';

/**
 * Blizzard's own artwork, vendored under `static/wow` (see the backend's `wow::catalog` for
 * where each came from): the class icons and each Classic talent tree's icon. Role icons are
 * drawn as vectors in `RoleIcon.svelte` instead: the game's role art is too small to scale.
 */
export const classIcon = (classSlug: string) => `/wow/classes/${classSlug}.jpg`;
export const specIcon = (classSlug: string, specSlug: string) =>
	`/wow/specs/${classSlug}-${specSlug}.jpg`;
/** A race's portrait, from Forever's character creation; both Skyborne share the one it has. */
export const raceIcon = (raceSlug: string, sex: Sex) =>
	`/wow/races/${raceSlug.endsWith('-skyborne') ? 'skyborne' : raceSlug}-${sex}.jpg`;

export const ROLES: Role[] = ['tank', 'healer', 'melee', 'ranged'];

export const ROLE_LABEL: Record<Role, string> = {
	tank: 'Tank',
	healer: 'Healer',
	melee: 'Melee',
	ranged: 'Ranged'
};
