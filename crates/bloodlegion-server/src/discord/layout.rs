//! What the bot keeps in the server, as the site has it: three roles that follow the guild ranks
//! (officers, the raiding roster, everyone else), and four categories that follow the access
//! levels, each open to its level and the ones above it. #announcements, in Public, is the one
//! channel the bot makes: everyone reads it, officers post.
//!
//! Pure: the permission entries and role changes are worked out here and applied by `sync`.

use serenity::all::{
    ChannelId, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId, UserId,
};

use crate::guild::Rank;

/// A level of access to the guild, and the Discord role a member gets for it, highest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    /// The leader and officers.
    Officer,
    /// The raiding roster: raiders and trials.
    Raider,
    /// Everyone else with an account.
    Member,
}

impl Access {
    pub const ALL: [Access; 3] = [Access::Officer, Access::Raider, Access::Member];

    pub fn of(rank: Rank) -> Access {
        match rank {
            _ if rank.is_officer() => Access::Officer,
            Rank::Raider | Rank::Trial => Access::Raider,
            _ => Access::Member,
        }
    }

    /// The bot's key for the role (`discord_objects`).
    pub fn key(self) -> &'static str {
        match self {
            Access::Officer => "role:officer",
            Access::Raider => "role:raider",
            Access::Member => "role:member",
        }
    }

    /// The role's name when the bot makes it; renaming it by hand afterwards is fine.
    pub fn role_name(self) -> &'static str {
        match self {
            Access::Officer => "Officer",
            Access::Raider => "Raider",
            Access::Member => "Member",
        }
    }
}

/// The ids of the three roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roles {
    pub officer: RoleId,
    pub raider: RoleId,
    pub member: RoleId,
}

impl Roles {
    pub fn get(&self, access: Access) -> RoleId {
        match access {
            Access::Officer => self.officer,
            Access::Raider => self.raider,
            Access::Member => self.member,
        }
    }

    pub fn all(&self) -> [RoleId; 3] {
        [self.officer, self.raider, self.member]
    }
}

/// Who a category is open to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Anyone in the server.
    Everyone,
    /// This level and the ones above it.
    AtLeast(Access),
}

/// A category the bot keeps.
pub struct Category {
    pub key: &'static str,
    pub name: &'static str,
    pub audience: Audience,
}

/// The bot's categories, in the order it makes them (so they line up that way in Discord).
pub const CATEGORIES: [Category; 4] = [
    Category {
        key: "category:public",
        name: "Public",
        audience: Audience::Everyone,
    },
    Category {
        key: "category:members",
        name: "Members",
        audience: Audience::AtLeast(Access::Member),
    },
    Category {
        key: "category:raiders",
        name: "Raiders",
        audience: Audience::AtLeast(Access::Raider),
    },
    Category {
        key: "category:officers",
        name: "Officers",
        audience: Audience::AtLeast(Access::Officer),
    },
];

/// The public category, which holds #announcements.
pub const PUBLIC: &Category = &CATEGORIES[0];

pub const ANNOUNCEMENTS_KEY: &str = "channel:announcements";
pub const ANNOUNCEMENTS_NAME: &str = "announcements";

/// Where everything the bot keeps is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub roles: Roles,
    pub announcements: ChannelId,
}

fn role(id: RoleId, allow: Permissions, deny: Permissions) -> PermissionOverwrite {
    PermissionOverwrite {
        allow,
        deny,
        kind: PermissionOverwriteType::Role(id),
    }
}

/// The permission entries the bot keeps on one of its categories. It sets only these, leaving any
/// others someone adds by hand. `everyone` is the server's @everyone role (its id is the server's);
/// `bot` keeps sight of the closed categories it manages.
pub fn category_overwrites(
    audience: Audience,
    roles: &Roles,
    everyone: RoleId,
    bot: UserId,
) -> Vec<PermissionOverwrite> {
    let view = Permissions::VIEW_CHANNEL;
    match audience {
        Audience::Everyone => vec![role(everyone, view, Permissions::empty())],
        Audience::AtLeast(level) => {
            let mut overwrites = vec![
                role(everyone, Permissions::empty(), view),
                PermissionOverwrite {
                    allow: view | Permissions::READ_MESSAGE_HISTORY,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Member(bot),
                },
            ];
            overwrites.extend(
                Access::ALL
                    .into_iter()
                    .filter(|access| *access <= level)
                    .map(|access| role(roles.get(access), view, Permissions::empty())),
            );
            overwrites
        }
    }
}

