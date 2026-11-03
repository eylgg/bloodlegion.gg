-- Saves the guild roster for the Blood Legion client, for the site's race to 60. On login, on
-- /reload, and whenever the game refreshes the guild list, each member's level is saved in
-- `BloodLegionDB.roster`, with the first time this addon saw them at 60 (`reached60`, Unix
-- seconds). WoW writes the file on /reload and logout, and the Blood Legion client
-- (`crates/bloodlegion-client`) sends it to the site. Members are never dropped, so someone who
-- left keeps their last level; the site only ever raises levels anyway.
--
-- Only Blood Legion's roster: WoW gives addons a guild's name, not an id, and the site checks it
-- too (`characters::roster::GUILD_NAME`).

local PREFIX = "|cffc41e3aBlood Legion|r: "

local function say(message)
	DEFAULT_CHAT_FRAME:AddMessage(PREFIX .. message)
end

local GUILD = "Blood Legion"
local MAX_LEVEL = 60

local function requestRoster()
	if not IsInGuild() then
		return
	end
	-- The list has offline members only while the guild frame shows them.
	if SetGuildRosterShowOffline then
		SetGuildRosterShowOffline(true)
	end
	if C_GuildInfo and C_GuildInfo.GuildRoster then
		C_GuildInfo.GuildRoster()
	elseif GuildRoster then
		GuildRoster()
	end
end

local function scanRoster()
	-- The name is nil until the game has it, and another guild's roster is none of ours.
	if not IsInGuild() or GetGuildInfo("player") ~= GUILD then
		return
	end
	BloodLegionDB = BloodLegionDB or {}
	local roster = BloodLegionDB.roster
	if not roster or roster.guild ~= GUILD then
		roster = { guild = GUILD, members = {} }
		BloodLegionDB.roster = roster
	end
	local now = time()
	for i = 1, GetNumGuildMembers() do
		local name, _, _, level, class = GetGuildRosterInfo(i)
		if name and level and level > 0 then
			local member = roster.members[name] or {}
			member.level = math.max(member.level or 0, level)
			member.class = class
			if member.level >= MAX_LEVEL and not member.reached60 then
				member.reached60 = now
			end
			roster.members[name] = member
		end
	end
	roster.scannedAt = now
end

local events = CreateFrame("Frame")
events:RegisterEvent("PLAYER_ENTERING_WORLD")
events:RegisterEvent("GUILD_ROSTER_UPDATE")
events:SetScript("OnEvent", function(_, event)
	if event == "PLAYER_ENTERING_WORLD" then
		requestRoster()
	else
		scanRoster()
	end
end)

local function rosterStatus()
	local roster = BloodLegionDB and BloodLegionDB.roster
	if not roster or not roster.scannedAt then
		say("No " .. GUILD .. " roster saved yet: log in on a character in the guild.")
		return
	end
	local members, capped = 0, 0
	for _, member in pairs(roster.members) do
		members = members + 1
		if member.reached60 then
			capped = capped + 1
		end
	end
	say(("Roster: %d members, %d at %d, last seen %s. /reload to save it for the client."):format(
		members,
		capped,
		MAX_LEVEL,
		date("%H:%M", roster.scannedAt)
	))
end

SLASH_BLOODLEGION1 = "/bl"
SLASH_BLOODLEGION2 = "/bloodlegion"
SlashCmdList.BLOODLEGION = function()
	requestRoster()
	rosterStatus()
end
