-- Bosses with loot recorded stay: removing them would take the loot with them.
DELETE FROM bosses b
WHERE b.zone IN ('barrow-deeps', 'hyjal-summit')
    AND b.name IN (
        'Chillhowl', 'Elder Tangleclaw', 'Khalith the Dreadspinner', 'Well of Sorrow', 'Amethrax',
        'Del''lynar Songwood', 'Ravus and Darlissa', 'Sonya Darkhallow',
        'Bandalar', 'Ancient of Decay', 'Time-Lost Battalion', 'Sylvestris Dusksong',
        'Old Gloomlurker', 'Gharalis the Abyssal', 'Kathris the Haunted', 'Anara Chillwind',
        'Elder Minderel', 'Tracker Stillwind', 'Council of Thorns', 'Nythus the Dreambound',
        'The Wild King'
    )
    AND NOT EXISTS (SELECT 1 FROM loot l WHERE l.boss_id = b.id);
