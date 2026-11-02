-- Loads a keybind plan exported from bloodlegion.com (Profile, Keybinds): the spells on each
-- action bar and stance bar, and the key bindings. `/bl import` opens a box to paste it into.
-- It also saves the guild roster for the Blood Legion client (see the end).
--
-- The export is `BLK1:` and the base64 of tab-separated lines (see `exportString` in the site's
-- `src/lib/wow/keybinds.ts`):
--   C class           N name
--   R first last      action slots to clear first
--   S slot id name    a spell, placed at the highest rank known
--   M slot name       a macro by name, else an item
--   K command k1 k2   a key binding

local PREFIX = "|cffc41e3aBlood Legion|r: "

local function say(message)
	DEFAULT_CHAT_FRAME:AddMessage(PREFIX .. message)
end

local B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"

local function decode(data)
	data = data:gsub("[^%w%+/=]", "")
	local bits = data:gsub(".", function(c)
		if c == "=" then
			return ""
		end
		local n, out = B64:find(c, 1, true) - 1, ""
		for i = 6, 1, -1 do
			out = out .. (bit.band(n, 2 ^ (i - 1)) > 0 and "1" or "0")
		end
		return out
	end)
	return (bits:gsub("%d%d%d?%d?%d?%d?%d?%d?", function(byte)
		if #byte ~= 8 then
			return ""
		end
		return string.char(tonumber(byte, 2))
	end))
end

local function parse(text)
	local body = text:gsub("%s", ""):match("^BLK1:(.+)$")
	if not body then
		return nil, "That is not a Blood Legion keybind export."
	end
	local plan = { ranges = {}, actions = {}, bindings = {} }
	for line in decode(body):gmatch("[^\n]+") do
		local kind, a, b, c = strsplit("\t", line)
		if kind == "C" then
			plan.class = a
		elseif kind == "N" then
			plan.name = a
		elseif kind == "R" then
			table.insert(plan.ranges, { tonumber(a), tonumber(b) })
		elseif kind == "S" then
			table.insert(plan.actions, { slot = tonumber(a), spell = tonumber(b), name = c })
		elseif kind == "M" then
			table.insert(plan.actions, { slot = tonumber(a), name = b })
		elseif kind == "K" then
			table.insert(plan.bindings, { command = a, keys = { b or "", c or "" } })
		end
	end
	return plan
end

-- The spellbook moved to `C_SpellBook` in later clients; take whichever this one has.
local BANK = BOOKTYPE_SPELL or "spell"

local function spellbookName(index)
	if C_SpellBook and C_SpellBook.GetSpellBookItemName then
		return C_SpellBook.GetSpellBookItemName(index, Enum.SpellBookSpellBank.Player)
	end
	return (GetSpellBookItemName or GetSpellName)(index, BANK)
end

local function pickupSpellbook(index)
	if C_SpellBook and C_SpellBook.PickupSpellBookItem then
		C_SpellBook.PickupSpellBookItem(index, Enum.SpellBookSpellBank.Player)
	else
		(PickupSpellBookItem or PickupSpell)(index, BANK)
	end
end

--- The spellbook index of a spell's highest rank: ranks are listed lowest first.
local function findSpell(name)
	local found
	for tab = 1, GetNumSpellTabs() do
		local _, _, offset, count = GetSpellTabInfo(tab)
		for index = offset + 1, offset + count do
			if spellbookName(index) == name then
				found = index
			end
		end
	end
	return found
end

local function pickup(action)
	if action.spell then
		local index = findSpell(action.name)
		if index then
			pickupSpellbook(index)
		end
	else
		local macro = GetMacroIndexByName(action.name)
		if macro and macro > 0 then
			PickupMacro(macro)
		else
			PickupItem(action.name)
		end
	end
	return GetCursorInfo() ~= nil
end

