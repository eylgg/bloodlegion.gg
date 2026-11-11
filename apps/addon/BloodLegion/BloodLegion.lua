-- Saves the guild roster for the Blood Legion client, for the site's race to 60. On login, on
-- /reload, and whenever the game refreshes the guild list, each member's level and guild rank are
-- saved in `BloodLegionDB.rosters[guild]`, with the first time this addon saw them at 60
-- (`reached60`, Unix seconds). WoW writes the file on /reload and logout, and the Blood Legion
-- client (`crates/bloodlegion-client`) sends it to the site. A scan that named every member drops
-- the ones who left.
--
-- One roster per guild the account's characters are in, by name: WoW gives addons a guild's name,
-- not an id. The site takes the one named in its settings (`BL`) and turns down the rest. Each
-- member keeps its GUID (`Player-6934-005096AC`), whose last part is the character's Battle.net id
-- in hex, which the site matches characters by before their names ("Eyl Gg", as the roster shows
-- them), and its rank by name ("Officer") and number (`rankIndex`, 0 the guild master).

-- The claws (`Claws.tga`, from the site's `claws-512.png`) before the name, at the text's height.
local PREFIX = "|TInterface\\AddOns\\BloodLegion\\Claws:0|t |cffc41e3aBlood Legion|r: "

local function say(message)
	DEFAULT_CHAT_FRAME:AddMessage(PREFIX .. message)
end

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

-- The saved rosters by guild, the one an older version kept (`BloodLegionDB.roster`) among them.
local function savedRosters()
	BloodLegionDB = BloodLegionDB or {}
	local rosters = BloodLegionDB.rosters or {}
	BloodLegionDB.rosters = rosters
	local old = BloodLegionDB.roster
	if old and old.guild and not rosters[old.guild] then
		rosters[old.guild] = old
	end
	BloodLegionDB.roster = nil
	return rosters
end

local function scanRoster()
	-- The name is nil until the game has it.
	local guild = IsInGuild() and GetGuildInfo("player")
	if not guild then
		return
	end
	local rosters = savedRosters()
	local roster = rosters[guild] or { guild = guild, members = {} }
	rosters[guild] = roster
	local now = time()
	local total = GetNumGuildMembers() or 0
	local named, seen = 0, {}
	for i = 1, total do
		local name, rank, rankIndex, level, class, _, _, _, _, _, _, _, _, _, _, _, guid =
			GetGuildRosterInfo(i)
		if name then
			named = named + 1
			seen[name] = true
		end
		if name and level and level > 0 then
			local member = roster.members[name] or {}
			member.level = math.max(member.level or 0, level)
			member.class = class
			member.guid = guid or member.guid
			member.rank = rank
			member.rankIndex = rankIndex
			if member.level >= MAX_LEVEL and not member.reached60 then
				member.reached60 = now
			end
			roster.members[name] = member
		end
	end
	-- A scan that named every member, offline ones too, is the guild as it is: anyone else left.
	-- One that missed some (offline members hidden, the list still loading) only adds.
	if total > 0 and named >= total then
		for name in pairs(roster.members) do
			if not seen[name] then
				roster.members[name] = nil
			end
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
	local guild = IsInGuild() and GetGuildInfo("player")
	if not guild then
		say("Not in a guild on this character: log in on one in the guild.")
		return
	end
	local roster = savedRosters()[guild]
	if not roster or not roster.scannedAt then
		say("No " .. guild .. " roster saved yet: open the guild list or /reload.")
		return
	end
	local members, capped = 0, 0
	for _, member in pairs(roster.members) do
		members = members + 1
		if member.reached60 then
			capped = capped + 1
		end
	end
	say(("%s roster: %d members, %d at %d, last seen %s. /reload to save it for the client."):format(
		guild,
		members,
		capped,
		MAX_LEVEL,
		date("%H:%M", roster.scannedAt)
	))
end

-- `/bl debug`: what the game tells addons about the player, the guild roster, and anything
-- Forever adds, for working out its formats (two names, rulesets). Printed, and saved in
-- `BloodLegionDB.debug` (written on /reload, like the roster) to send as a file.

local function pack(...)
	return { n = select("#", ...), ... }
end

-- Every return value of `fn(...)`, as text; a missing function or an error says so.
local function call(fn, ...)
	if type(fn) ~= "function" then
		return "(no such function)"
	end
	local results = pack(pcall(fn, ...))
	if not results[1] then
		return "(error: " .. tostring(results[2]) .. ")"
	end
	local parts = {}
	for i = 2, results.n do
		local value = results[i]
		parts[#parts + 1] = type(value) == "string" and ("%q"):format(value) or tostring(value)
	end
	return #parts > 0 and table.concat(parts, ", ") or "(nothing)"
end

-- Globals, and functions in the game's C_ namespaces, whose names mention one of these.
local LOOK_FOR = { "ruleset", "surname", "lastname", "firstname", "camelot", "forever", "realm" }
local MAX_FOUND = 60

local function mentions(name)
	local lower = name:lower()
	for _, word in ipairs(LOOK_FOR) do
		if lower:find(word, 1, true) then
			return true
		end
	end
	return false
end

local function findNames()
	local found = {}
	for name, value in pairs(_G) do
		if type(name) == "string" then
			local namespace = name:sub(1, 2) == "C_" and type(value) == "table"
			if mentions(name) then
				found[#found + 1] = name .. " (" .. type(value) .. ")"
			end
			-- A namespace named for one (C_Camelot) shows everything in it.
			if namespace then
				local all = mentions(name)
				for key, inner in pairs(value) do
					if type(key) == "string" and (all or mentions(key)) then
						found[#found + 1] = name .. "." .. key .. " (" .. type(inner) .. ")"
					end
				end
			end
		end
	end
	table.sort(found)
	local total = #found
	while #found > MAX_FOUND do
		table.remove(found)
	end
	return found, total
end

local function debugInfo()
	local lines = {}
	local function add(label, value)
		lines[#lines + 1] = label .. ": " .. value
	end
	add("Build", call(GetBuildInfo))
	add("UnitName", call(UnitName, "player"))
	add("UnitFullName", call(UnitFullName, "player"))
	add("GetUnitName full", call(GetUnitName, "player", true))
	add("UnitGUID", call(UnitGUID, "player"))
	add("GetRealmName", call(GetRealmName))
	add("GetNormalizedRealmName", call(GetNormalizedRealmName))
	add("GetRealmID", call(GetRealmID))
	add("UnitRace", call(UnitRace, "player"))
	add("UnitSex", call(UnitSex, "player"))
	add("UnitClass", call(UnitClass, "player"))
	add("UnitLevel", call(UnitLevel, "player"))
	add("UnitFactionGroup", call(UnitFactionGroup, "player"))
	add("IsInGuild", call(IsInGuild))
	add("GetGuildInfo", call(GetGuildInfo, "player"))
	add("GetNumGuildMembers", call(GetNumGuildMembers))
	local count = (IsInGuild() and GetNumGuildMembers and GetNumGuildMembers()) or 0
	for i = 1, math.min(count, 3) do
		add("GetGuildRosterInfo(" .. i .. ")", call(GetGuildRosterInfo, i))
	end
	local found, total = findNames()
	add("Names mentioning " .. table.concat(LOOK_FOR, "/"), ("%d found%s"):format(
		total,
		total > #found and (", first " .. #found .. " listed") or ""
	))
	for _, name in ipairs(found) do
		lines[#lines + 1] = "  " .. name
	end
	return lines
end

local function debug()
	requestRoster()
	local lines = debugInfo()
	BloodLegionDB = BloodLegionDB or {}
	BloodLegionDB.debug = { at = time(), lines = lines }
	say("Debug (also saved for the client: /reload writes it):")
	for _, line in ipairs(lines) do
		DEFAULT_CHAT_FRAME:AddMessage(line)
	end
end

-- For the sub-addons (BloodLegion_GCD, ...), which load after this one: `say`, and
-- `commands[name] = function(rest)` to add `/bl name ...`.
BloodLegion = { say = say, commands = {} }

SLASH_BLOODLEGION1 = "/bl"
SLASH_BLOODLEGION2 = "/bloodlegion"
SlashCmdList.BLOODLEGION = function(message)
	local command, rest = (message or ""):match("^%s*(%S*)%s*(.-)%s*$")
	command = command:lower()
	if command == "debug" then
		debug()
	elseif BloodLegion.commands[command] then
		BloodLegion.commands[command](rest)
	else
		requestRoster()
		rosterStatus()
	end
end
