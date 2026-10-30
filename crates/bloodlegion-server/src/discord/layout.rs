//! What the bot keeps in the server, as the site has it: a role for each access level (the leader,
//! officers, raiders, trials, members; the public have none), and four categories that follow the levels, each open to
//! its level and the ones above it. Three channels besides, at the top of the list in no category,
//! where only the leader and officers post:
//! #public-announcements, which everyone reads (and the site shows), #member-announcements, and
//! #raider-announcements, for the raiding roster.
//!
//! The bot hands out roles only when a superuser asks (`fix`); the role check (`check`) lists who
//! lacks theirs. Pure: the permission entries are worked out here, checked by `check`, and set by
//! `fix` when a superuser asks.

use serenity::all::{
    ChannelId, PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId, UserId,
};

use crate::guild::Rank;

/// An access level, and the Discord role for it, highest first. A server once had a Friend role
/// too: the bot no longer keeps it, and leaves it to be removed by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    Leader,
    Officer,
    Raider,
    Trial,
    Member,
}

impl Access {
    /// The roles that stand together at the top, no other role between them: Officer directly
    /// under Leader.
    pub const TOGETHER: [Access; 2] = [Access::Leader, Access::Officer];

    /// Whether the level's role should have Discord's Administrator permission: the leader and the
    /// officers run the server.
    pub fn is_administrator(self) -> bool {
        matches!(self, Access::Leader | Access::Officer)
    }

    /// The role's color, as `0xRRGGBB`: the game's item qualities, and blood red for the raiders.
    pub fn color(self) -> u32 {
        match self {
            Access::Leader => 0xFF8000,  // Legendary orange
            Access::Officer => 0xA335EE, // Epic purple
            Access::Raider => 0xC41F3B,  // Blood red
            Access::Trial => 0x0070DD,   // Rare blue
            Access::Member => 0x1EFF00,  // Uncommon green
        }
    }

    pub const ALL: [Access; 5] = [
        Access::Leader,
        Access::Officer,
        Access::Raider,
        Access::Trial,
        Access::Member,
    ];

    /// The level a rank has; none for the public, who see what everyone does (`@everyone`).
    pub fn of(rank: Rank) -> Option<Access> {
        match rank {
            Rank::Leader => Some(Access::Leader),
            Rank::Officer => Some(Access::Officer),
            Rank::Raider => Some(Access::Raider),
            Rank::Trial => Some(Access::Trial),
            Rank::Member => Some(Access::Member),
            Rank::Public => None,
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
        }
    }
}

/// The ids of the roles, one per [`Access`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roles(pub [RoleId; 5]);

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

