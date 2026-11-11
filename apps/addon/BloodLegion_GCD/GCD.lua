-- Blood Legion - GCD: what you pressed on the global cooldown, and what is queued for the next.
--
--   [older presses] [NOW, with the GCD sweeping over it] [NEXT]
--
-- NOW is the spell that started the running GCD. To its left are earlier presses, newest
-- nearest, off-GCD ones (trinkets, Heroic Strike) too; a cast that was interrupted or failed is
-- greyed. NEXT is the spell the game has queued for when the GCD (or cast) ends, from the action
-- bars: a queued button is "current", as its highlight shows. Hidden when nothing is.
--
-- There is no list of spells or GCD lengths: the game's own "Global Cooldown" spell (61304) has
-- the GCD as its cooldown, so a class's shorter GCD, or anything speeding it up, is already in
-- it. A press started the GCD if, the frame after it was sent, a GCD is running that no earlier
-- press took.
--
-- Forever has Midnight's combat restrictions: in combat a cooldown's start and duration can be
-- secret values, which addon code can't compare or do sums with, and `SetCooldown` refuses. Its
-- `isActive` stays a plain boolean. So this uses the numbers when they're plain, and otherwise
-- tells GCDs apart by watching `isActive` go off between them, and draws the sweep from the
-- game's duration object. `/bl gcd probe` shows what this client gives.
--
-- `/bl gcd` unlocks it to drag and locks it again; `/bl gcd reset|hide|show|probe`. The position
-- is saved in `BloodLegionGCDDB`.

local GCD_SPELL = 61304
local HISTORY = 4
local SIZE, SMALL, GAP = 44, 32, 4
local MAX_SLOT = 180 -- action bar slots

-- Repeating attacks are "current" the whole time they're on, never queued.
local AUTO = { [6603] = true, [75] = true, [5019] = true } -- Attack, Auto Shot, Shoot

local function isSecret(value)
	return issecretvalue ~= nil and issecretvalue(value) == true
end

local function spellInfo(id)
	if C_Spell and C_Spell.GetSpellInfo then
		local info = C_Spell.GetSpellInfo(id)
		if info then
			return info.name, info.iconID
		end
	elseif GetSpellInfo then
		local name, _, icon = GetSpellInfo(id)
		return name, icon
	end
end

-- Whether a GCD is running, and its start and duration when they aren't secret.
local function gcd()
	if C_Spell and C_Spell.GetSpellCooldown then
		local cd = C_Spell.GetSpellCooldown(GCD_SPELL)
		if not cd then
			return false
		end
		local start, duration = cd.startTime, cd.duration
		if start and duration and not isSecret(start) and not isSecret(duration) then
			return duration > 0, start, duration
		end
		return not isSecret(cd.isActive) and cd.isActive == true
	elseif GetSpellCooldown then
		local start, duration = GetSpellCooldown(GCD_SPELL)
		return (duration or 0) > 0, start, duration
	end
	return false
end

-- The frame.

local frame = CreateFrame("Frame", "BloodLegionGCDFrame", UIParent)
frame:SetSize(HISTORY * (SMALL + GAP) + SIZE + GAP + SMALL, SIZE)
frame:SetClampedToScreen(true)
frame:SetMovable(true)
frame:RegisterForDrag("LeftButton")
frame:SetScript("OnDragStart", frame.StartMoving)
frame:SetScript("OnDragStop", function(self)
	self:StopMovingOrSizing()
	local point, _, _, x, y = self:GetPoint()
	BloodLegionGCDDB.point, BloodLegionGCDDB.x, BloodLegionGCDDB.y = point, x, y
end)

-- Shown only while unlocked, so there is something to grab.
local handle = frame:CreateTexture(nil, "BACKGROUND")
handle:SetAllPoints()
handle:SetColorTexture(0.77, 0.12, 0.23, 0.35)

local function makeIcon(size)
	local icon = CreateFrame("Frame", nil, frame)
	icon:SetSize(size, size)
	icon.border = icon:CreateTexture(nil, "BACKGROUND")
	icon.border:SetPoint("TOPLEFT", -1, 1)
	icon.border:SetPoint("BOTTOMRIGHT", 1, -1)
	icon.border:SetColorTexture(0, 0, 0, 1)
	icon.texture = icon:CreateTexture(nil, "ARTWORK")
	icon.texture:SetAllPoints()
	icon.texture:SetTexCoord(0.07, 0.93, 0.07, 0.93) -- trims the stock icon border
	icon:Hide()
	return icon
end

local function label(icon, text)
	local font = icon:CreateFontString(nil, "OVERLAY", "GameFontNormalSmall")
	font:SetPoint("BOTTOM", icon, "TOP", 0, 2)
	font:SetText(text)
end

local history = {}
for i = 1, HISTORY do
	local icon = makeIcon(SMALL)
	icon:SetPoint("RIGHT", frame, "LEFT", (HISTORY - i + 1) * (SMALL + GAP) - GAP, 0)
	icon:SetAlpha(0.7)
	history[i] = icon