-- Applies the plan, or says why not and returns false (so the import box stays open with the
-- paste in it).
local function apply(plan)
	if InCombatLockdown() then
		say("Not while in combat; try again once it ends.")
		return false
	end
	local _, class = UnitClass("player")
	if plan.class and class:lower() ~= plan.class then
		say(("That plan is for a %s, and this is a %s."):format(plan.class, class:lower()))
		return false
	end

	ClearCursor()
	for _, range in ipairs(plan.ranges) do
		for slot = range[1], range[2] do
			if HasAction(slot) then
				PickupAction(slot)
				ClearCursor()
			end
		end
	end

	local placed, missing = 0, {}
	for _, action in ipairs(plan.actions) do
		if pickup(action) then
			PlaceAction(action.slot)
			placed = placed + 1
		else
			table.insert(missing, action.name)
		end
		ClearCursor()
	end

	for _, binding in ipairs(plan.bindings) do
		for _, key in ipairs({ GetBindingKey(binding.command) }) do
			SetBinding(key)
		end
		for _, key in ipairs(binding.keys) do
			if key ~= "" then
				SetBinding(key, binding.command)
			end
		end
	end
	SaveBindings(CHARACTER_BINDINGS or 2)

	say(("Loaded %s: %d buttons placed, %d bindings set."):format(
		plan.name or "the plan",
		placed,
		#plan.bindings
	))
	if #missing > 0 then
		say("Not known (yet): " .. table.concat(missing, ", "))
	end
	return true
end

-- The import box: paste, then Import.
local frame

local function showImport()
	if not frame then
		frame = CreateFrame("Frame", "BloodLegionImport", UIParent, "BasicFrameTemplateWithInset")
		frame:SetSize(460, 260)
		frame:SetPoint("CENTER")
		frame:SetMovable(true)
		frame:EnableMouse(true)
		frame:RegisterForDrag("LeftButton")
		frame:SetScript("OnDragStart", frame.StartMoving)
		frame:SetScript("OnDragStop", frame.StopMovingOrSizing)
		tinsert(UISpecialFrames, frame:GetName())

		local title = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
		title:SetPoint("TOP", 0, -5)
		title:SetText("Blood Legion: paste a keybind export")

		local scroll = CreateFrame("ScrollFrame", nil, frame, "UIPanelScrollFrameTemplate")
		scroll:SetPoint("TOPLEFT", 12, -32)
		scroll:SetPoint("BOTTOMRIGHT", -32, 44)

		local edit = CreateFrame("EditBox", nil, scroll)
		edit:SetMultiLine(true)
		edit:SetMaxLetters(0)
		edit:SetAutoFocus(true)
		edit:SetFontObject(ChatFontNormal)
		edit:SetWidth(400)
		edit:SetScript("OnEscapePressed", function()
			frame:Hide()
		end)
		scroll:SetScrollChild(edit)
		frame.edit = edit

		local import = CreateFrame("Button", nil, frame, "UIPanelButtonTemplate")
		import:SetSize(100, 24)
		import:SetPoint("BOTTOMRIGHT", -12, 12)
		import:SetText("Import")
		import:SetScript("OnClick", function()
			local plan, err = parse(edit:GetText())
			if not plan then
				say(err)
				return
			end
			if apply(plan) then
				frame:Hide()
			end
		end)
	end
	frame.edit:SetText("")
	frame:Show()
	frame.edit:SetFocus()
end

-- The guild roster, for the site's race to 60. On login, on /reload, and whenever the game
-- refreshes the guild list, each member's level is saved in `BloodLegionDB.roster`, with the first
-- time this addon saw them at 60 (`reached60`, Unix seconds). WoW writes the file on /reload and
-- logout, and the Blood Legion client (`crates/bloodlegion-client`) sends it to the site. Members are never
-- dropped, so someone who left keeps their last level; the site only ever raises levels anyway.
--
-- Only Blood Legion's roster: WoW gives addons a guild's name, not an id, and the site checks it
-- too (`characters::roster::GUILD_NAME`).
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
SlashCmdList.BLOODLEGION = function(message)
	if message:match("^%s*import") or message:match("^%s*$") then
		showImport()
	elseif message:match("^%s*roster") then
		requestRoster()
		rosterStatus()
	else
		say("/bl import to load a keybind plan from bloodlegion.com; /bl roster for the saved roster.")
	end
end
