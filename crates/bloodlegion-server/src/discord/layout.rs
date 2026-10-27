//! What the bot keeps in the server, as the site has it: a role for each rank (the leader,
//! officers, raiders, trials, members, friends), and four categories that follow the access levels,
//! each open to its level and the ones above it. The bot makes two channels besides, at the top of
//! the list in no category, where only the leader and officers post: #public-announcements, which
//! everyone reads (and the site shows), and #raider-announcements, for the raiding roster.
//!
//! The bot makes the roles but does not hand them out: the role check (`check`) lists who lacks
//! theirs. Pure: the permission entries are worked out here and applied by `sync`.

use serenity::all::{
    ChannelId, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId, UserId,
};

use crate::guild::Rank;

/// A rank, and the Discord role for it, highest access first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    Leader,
    Officer,
    Raider,
    Trial,
    Member,
    Friend,
}

impl Access {
    pub const ALL: [Access; 6] = [
        Access::Leader,
        Access::Officer,
        Access::Raider,
        Access::Trial,
        Access::Member,
        Access::Friend,
    ];

    pub fn of(rank: Rank) -> Access {
        match rank {
            Rank::Leader => Access::Leader,
            Rank::Officer => Access::Officer,
            Rank::Raider => Access::Raider,
            Rank::Trial => Access::Trial,
            Rank::Member => Access::Member,
            Rank::Friend => Access::Friend,
        }
    }

    /// The bot's key for the role (`discord_objects`).
    pub fn key(self) -> &'static str {
        match self {
            Access::Leader => "role:leader",
            Access::Officer => "role:officer",
            Access::Raider => "role:raider",
            Access::Trial => "role:trial",
            Access::Member => "role:member",
            Access::Friend => "role:friend",
        }
    }

    /// The role's name when the bot makes it; renaming it by hand afterwards is fine.
    pub fn role_name(self) -> &'static str {
        match self {
            Access::Leader => "Leader",
            Access::Officer => "Officer",
            Access::Raider => "Raider",
            Access::Trial => "Trial",
            Access::Member => "Member",
            Access::Friend => "Friend",
        }
    }
}

/// The ids of the roles, one per [`Access`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roles(pub [RoleId; 6]);

impl Roles {
    pub fn get(&self, access: Access) -> RoleId {
        self.0[access as usize]
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
        audience: Audience::AtLeast(Access::Friend),
    },
    Category {
        key: "category:raiders",
        name: "Raiders",
        audience: Audience::AtLeast(Access::Trial),
    },
    Category {
        key: "category:officers",
        name: "Officers",
        audience: Audience::AtLeast(Access::Officer),
    },
];

/// An announcements channel: at the top of the list, in no category, so it stands apart; read by
/// `audience`, posted in by the leader and officers.
pub struct Announcements {
    pub key: &'static str,
    pub name: &'static str,
    pub audience: Audience,
}

/// The public one, which the site shows on its front page. Its key is the one #announcements had,
/// so the channel made under that name is this one, its posts kept.
pub const PUBLIC_ANNOUNCEMENTS: Announcements = Announcements {
    key: "channel:announcements",
    name: "public-announcements",
    audience: Audience::Everyone,
};

pub const RAIDER_ANNOUNCEMENTS: Announcements = Announcements {
    key: "channel:raider-announcements",
    name: "raider-announcements",
    audience: Audience::AtLeast(Access::Trial),
};

pub const ANNOUNCEMENTS: [&Announcements; 2] = [&PUBLIC_ANNOUNCEMENTS, &RAIDER_ANNOUNCEMENTS];

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