end

local now = makeIcon(SIZE)
now:SetPoint("LEFT", frame, "LEFT", HISTORY * (SMALL + GAP), 0)
now.cooldown = CreateFrame("Cooldown", nil, now, "CooldownFrameTemplate")
now.cooldown:SetAllPoints()
label(now, "Now")

local nextIcon = makeIcon(SMALL)
nextIcon:SetPoint("LEFT", now, "RIGHT", GAP, 0)
label(nextIcon, "Next")

-- The sweep: from the numbers when they're plain, else from the game's duration object, which
-- the cooldown frame takes even when what's in it is secret.
local function sweep(start, duration)
	if start then
		now.cooldown:SetCooldown(start, duration)
	elseif C_Spell and C_Spell.GetSpellCooldownDuration and now.cooldown.SetCooldownFromDurationObject then
		pcall(function()
			now.cooldown:SetCooldownFromDurationObject(C_Spell.GetSpellCooldownDuration(GCD_SPELL))
		end)
	end
end

-- What was pressed, newest first. `current` is the one that started the running GCD.

local presses = {}
local byCast = {} -- castGUID -> press, to grey it if it fails
local current
local claimedStart -- that GCD's start, when it isn't secret
local active = false -- whether a GCD was running when last looked

local function show(icon, press)
	if press then
		icon.texture:SetTexture(press.icon)
		icon.texture:SetDesaturated(press.failed or false)
		icon:Show()
	else
		icon:Hide()
	end
end

local function redraw()
	show(now, current)
	local i = 1
	for _, press in ipairs(presses) do
		if press ~= current and i <= HISTORY then
			show(history[i], press)
			i = i + 1
		end
	end
	for j = i, HISTORY do
		show(history[j], nil)
	end
end

-- Each frame: when the GCD ends, NOW moves into the history.
frame:SetScript("OnUpdate", function()
	local running = gcd()
	if not running and active then
		current, claimedStart = nil, nil
		redraw()
	end
	active = running
end)

local function decide(press)
	local running, start, duration = gcd()
	if not running then
		return
	end
	-- With plain numbers a new start is a new GCD, even back to back. With secret ones, a GCD is
	-- free if none was claimed since `isActive` last went off.
	local free
	if start then
		free = start ~= claimedStart
	else
		free = current == nil
	end
	if free then
		current, claimedStart, active = press, start, true
		sweep(start, duration)
	end
	redraw()
end

local function pressed(castGUID, spellID)
	if spellID == nil or isSecret(spellID) or AUTO[spellID] then
		return
	end
	if isSecret(castGUID) then
		castGUID = nil
	end
	if castGUID and byCast[castGUID] then
		return
	end
	local name, icon = spellInfo(spellID)
	if not icon then
		return
	end
	local press = { spell = spellID, name = name, icon = icon, cast = castGUID }
	table.insert(presses, 1, press)
	while #presses > HISTORY + 1 do
		local old = table.remove(presses)
		if old.cast then
			byCast[old.cast] = nil
		end
	end
	if castGUID then
		byCast[castGUID] = press
	end
	redraw()
	-- A frame later, once the client has started the GCD the press set off.
	if C_Timer and C_Timer.After then
		C_Timer.After(0, function()
			decide(press)
		end)
	else
		decide(press)
	end
end

local function failed(castGUID)
	local press = castGUID and not isSecret(castGUID) and byCast[castGUID]
	if press then
		press.failed = true
		redraw()
	end
end

-- What is queued: a non-repeating spell on the bars that is "current" but isn't the one being
-- cast right now.

local function queued()
	local casting = UnitCastingInfo("player") or UnitChannelInfo("player")
	if isSecret(casting) then
		casting = nil
	end
	for slot = 1, MAX_SLOT do
		if HasAction(slot) and IsCurrentAction(slot) and not IsAutoRepeatAction(slot) then
			local kind, id = GetActionInfo(slot)
			if kind == "macro" and GetMacroSpell then
				kind, id = "spell", GetMacroSpell(id)
			end
			if kind == "spell" and id and not isSecret(id) and not AUTO[id] then
				local name, icon = spellInfo(id)
				if icon and name ~= casting then
					return icon
				end
			end
		end
	end
end

local function redrawNext()
	local icon = queued()
	if icon then
		nextIcon.texture:SetTexture(icon)
		nextIcon:Show()
	else
		nextIcon:Hide()
	end
end

-- `/bl gcd probe`: what this client gives for the things above, and for what could come next.
-- Bind it to a macro and run it in combat too, since that's when values turn secret.

local function describe(value)
	if isSecret(value) then
		return "secret"
	end
	return tostring(value)
end