/// The bot's categories, top to bottom, the most closed first; made in this order, so they line up
/// that way in Discord.
pub const CATEGORIES: [Category; 4] = [
    Category {
        key: "category:officers",
        name: "Officers",
        audience: Audience::AtLeast(Access::Officer),
    },
    Category {
        key: "category:raiders",
        name: "Raiders",
        audience: Audience::AtLeast(Access::Trial),
    },
    Category {
        key: "category:members",
        name: "Members",
        audience: Audience::AtLeast(Access::Member),
    },
    Category {
        key: "category:public",
        name: "Public",
        audience: Audience::Everyone,
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

pub const MEMBER_ANNOUNCEMENTS: Announcements = Announcements {
    key: "channel:member-announcements",
    name: "member-announcements",
    audience: Audience::AtLeast(Access::Member),
};

pub const RAIDER_ANNOUNCEMENTS: Announcements = Announcements {
    key: "channel:raider-announcements",
    name: "raider-announcements",
    audience: Audience::AtLeast(Access::Trial),
};

/// The announcements channels, top to bottom, as they stand at the top of the list: the most closed
/// first, as the categories go.
pub const ANNOUNCEMENTS: [&Announcements; 3] = [
    &RAIDER_ANNOUNCEMENTS,
    &MEMBER_ANNOUNCEMENTS,
    &PUBLIC_ANNOUNCEMENTS,
];

/// Where everything the bot keeps is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// #public-announcements; `None` until it is made.
    pub announcements: Option<ChannelId>,
}

fn role(id: RoleId, allow: Permissions, deny: Permissions) -> PermissionOverwrite {
    PermissionOverwrite {
        allow,
        deny,
        kind: PermissionOverwriteType::Role(id),
    }
}

/// The permission entries the bot keeps on one of its categories. It sets only these, leaving any
/// others someone adds by hand. `everyone` is the server's @everyone role (its id is the server's).
/// None is for the bot itself: its role's Administrator permission lets it see and manage the
/// closed categories, as it does every channel.
pub fn category_overwrites(
    audience: Audience,
    roles: &Roles,
    everyone: RoleId,
) -> Vec<PermissionOverwrite> {
    let view = Permissions::VIEW_CHANNEL;
    match audience {
        Audience::Everyone => vec![role(everyone, view, Permissions::empty())],
        Audience::AtLeast(level) => {
            let mut overwrites = vec![role(everyone, Permissions::empty(), view)];
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

/// Those of `wanted` (the bot's entries for a channel) that `actual` (the channel's entries now)
/// lacks or has otherwise: each is the bot's for one role or member, which it sets whole.
pub fn differing<'a>(
    wanted: &'a [PermissionOverwrite],
    actual: &[PermissionOverwrite],
) -> Vec<&'a PermissionOverwrite> {
    wanted
        .iter()
        .filter(|entry| {
            !actual.iter().any(|now| {
                now.kind == entry.kind && now.allow == entry.allow && now.deny == entry.deny
            })
        })
        .collect()
}

/// Whether `actual` (a channel's entries) has one for the bot itself, which it once kept on its
/// closed channels to see them; its role's Administrator permission does that now, so the entry
/// goes (`fix::fix_permissions`).
pub fn has_own_entry(actual: &[PermissionOverwrite], bot: UserId) -> bool {
    actual
        .iter()
        .any(|entry| entry.kind == PermissionOverwriteType::Member(bot))
}

/// An announcements channel's permission entries: its readers (by `audience`) read it, and only the
/// leader and officers post (or start threads). Its own, not a category's: it is in none.
pub fn announcements_overwrites(
    audience: Audience,
    roles: &Roles,
    everyone: RoleId,
) -> Vec<PermissionOverwrite> {
    let read = Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    let post = Permissions::SEND_MESSAGES
        | Permissions::SEND_MESSAGES_IN_THREADS
        | Permissions::CREATE_PUBLIC_THREADS
        | Permissions::CREATE_PRIVATE_THREADS;
    let mut overwrites = match audience {
        Audience::Everyone => vec![role(everyone, read, post)],
        Audience::AtLeast(level) => {
            let mut overwrites = vec![role(everyone, Permissions::empty(), read | post)];
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
    const ROLES: Roles = Roles([LEADER, OFFICER, RAIDER, TRIAL, MEMBER]);
    const EVERYONE: RoleId = RoleId::new(100);
    const BOT: UserId = UserId::new(200);

    #[test]
    fn each_rank_has_its_role() {
        assert_eq!(Access::of(Rank::Leader), Some(Access::Leader));
        assert_eq!(Access::of(Rank::Officer), Some(Access::Officer));
        assert_eq!(Access::of(Rank::Raider), Some(Access::Raider));
        assert_eq!(Access::of(Rank::Trial), Some(Access::Trial));
        assert_eq!(Access::of(Rank::Member), Some(Access::Member));
        assert_eq!(Access::of(Rank::Public), None);
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
        use PermissionOverwriteType::Role;
        let audience = |key: &str| CATEGORIES.iter().find(|c| c.key == key).unwrap().audience;

        let public = category_overwrites(audience("category:public"), &ROLES, EVERYONE);
        assert_eq!(viewers(&public), vec![Role(EVERYONE)]);

        let raiders = category_overwrites(audience("category:raiders"), &ROLES, EVERYONE);
        assert!(
            raiders
                .iter()
                .any(|o| o.kind == Role(EVERYONE) && o.deny.contains(Permissions::VIEW_CHANNEL))
        );
        assert_eq!(
            viewers(&raiders),
            vec![Role(LEADER), Role(OFFICER), Role(RAIDER), Role(TRIAL)]
        );

        let officers = category_overwrites(audience("category:officers"), &ROLES, EVERYONE);
        assert_eq!(viewers(&officers), vec![Role(LEADER), Role(OFFICER)]);

        let members = category_overwrites(audience("category:members"), &ROLES, EVERYONE);
        assert_eq!(
            viewers(&members),
            vec![
                Role(LEADER),
                Role(OFFICER),
                Role(RAIDER),
                Role(TRIAL),
                Role(MEMBER)
            ]
        );
    }

    #[test]
    fn finds_the_entries_that_differ() {
        let public = category_overwrites(Audience::Everyone, &ROLES, EVERYONE);
        assert!(differing(&public, &public).is_empty());
        assert_eq!(differing(&public, &[]).len(), 1);
        // @everyone's entry there, but denying the view instead.
        let wrong = role(EVERYONE, Permissions::empty(), Permissions::VIEW_CHANNEL);
        assert_eq!(differing(&public, &[wrong]), vec![&public[0]]);
        // An entry of someone else's is no matter.
        let extra = role(
            RoleId::new(42),
            Permissions::VIEW_CHANNEL,
            Permissions::empty(),
        );
        assert!(differing(&public, &[public[0].clone(), extra]).is_empty());
    }

    #[test]
    fn the_bot_keeps_no_entry_for_itself() {
        for category in &CATEGORIES {
            let entries = category_overwrites(category.audience, &ROLES, EVERYONE);
            assert!(!has_own_entry(&entries, BOT), "{}", category.name);
        }
        for channel in ANNOUNCEMENTS {
            let entries = announcements_overwrites(channel.audience, &ROLES, EVERYONE);
            assert!(!has_own_entry(&entries, BOT), "{}", channel.name);
        }
        let old = PermissionOverwrite {
            allow: Permissions::VIEW_CHANNEL,
            deny: Permissions::empty(),
            kind: PermissionOverwriteType::Member(BOT),
        };
        assert!(has_own_entry(&[old], BOT));
    }

    #[test]
    fn only_the_leader_and_officers_post_announcements() {
        let entry = |overwrites: &[PermissionOverwrite], id| {
            overwrites
                .iter()
                .find(|o| o.kind == PermissionOverwriteType::Role(id))
                .cloned()
        };
        let public = announcements_overwrites(Audience::Everyone, &ROLES, EVERYONE);
        let everyone = entry(&public, EVERYONE).unwrap();
        assert!(everyone.allow.contains(Permissions::VIEW_CHANNEL));
        assert!(everyone.deny.contains(Permissions::SEND_MESSAGES));

        let raider = announcements_overwrites(RAIDER_ANNOUNCEMENTS.audience, &ROLES, EVERYONE);
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

        let member = announcements_overwrites(MEMBER_ANNOUNCEMENTS.audience, &ROLES, EVERYONE);
        assert!(
            entry(&member, MEMBER)
                .unwrap()
                .allow
                .contains(Permissions::VIEW_CHANNEL)
        );

        for overwrites in [&public, &member, &raider] {
            for poster in [LEADER, OFFICER] {
                let poster = entry(overwrites, poster).unwrap();
                assert!(poster.allow.contains(Permissions::SEND_MESSAGES));
                assert!(poster.deny.is_empty());
            }
        }
    }
}
