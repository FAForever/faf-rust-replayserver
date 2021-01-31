insert into game_validity
       (id, message)
values (0, 'Valid');

insert into login
       (id,    login,  email,               password,           create_time)
values (1,    'user1', 'user1@example.com', SHA2('pass1', 256), '2000-01-01 00:00:00'),
       (2,    'user2', 'user2@example.com', SHA2('pass2', 256), '2000-02-02 00:00:00'),
       (3,    'user3', 'user3@example.com', SHA2('pass3', 256), '2000-03-03 00:00:00'),
       (4,    'user4', 'user4@example.com', SHA2('pass4', 256), '2000-04-04 00:00:00'),
       (5,    'user5', 'user5@example.com', SHA2('pass5', 256), '2000-05-05 00:00:00');

insert into map
       (id, display_name, map_type,         battle_type, author)
values (11, 'SCMP_001',   'FFA',            'skirmish',  1),
       (12, 'SCMP_002',   'Astro 20 NR',    'skirmish',  2),
       (13, 'SCMP_003',   'LateNightCanis', 'skirmish',  3),
       (14, 'SCMP_004',   'FFA',            'skirmish',  4);

insert into map_version
       (id, description, max_players, width, height, version, filename,                  hidden, map_id)
values (21, 'SCMP 001',  8,           1024,  1024,   1,       'maps/scmp_001.zip',       0,      11),
       (22, 'SCMP 002',  8,           1024,  1024,   1,       'maps/scmp_002.zip',       0,      12),
       (23, NULL,        8,           1024,  1024,   1,       'maps/scmp_000.zip',       0,      14),
       (24, 'SCMP 003',  8,           1024,  1024,   1,       'maps/scmp_003.zip',       0,      13),
       (25, 'SCMP 003',  8,           512,   512,    2,       'maps/scmp_003.v0002.zip', 0,      13),
       (26, 'SCMP 003',  8,           512,   512,    3,       'maps/scmp_003.v0003.zip', 0,      13);

insert into game_featuredMods
       (id,   gamemod,       name,           description,  publish, git_url,       git_branch, file_extension, allow_override)
values (31,   'faf',         'FAF',          'FAF',        1,       'fa.git',      'a',        'nx2',          FALSE),
       (32,   'ladder1v1',   'FAF',          'Ladder',     1,       'ladder.git',  'b',        'nx2',          TRUE),
       (33,   'murderparty', 'Murder Party', 'Random mod', 1,       'example.git', 'c',        'cop',          TRUE),
       (34,   'labwars',     'Lab wars',     'No files',   1,       'example.git', 'd',        'nx2',          TRUE);

insert into updates_faf
       (id, filename,             path)
values (41, 'ForgedAlliance.exe', 'bin'),
       (42, 'effects.nx2',        'gamedata'),
       (43, 'env.nx2',            'gamedata');

insert into updates_murderparty
       (id, filename,  path)
values (44, 'foo.nx2', 'gamedata');

insert into updates_faf_files
       (id, fileId, version, name,                      md5,                                obselete) -- TODO max version logic?
values (51, 41,     3658,    'ForgedAlliance.3658.exe', '2cd7784fb131ea4955e992cfee8ca9b8', 0),
       (52, 41,     3659,    'ForgedAlliance.3659.exe', 'ee2df6c3cb80dc8258428e8fa092bce1', 0),
       (53, 42,     3658,    'effects_0.3658.nxt',      '3758baad77531dd5323c766433412e91', 0),
       (54, 42,     3659,    'effects_0.3659.nxt',      '3758baad77531dd5323c766433412e91', 0),
       (55, 43,     3656,    'env_0.3656.nxt',          '32a50729cb5155ec679771f38a151d29', 0);

insert into updates_murderparty_files
       (id, fileId, version, name,      md5,                                obselete)
values (56, 44,     1000,    'foo.nx2', '2cd7784fb131ea4955e992cfee8ca9b8', 0),
       (57, 44,     1001,    'foo.nx2', '2cd7784fb131ea4955e992cfee8ca9b8', 0),
       (58, 44,     1002,    'foo.nx2', '2cd7784fb131ea4955e992cfee8ca9b8', 0);

insert into game_stats
       (id,   startTime,             endTime,               gameName,                   gameType, gameMod, host,   mapId, validity)
values (1000, '2010-01-01 00:00:00', '2010-01-01 01:00:00', '2v2 Game',                '0',       31,      1,      21,    0),
       (1010, '2010-01-02 00:00:00', '2010-01-02 01:00:00', '1 vs AI Game',            '1',       31,      2,      22,    0),
       (1020, '2010-01-03 00:00:00', '2010-01-03 01:00:00', 'Ladder Game',             '2',       32,      3,      22,    0),
       (1030, '2010-01-03 00:00:00', '2010-01-03 01:00:00', 'Game on versioned map 1', '3',       31,      4,      24,    0),
       (1040, '2010-01-03 00:00:00', '2010-01-03 01:00:00', 'Game on versioned map 2', '3',       33,      5,      26,    0),
       (1050, '2010-01-03 00:00:00', NULL,                  'Game with no end time',   '0',       31,      1,      21,    0),
       (1060, '2010-01-03 00:00:00', NULL,                  'Game with max nulls',     '0',       31,      1,      NULL,  0),
       (1070, '2010-01-03 00:00:00', NULL,                  'Game with no players',    '0',       31,      1,      21,    0),
       (1080, '2010-01-03 00:00:00', '2010-01-03 01:00:00', 'Game with AI only',       '0',       31,      1,      21,    0),
       (1090, '2010-01-03 00:00:00', '2010-01-03 01:00:00', 'Game with no mod files',  '0',       34,      1,      21,    0);

insert into game_player_stats
       (id,   gameId, playerId, AI, faction, color, team, place, mean, deviation)  -- all not nullable
values (1001, 1000,   1,        0,  0,       1,     1,    1,     1500, 500),
       (1002, 1000,   2,        0,  1,       2,     1,    2,     1400, 400),
       (1003, 1000,   3,        0,  2,       3,     2,    3,     1300, 300),
       (1004, 1000,   4,        0,  3,       4,     2,    4,     1200, 200),
       (1011, 1010,   1,        0,  0,       1,     1,    1,     1500, 500),
       (1012, 1010,   2,        1,  1,       2,     2,    2,     1000, 0),
       (1021, 1020,   1,        0,  0,       1,     1,    1,     1500, 500),
       (1022, 1020,   2,        0,  1,       2,     2,    2,     1000, 0),
       (1031, 1030,   1,        0,  0,       1,     1,    1,     1500, 500),
       (1032, 1030,   2,        0,  1,       2,     2,    2,     1000, 0),
       (1041, 1040,   1,        0,  0,       1,     1,    1,     1500, 500),
       (1042, 1040,   2,        0,  1,       2,     2,    2,     1000, 0),
       (1051, 1050,   1,        0,  0,       1,     1,    1,     1500, 500),
       (1052, 1050,   2,        0,  1,       2,     2,    2,     1000, 0),
       (1081, 1080,   2,        1,  1,       2,     2,    2,     1000, 0);