local function probe(say)
	local function has(path)
		local value = _G
		for part in path:gmatch("[^.]+") do
			value = type(value) == "table" and value[part] or nil
		end
		return path .. ": " .. (value ~= nil and type(value) or "missing")
	end
	say(("Probe (%s combat, build %s):"):format(
		InCombatLockdown() and "in" or "out of",
		tostring((GetBuildInfo()))
	))
	local lines = {}
	if C_Spell and C_Spell.GetSpellCooldown then
		local cd = C_Spell.GetSpellCooldown(GCD_SPELL)
		if cd then
			lines[#lines + 1] = ("61304 cooldown: start %s, duration %s, isActive %s, isOnGCD %s"):format(
				describe(cd.startTime),
				describe(cd.duration),
				describe(cd.isActive),
				describe(cd.isOnGCD)
			)
		else
			lines[#lines + 1] = "61304 cooldown: nil"
		end
	end
	for _, path in ipairs({
		"issecretvalue",
		"C_Spell.GetSpellCooldown",
		"C_Spell.GetSpellCooldownDuration",
		"C_Secrets.GetSpellCooldownSecrecy",
		"C_CooldownViewer",
		"IsCurrentAction",
		"GetActionInfo",
		"GetMacroSpell",
	}) do
		lines[#lines + 1] = has(path)
	end
	lines[#lines + 1] = "SetCooldownFromDurationObject: "
		.. (now.cooldown.SetCooldownFromDurationObject and "yes" or "missing")
	lines[#lines + 1] = "SpellQueueWindow: " .. tostring(GetCVar and GetCVar("SpellQueueWindow"))
	for _, line in ipairs(lines) do
		DEFAULT_CHAT_FRAME:AddMessage("  " .. line)
	end
	BloodLegionGCDDB.probe = { at = time(), combat = InCombatLockdown(), lines = lines }
end

-- Lock, position, slash command.

local function place()
	frame:ClearAllPoints()
	local db = BloodLegionGCDDB
	frame:SetPoint(db.point or "CENTER", UIParent, db.point or "CENTER", db.x or 0, db.y or -180)
end

local function lock(locked)
	BloodLegionGCDDB.locked = locked
	frame:EnableMouse(not locked)
	if locked then
		handle:Hide()
	else
		handle:Show()
	end
end

local say = BloodLegion and BloodLegion.say or print

local function command(rest)
	local db = BloodLegionGCDDB
	rest = (rest or ""):lower()
	if rest == "reset" then
		db.point, db.x, db.y = nil, nil, nil
		place()
		say("GCD bar moved back to the middle.")
	elseif rest == "hide" then
		db.hidden = true
		frame:Hide()
		say("GCD bar hidden: /bl gcd show brings it back.")
	elseif rest == "show" then
		db.hidden = false
		frame:Show()
	elseif rest == "probe" then
		probe(say)
	else
		lock(not db.locked)
		say(db.locked and "GCD bar locked." or "GCD bar unlocked: drag it, then /bl gcd to lock it.")
	end
end

if BloodLegion and BloodLegion.commands then
	BloodLegion.commands.gcd = command
end

-- Events.

local events = {
	ADDON_LOADED = function(name)
		if name ~= "BloodLegion_GCD" then
			return
		end
		BloodLegionGCDDB = BloodLegionGCDDB or { locked = true }
		place()
		lock(BloodLegionGCDDB.locked ~= false)
		if BloodLegionGCDDB.hidden then
			frame:Hide()
		end
	end,
	-- Sent is the moment of the press, so presses are in the order they were made, not the
	-- order the server answers; one it turns down fails, and is greyed. Start and succeeded
	-- catch any that sent misses (a cast is only recorded once).
	UNIT_SPELLCAST_SENT = function(unit, target, castGUID, spellID)
		pressed(castGUID, spellID)
	end,
	UNIT_SPELLCAST_START = function(unit, castGUID, spellID)
		pressed(castGUID, spellID)
		redrawNext()
	end,
	UNIT_SPELLCAST_CHANNEL_START = function(unit, castGUID, spellID)
		pressed(castGUID, spellID)
		redrawNext()
	end,
	UNIT_SPELLCAST_SUCCEEDED = function(unit, castGUID, spellID)
		pressed(castGUID, spellID)
		redrawNext()
	end,
	UNIT_SPELLCAST_INTERRUPTED = function(unit, castGUID)
		failed(castGUID)
	end,
	UNIT_SPELLCAST_FAILED = function(unit, castGUID)
		failed(castGUID)
	end,
	ACTIONBAR_UPDATE_STATE = redrawNext,
	CURRENT_SPELL_CAST_CHANGED = redrawNext,
	PLAYER_ENTERING_WORLD = function()
		redraw()
		redrawNext()
	end,
}

frame:SetScript("OnEvent", function(_, event, ...)
	events[event](...)
end)
for event in pairs(events) do
	if event:sub(1, 15) == "UNIT_SPELLCAST_" then
		frame:RegisterUnitEvent(event, "player")
	else
		frame:RegisterEvent(event)
	end
end