/// #announcements: everyone reads it, and only officers post (or start threads).
pub fn announcements_overwrites(roles: &Roles, everyone: RoleId) -> Vec<PermissionOverwrite> {
    let read = Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    let post = Permissions::SEND_MESSAGES
        | Permissions::SEND_MESSAGES_IN_THREADS
        | Permissions::CREATE_PUBLIC_THREADS
        | Permissions::CREATE_PRIVATE_THREADS;
    vec![
        role(everyone, read, post),
        role(roles.officer, read | post, Permissions::empty()),
    ]
}

/// The bot's roles to give and take so a member holding `current` ends up with exactly the one
/// for `access` (none when they have no linked account, or it is disabled). Roles the bot does not
/// keep are left alone.
pub fn role_changes(
    current: &[RoleId],
    access: Option<Access>,
    roles: &Roles,
) -> (Vec<RoleId>, Vec<RoleId>) {
    let wanted = access.map(|access| roles.get(access));
    let add = wanted
        .filter(|id| !current.contains(id))
        .into_iter()
        .collect();
    let remove = roles
        .all()
        .into_iter()
        .filter(|id| Some(*id) != wanted && current.contains(id))
        .collect();
    (add, remove)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROLES: Roles = Roles {
        officer: RoleId::new(1),
        raider: RoleId::new(2),
        member: RoleId::new(3),
    };
    const EVERYONE: RoleId = RoleId::new(100);
    const BOT: UserId = UserId::new(200);

    #[test]
    fn ranks_fall_into_the_sites_access_levels() {
        assert_eq!(Access::of(Rank::Leader), Access::Officer);
        assert_eq!(Access::of(Rank::Officer), Access::Officer);
        assert_eq!(Access::of(Rank::Raider), Access::Raider);
        assert_eq!(Access::of(Rank::Trial), Access::Raider);
        for rank in [Rank::Member, Rank::Friend, Rank::Retired] {
            assert_eq!(Access::of(rank), Access::Member);
        }
    }

    fn viewers(overwrites: &[PermissionOverwrite]) -> Vec<PermissionOverwriteType> {
        overwrites
            .iter()
            .filter(|o| o.allow.contains(Permissions::VIEW_CHANNEL))
            .map(|o| o.kind)
            .collect()
    }

    #[test]
    fn each_category_opens_to_its_level_and_those_above() {
        use PermissionOverwriteType::{Member, Role};
        let public = category_overwrites(Audience::Everyone, &ROLES, EVERYONE, BOT);
        assert_eq!(viewers(&public), vec![Role(EVERYONE)]);

        let raiders = category_overwrites(Audience::AtLeast(Access::Raider), &ROLES, EVERYONE, BOT);
        assert!(
            raiders
                .iter()
                .any(|o| o.kind == Role(EVERYONE) && o.deny.contains(Permissions::VIEW_CHANNEL))
        );
        assert_eq!(
            viewers(&raiders),
            vec![Member(BOT), Role(ROLES.officer), Role(ROLES.raider)]
        );

        let officers =
            category_overwrites(Audience::AtLeast(Access::Officer), &ROLES, EVERYONE, BOT);
        assert_eq!(viewers(&officers), vec![Member(BOT), Role(ROLES.officer)]);

        let members = category_overwrites(Audience::AtLeast(Access::Member), &ROLES, EVERYONE, BOT);
        assert_eq!(viewers(&members).len(), 4);
    }

    #[test]
    fn only_officers_post_announcements() {
        let overwrites = announcements_overwrites(&ROLES, EVERYONE);
        let everyone = overwrites
            .iter()
            .find(|o| o.kind == PermissionOverwriteType::Role(EVERYONE))
            .unwrap();
        assert!(everyone.allow.contains(Permissions::VIEW_CHANNEL));
        assert!(everyone.deny.contains(Permissions::SEND_MESSAGES));
        let officer = overwrites
            .iter()
            .find(|o| o.kind == PermissionOverwriteType::Role(ROLES.officer))
            .unwrap();
        assert!(officer.allow.contains(Permissions::SEND_MESSAGES));
    }

    #[test]
    fn role_changes_leave_exactly_the_one_role() {
        let other = RoleId::new(50);
        // A new raider gets Raider.
        assert_eq!(
            role_changes(&[other], Some(Access::Raider), &ROLES),
            (vec![ROLES.raider], vec![])
        );
        // A raider made an officer swaps Raider for Officer.
        assert_eq!(
            role_changes(&[ROLES.raider, other], Some(Access::Officer), &ROLES),
            (vec![ROLES.officer], vec![ROLES.raider])
        );
        // Already right: nothing.
        assert_eq!(
            role_changes(&[ROLES.member], Some(Access::Member), &ROLES),
            (vec![], vec![])
        );
        // Unlinked: the bot's roles go, anyone else's stay.
        assert_eq!(
            role_changes(&[ROLES.officer, ROLES.member, other], None, &ROLES),
            (vec![], vec![ROLES.officer, ROLES.member])
        );
    }
}