/// An announcements channel's permission entries: its readers (by `audience`) read it, and only the
/// leader and officers post (or start threads). Its own, not a category's: it is in none.
pub fn announcements_overwrites(
    audience: Audience,
    roles: &Roles,
    everyone: RoleId,
    bot: UserId,
) -> Vec<PermissionOverwrite> {
    let read = Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    let post = Permissions::SEND_MESSAGES
        | Permissions::SEND_MESSAGES_IN_THREADS
        | Permissions::CREATE_PUBLIC_THREADS
        | Permissions::CREATE_PRIVATE_THREADS;
    let mut overwrites = match audience {
        Audience::Everyone => vec![role(everyone, read, post)],
        Audience::AtLeast(level) => {
            let mut overwrites = vec![
                role(everyone, Permissions::empty(), read | post),
                PermissionOverwrite {
                    allow: read,
                    deny: Permissions::empty(),
                    kind: PermissionOverwriteType::Member(bot),
                },
            ];
            overwrites.extend(
                Access::ALL
                    .into_iter()
                    .filter(|access| *access <= level)
                    .map(|access| role(roles.get(access), read, post)),
            );
            overwrites
        }
    };
    // The posters' entries come last and allow posting, overriding their reader entries' deny.
    for access in [Access::Leader, Access::Officer] {
        overwrites.retain(|o| o.kind != PermissionOverwriteType::Role(roles.get(access)));
        overwrites.push(role(roles.get(access), read | post, Permissions::empty()));
    }
    overwrites
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEADER: RoleId = RoleId::new(1);
    const OFFICER: RoleId = RoleId::new(2);
    const RAIDER: RoleId = RoleId::new(3);
    const TRIAL: RoleId = RoleId::new(4);
    const MEMBER: RoleId = RoleId::new(5);
    const FRIEND: RoleId = RoleId::new(6);
    const ROLES: Roles = Roles([LEADER, OFFICER, RAIDER, TRIAL, MEMBER, FRIEND]);
    const EVERYONE: RoleId = RoleId::new(100);
    const BOT: UserId = UserId::new(200);

    #[test]
    fn each_rank_has_its_role() {
        assert_eq!(Access::of(Rank::Leader), Access::Leader);
        assert_eq!(Access::of(Rank::Officer), Access::Officer);
        assert_eq!(Access::of(Rank::Raider), Access::Raider);
        assert_eq!(Access::of(Rank::Trial), Access::Trial);
        assert_eq!(Access::of(Rank::Member), Access::Member);
        assert_eq!(Access::of(Rank::Friend), Access::Friend);
        for (n, access) in Access::ALL.into_iter().enumerate() {
            assert_eq!(ROLES.get(access), ROLES.0[n]);
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
        let audience = |key: &str| CATEGORIES.iter().find(|c| c.key == key).unwrap().audience;

        let public = category_overwrites(audience("category:public"), &ROLES, EVERYONE, BOT);
        assert_eq!(viewers(&public), vec![Role(EVERYONE)]);

        let raiders = category_overwrites(audience("category:raiders"), &ROLES, EVERYONE, BOT);
        assert!(
            raiders
                .iter()
                .any(|o| o.kind == Role(EVERYONE) && o.deny.contains(Permissions::VIEW_CHANNEL))
        );
        assert_eq!(
            viewers(&raiders),
            vec![
                Member(BOT),
                Role(LEADER),
                Role(OFFICER),
                Role(RAIDER),
                Role(TRIAL)
            ]
        );

        let officers = category_overwrites(audience("category:officers"), &ROLES, EVERYONE, BOT);
        assert_eq!(
            viewers(&officers),
            vec![Member(BOT), Role(LEADER), Role(OFFICER)]
        );

        let members = category_overwrites(audience("category:members"), &ROLES, EVERYONE, BOT);
        assert_eq!(
            viewers(&members),
            vec![
                Member(BOT),
                Role(LEADER),
                Role(OFFICER),
                Role(RAIDER),
                Role(TRIAL),
                Role(MEMBER),
                Role(FRIEND)
            ]
        );
    }

    #[test]
    fn only_the_leader_and_officers_post_announcements() {
        let entry = |overwrites: &[PermissionOverwrite], id| {
            overwrites
                .iter()
                .find(|o| o.kind == PermissionOverwriteType::Role(id))
                .cloned()
        };
        let public = announcements_overwrites(Audience::Everyone, &ROLES, EVERYONE, BOT);
        let everyone = entry(&public, EVERYONE).unwrap();
        assert!(everyone.allow.contains(Permissions::VIEW_CHANNEL));
        assert!(everyone.deny.contains(Permissions::SEND_MESSAGES));

        let raider = announcements_overwrites(RAIDER_ANNOUNCEMENTS.audience, &ROLES, EVERYONE, BOT);
        assert!(
            entry(&raider, EVERYONE)
                .unwrap()
                .deny
                .contains(Permissions::VIEW_CHANNEL)
        );
        let trial = entry(&raider, TRIAL).unwrap();
        assert!(trial.allow.contains(Permissions::VIEW_CHANNEL));
        assert!(trial.deny.contains(Permissions::SEND_MESSAGES));
        assert_eq!(entry(&raider, MEMBER), None);

        for overwrites in [&public, &raider] {
            for poster in [LEADER, OFFICER] {
                let poster = entry(overwrites, poster).unwrap();
                assert!(poster.allow.contains(Permissions::SEND_MESSAGES));
                assert!(poster.deny.is_empty());
            }
        }
    }
}
