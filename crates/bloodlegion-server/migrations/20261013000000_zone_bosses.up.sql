/*
# Zone bosses

The bosses of the Barrow Deeps and Hyjal Summit, in encounter order, so officers need not enter
them before recording loot. Any an officer already added are kept.
*/

INSERT INTO bosses (zone, name) VALUES
    ('barrow-deeps', 'Chillhowl'),
    ('barrow-deeps', 'Elder Tangleclaw'),
    ('barrow-deeps', 'Khalith the Dreadspinner'),
    ('barrow-deeps', 'Well of Sorrow'),
    ('barrow-deeps', 'Amethrax'),
    ('barrow-deeps', 'Del''lynar Songwood'),
    ('barrow-deeps', 'Ravus and Darlissa'),
    ('barrow-deeps', 'Sonya Darkhallow'),
    ('hyjal-summit', 'Bandalar'),
    ('hyjal-summit', 'Ancient of Decay'),
    ('hyjal-summit', 'Time-Lost Battalion'),
    ('hyjal-summit', 'Sylvestris Dusksong'),
    ('hyjal-summit', 'Old Gloomlurker'),
    ('hyjal-summit', 'Gharalis the Abyssal'),
    ('hyjal-summit', 'Kathris the Haunted'),
    ('hyjal-summit', 'Anara Chillwind'),
    ('hyjal-summit', 'Elder Minderel'),
    ('hyjal-summit', 'Tracker Stillwind'),
    ('hyjal-summit', 'Council of Thorns'),
    ('hyjal-summit', 'Nythus the Dreambound'),
    ('hyjal-summit', 'The Wild King')
ON CONFLICT (zone, name) DO NOTHING;
