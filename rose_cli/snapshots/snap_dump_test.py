# -*- coding: utf-8 -*-
# snapshottest: v1 - https://goo.gl/zC4yUc
from __future__ import unicode_literals

from snapshottest import Snapshot


snapshots = Snapshot()

snapshots['test_dump_collage 1'] = {
    'name': 'Rose Gold',
    'releases': [
        {
            'added_at': '0000-01-01T00:00:00+00:00',
            'catalognumber': None,
            'compositiondate': None,
            'cover_image_path': None,
            'descriptors': [
                'Warm',
                'Hot'
            ],
            'disctotal': 1,
            'edition': None,
            'genres': [
                'Techno',
                'Deep House'
            ],
            'id': 'r1',
            'labels': [
                'Silk Music'
            ],
            'new': False,
            'originaldate': None,
            'parent_genres': [
                'Dance',
                'Electronic',
                'Electronic Dance Music',
                'House'
            ],
            'parent_secondary_genres': [
                'Dance',
                'Electronic',
                'Electronic Dance Music',
                'House',
                'Tech House'
            ],
            'position': 1,
            'releaseartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Techno Man'
                    },
                    {
                        'alias': False,
                        'name': 'Bass Man'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'releasedate': '2023',
            'releasetitle': 'Release 1',
            'releasetype': 'album',
            'secondary_genres': [
                'Rominimal',
                'Ambient'
            ],
            'source_path': '/dummy/r1'
        },
        {
            'added_at': '0000-01-01T00:00:00+00:00',
            'catalognumber': 'DG-001',
            'compositiondate': None,
            'cover_image_path': '/dummy/r2/cover.jpg',
            'descriptors': [
                'Wet'
            ],
            'disctotal': 1,
            'edition': 'Deluxe',
            'genres': [
                'Modern Classical'
            ],
            'id': 'r2',
            'labels': [
                'Native State'
            ],
            'new': True,
            'originaldate': '2019',
            'parent_genres': [
                'Classical Music',
                'Western Classical Music'
            ],
            'parent_secondary_genres': [
                'Classical Music',
                'Western Classical Music'
            ],
            'position': 2,
            'releaseartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                    {
                        'alias': False,
                        'name': 'Conductor Woman'
                    }
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Violin Woman'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'releasedate': '2021',
            'releasetitle': 'Release 2',
            'releasetype': 'album',
            'secondary_genres': [
                'Orchestral'
            ],
            'source_path': '/dummy/r2'
        }
    ]
}

snapshots['test_dump_collages 1'] = [
    {
        'name': 'Rose Gold',
        'releases': [
            {
                'added_at': '0000-01-01T00:00:00+00:00',
                'catalognumber': None,
                'compositiondate': None,
                'cover_image_path': None,
                'descriptors': [
                    'Warm',
                    'Hot'
                ],
                'disctotal': 1,
                'edition': None,
                'genres': [
                    'Techno',
                    'Deep House'
                ],
                'id': 'r1',
                'labels': [
                    'Silk Music'
                ],
                'new': False,
                'originaldate': None,
                'parent_genres': [
                    'Dance',
                    'Electronic',
                    'Electronic Dance Music',
                    'House'
                ],
                'parent_secondary_genres': [
                    'Dance',
                    'Electronic',
                    'Electronic Dance Music',
                    'House',
                    'Tech House'
                ],
                'position': 1,
                'releaseartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Techno Man'
                        },
                        {
                            'alias': False,
                            'name': 'Bass Man'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'releasedate': '2023',
                'releasetitle': 'Release 1',
                'releasetype': 'album',
                'secondary_genres': [
                    'Rominimal',
                    'Ambient'
                ],
                'source_path': '/dummy/r1'
            },
            {
                'added_at': '0000-01-01T00:00:00+00:00',
                'catalognumber': 'DG-001',
                'compositiondate': None,
                'cover_image_path': '/dummy/r2/cover.jpg',
                'descriptors': [
                    'Wet'
                ],
                'disctotal': 1,
                'edition': 'Deluxe',
                'genres': [
                    'Modern Classical'
                ],
                'id': 'r2',
                'labels': [
                    'Native State'
                ],
                'new': True,
                'originaldate': '2019',
                'parent_genres': [
                    'Classical Music',
                    'Western Classical Music'
                ],
                'parent_secondary_genres': [
                    'Classical Music',
                    'Western Classical Music'
                ],
                'position': 2,
                'releaseartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                        {
                            'alias': False,
                            'name': 'Conductor Woman'
                        }
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Violin Woman'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'releasedate': '2021',
                'releasetitle': 'Release 2',
                'releasetype': 'album',
                'secondary_genres': [
                    'Orchestral'
                ],
                'source_path': '/dummy/r2'
            }
        ]
    },
    {
        'name': 'Ruby Red',
        'releases': [
        ]
    }
]

snapshots['test_dump_playlist 1'] = {
    'cover_image_path': '/dummy/!playlists/Lala Lisa.jpg',
    'name': 'Lala Lisa',
    'tracks': [
        {
            'added_at': '0000-01-01T00:00:00+00:00',
            'catalognumber': None,
            'compositiondate': None,
            'descriptors': [
                'Warm',
                'Hot'
            ],
            'discnumber': '01',
            'disctotal': 1,
            'duration_seconds': 120,
            'edition': None,
            'genres': [
                'Techno',
                'Deep House'
            ],
            'id': 't1',
            'labels': [
                'Silk Music'
            ],
            'new': False,
            'originaldate': None,
            'parent_genres': [
                'Dance',
                'Electronic',
                'Electronic Dance Music',
                'House'
            ],
            'parent_secondary_genres': [
                'Dance',
                'Electronic',
                'Electronic Dance Music',
                'House',
                'Tech House'
            ],
            'position': 1,
            'release_id': 'r1',
            'releaseartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Techno Man'
                    },
                    {
                        'alias': False,
                        'name': 'Bass Man'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'releasedate': '2023',
            'releasetitle': 'Release 1',
            'releasetype': 'album',
            'secondary_genres': [
                'Rominimal',
                'Ambient'
            ],
            'source_path': '/dummy/r1/01.m4a',
            'trackartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Techno Man'
                    },
                    {
                        'alias': False,
                        'name': 'Bass Man'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'tracknumber': '01',
            'tracktitle': 'Track 1',
            'tracktotal': 2
        },
        {
            'added_at': '0000-01-01T00:00:00+00:00',
            'catalognumber': 'DG-001',
            'compositiondate': None,
            'descriptors': [
                'Wet'
            ],
            'discnumber': '01',
            'disctotal': 1,
            'duration_seconds': 120,
            'edition': 'Deluxe',
            'genres': [
                'Modern Classical'
            ],
            'id': 't3',
            'labels': [
                'Native State'
            ],
            'new': True,
            'originaldate': '2019',
            'parent_genres': [
                'Classical Music',
                'Western Classical Music'
            ],
            'parent_secondary_genres': [
                'Classical Music',
                'Western Classical Music'
            ],
            'position': 2,
            'release_id': 'r2',
            'releaseartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                    {
                        'alias': False,
                        'name': 'Conductor Woman'
                    }
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Violin Woman'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'releasedate': '2021',
            'releasetitle': 'Release 2',
            'releasetype': 'album',
            'secondary_genres': [
                'Orchestral'
            ],
            'source_path': '/dummy/r2/01.m4a',
            'trackartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                    {
                        'alias': False,
                        'name': 'Conductor Woman'
                    }
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Violin Woman'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'tracknumber': '01',
            'tracktitle': 'Track 1',
            'tracktotal': 1
        }
    ]
}

snapshots['test_dump_playlists 1'] = [
    {
        'cover_image_path': '/dummy/!playlists/Lala Lisa.jpg',
        'name': 'Lala Lisa',
        'tracks': [
            {
                'added_at': '0000-01-01T00:00:00+00:00',
                'catalognumber': None,
                'compositiondate': None,
                'descriptors': [
                    'Warm',
                    'Hot'
                ],
                'discnumber': '01',
                'disctotal': 1,
                'duration_seconds': 120,
                'edition': None,
                'genres': [
                    'Techno',
                    'Deep House'
                ],
                'id': 't1',
                'labels': [
                    'Silk Music'
                ],
                'new': False,
                'originaldate': None,
                'parent_genres': [
                    'Dance',
                    'Electronic',
                    'Electronic Dance Music',
                    'House'
                ],
                'parent_secondary_genres': [
                    'Dance',
                    'Electronic',
                    'Electronic Dance Music',
                    'House',
                    'Tech House'
                ],
                'position': 1,
                'release_id': 'r1',
                'releaseartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Techno Man'
                        },
                        {
                            'alias': False,
                            'name': 'Bass Man'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'releasedate': '2023',
                'releasetitle': 'Release 1',
                'releasetype': 'album',
                'secondary_genres': [
                    'Rominimal',
                    'Ambient'
                ],
                'source_path': '/dummy/r1/01.m4a',
                'trackartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Techno Man'
                        },
                        {
                            'alias': False,
                            'name': 'Bass Man'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'tracknumber': '01',
                'tracktitle': 'Track 1',
                'tracktotal': 2
            },
            {
                'added_at': '0000-01-01T00:00:00+00:00',
                'catalognumber': 'DG-001',
                'compositiondate': None,
                'descriptors': [
                    'Wet'
                ],
                'discnumber': '01',
                'disctotal': 1,
                'duration_seconds': 120,
                'edition': 'Deluxe',
                'genres': [
                    'Modern Classical'
                ],
                'id': 't3',
                'labels': [
                    'Native State'
                ],
                'new': True,
                'originaldate': '2019',
                'parent_genres': [
                    'Classical Music',
                    'Western Classical Music'
                ],
                'parent_secondary_genres': [
                    'Classical Music',
                    'Western Classical Music'
                ],
                'position': 2,
                'release_id': 'r2',
                'releaseartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                        {
                            'alias': False,
                            'name': 'Conductor Woman'
                        }
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Violin Woman'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'releasedate': '2021',
                'releasetitle': 'Release 2',
                'releasetype': 'album',
                'secondary_genres': [
                    'Orchestral'
                ],
                'source_path': '/dummy/r2/01.m4a',
                'trackartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                        {
                            'alias': False,
                            'name': 'Conductor Woman'
                        }
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Violin Woman'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'tracknumber': '01',
                'tracktitle': 'Track 1',
                'tracktotal': 1
            }
        ]
    },
    {
        'cover_image_path': None,
        'name': 'Turtle Rabbit',
        'tracks': [
        ]
    }
]

snapshots['test_dump_release 1'] = {
    'added_at': '0000-01-01T00:00:00+00:00',
    'catalognumber': None,
    'compositiondate': None,
    'cover_image_path': None,
    'descriptors': [
        'Warm',
        'Hot'
    ],
    'disctotal': 1,
    'edition': None,
    'genres': [
        'Techno',
        'Deep House'
    ],
    'id': 'r1',
    'labels': [
        'Silk Music'
    ],
    'new': False,
    'originaldate': None,
    'parent_genres': [
        'Dance',
        'Electronic',
        'Electronic Dance Music',
        'House'
    ],
    'parent_secondary_genres': [
        'Dance',
        'Electronic',
        'Electronic Dance Music',
        'House',
        'Tech House'
    ],
    'releaseartists': {
        'composer': [
        ],
        'conductor': [
        ],
        'djmixer': [
        ],
        'guest': [
        ],
        'main': [
            {
                'alias': False,
                'name': 'Techno Man'
            },
            {
                'alias': False,
                'name': 'Bass Man'
            }
        ],
        'producer': [
        ],
        'remixer': [
        ]
    },
    'releasedate': '2023',
    'releasetitle': 'Release 1',
    'releasetype': 'album',
    'secondary_genres': [
        'Rominimal',
        'Ambient'
    ],
    'source_path': '/dummy/r1',
    'tracks': [
        {
            'discnumber': '01',
            'duration_seconds': 120,
            'id': 't1',
            'source_path': '/dummy/r1/01.m4a',
            'trackartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Techno Man'
                    },
                    {
                        'alias': False,
                        'name': 'Bass Man'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'tracknumber': '01',
            'tracktitle': 'Track 1',
            'tracktotal': 2
        },
        {
            'discnumber': '01',
            'duration_seconds': 240,
            'id': 't2',
            'source_path': '/dummy/r1/02.m4a',
            'trackartists': {
                'composer': [
                ],
                'conductor': [
                ],
                'djmixer': [
                ],
                'guest': [
                ],
                'main': [
                    {
                        'alias': False,
                        'name': 'Techno Man'
                    },
                    {
                        'alias': False,
                        'name': 'Bass Man'
                    }
                ],
                'producer': [
                ],
                'remixer': [
                ]
            },
            'tracknumber': '02',
            'tracktitle': 'Track 2',
            'tracktotal': 2
        }
    ]
}

snapshots['test_dump_releases 1'] = [
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': None,
        'compositiondate': None,
        'cover_image_path': None,
        'descriptors': [
            'Warm',
            'Hot'
        ],
        'disctotal': 1,
        'edition': None,
        'genres': [
            'Techno',
            'Deep House'
        ],
        'id': 'r1',
        'labels': [
            'Silk Music'
        ],
        'new': False,
        'originaldate': None,
        'parent_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House'
        ],
        'parent_secondary_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House',
            'Tech House'
        ],
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2023',
        'releasetitle': 'Release 1',
        'releasetype': 'album',
        'secondary_genres': [
            'Rominimal',
            'Ambient'
        ],
        'source_path': '/dummy/r1',
        'tracks': [
            {
                'discnumber': '01',
                'duration_seconds': 120,
                'id': 't1',
                'source_path': '/dummy/r1/01.m4a',
                'trackartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Techno Man'
                        },
                        {
                            'alias': False,
                            'name': 'Bass Man'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'tracknumber': '01',
                'tracktitle': 'Track 1',
                'tracktotal': 2
            },
            {
                'discnumber': '01',
                'duration_seconds': 240,
                'id': 't2',
                'source_path': '/dummy/r1/02.m4a',
                'trackartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Techno Man'
                        },
                        {
                            'alias': False,
                            'name': 'Bass Man'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'tracknumber': '02',
                'tracktitle': 'Track 2',
                'tracktotal': 2
            }
        ]
    },
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': 'DG-001',
        'compositiondate': None,
        'cover_image_path': '/dummy/r2/cover.jpg',
        'descriptors': [
            'Wet'
        ],
        'disctotal': 1,
        'edition': 'Deluxe',
        'genres': [
            'Modern Classical'
        ],
        'id': 'r2',
        'labels': [
            'Native State'
        ],
        'new': True,
        'originaldate': '2019',
        'parent_genres': [
            'Classical Music',
            'Western Classical Music'
        ],
        'parent_secondary_genres': [
            'Classical Music',
            'Western Classical Music'
        ],
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
                {
                    'alias': False,
                    'name': 'Conductor Woman'
                }
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Violin Woman'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2021',
        'releasetitle': 'Release 2',
        'releasetype': 'album',
        'secondary_genres': [
            'Orchestral'
        ],
        'source_path': '/dummy/r2',
        'tracks': [
            {
                'discnumber': '01',
                'duration_seconds': 120,
                'id': 't3',
                'source_path': '/dummy/r2/01.m4a',
                'trackartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                        {
                            'alias': False,
                            'name': 'Conductor Woman'
                        }
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Violin Woman'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'tracknumber': '01',
                'tracktitle': 'Track 1',
                'tracktotal': 1
            }
        ]
    },
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': 'DG-002',
        'compositiondate': '1780',
        'cover_image_path': None,
        'descriptors': [
        ],
        'disctotal': 1,
        'edition': None,
        'genres': [
        ],
        'id': 'r3',
        'labels': [
        ],
        'new': False,
        'originaldate': None,
        'parent_genres': [
        ],
        'parent_secondary_genres': [
        ],
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2021-04-20',
        'releasetitle': 'Release 3',
        'releasetype': 'album',
        'secondary_genres': [
        ],
        'source_path': '/dummy/r3',
        'tracks': [
            {
                'discnumber': '01',
                'duration_seconds': 120,
                'id': 't4',
                'source_path': '/dummy/r3/01.m4a',
                'trackartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                    ],
                    'main': [
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'tracknumber': '01',
                'tracktitle': 'Track 1',
                'tracktotal': 1
            }
        ]
    }
]

snapshots['test_dump_releases_matcher 1'] = [
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': 'DG-001',
        'compositiondate': None,
        'cover_image_path': '/dummy/r2/cover.jpg',
        'descriptors': [
            'Wet'
        ],
        'disctotal': 1,
        'edition': 'Deluxe',
        'genres': [
            'Modern Classical'
        ],
        'id': 'r2',
        'labels': [
            'Native State'
        ],
        'new': True,
        'originaldate': '2019',
        'parent_genres': [
            'Classical Music',
            'Western Classical Music'
        ],
        'parent_secondary_genres': [
            'Classical Music',
            'Western Classical Music'
        ],
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
                {
                    'alias': False,
                    'name': 'Conductor Woman'
                }
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Violin Woman'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2021',
        'releasetitle': 'Release 2',
        'releasetype': 'album',
        'secondary_genres': [
            'Orchestral'
        ],
        'source_path': '/dummy/r2',
        'tracks': [
            {
                'discnumber': '01',
                'duration_seconds': 120,
                'id': 't3',
                'source_path': '/dummy/r2/01.m4a',
                'trackartists': {
                    'composer': [
                    ],
                    'conductor': [
                    ],
                    'djmixer': [
                    ],
                    'guest': [
                        {
                            'alias': False,
                            'name': 'Conductor Woman'
                        }
                    ],
                    'main': [
                        {
                            'alias': False,
                            'name': 'Violin Woman'
                        }
                    ],
                    'producer': [
                    ],
                    'remixer': [
                    ]
                },
                'tracknumber': '01',
                'tracktitle': 'Track 1',
                'tracktotal': 1
            }
        ]
    }
]

snapshots['test_dump_track 1'] = {
    'added_at': '0000-01-01T00:00:00+00:00',
    'catalognumber': None,
    'compositiondate': None,
    'descriptors': [
        'Warm',
        'Hot'
    ],
    'discnumber': '01',
    'disctotal': 1,
    'duration_seconds': 120,
    'edition': None,
    'genres': [
        'Techno',
        'Deep House'
    ],
    'id': 't1',
    'labels': [
        'Silk Music'
    ],
    'new': False,
    'originaldate': None,
    'parent_genres': [
        'Dance',
        'Electronic',
        'Electronic Dance Music',
        'House'
    ],
    'parent_secondary_genres': [
        'Dance',
        'Electronic',
        'Electronic Dance Music',
        'House',
        'Tech House'
    ],
    'release_id': 'r1',
    'releaseartists': {
        'composer': [
        ],
        'conductor': [
        ],
        'djmixer': [
        ],
        'guest': [
        ],
        'main': [
            {
                'alias': False,
                'name': 'Techno Man'
            },
            {
                'alias': False,
                'name': 'Bass Man'
            }
        ],
        'producer': [
        ],
        'remixer': [
        ]
    },
    'releasedate': '2023',
    'releasetitle': 'Release 1',
    'releasetype': 'album',
    'secondary_genres': [
        'Rominimal',
        'Ambient'
    ],
    'source_path': '/dummy/r1/01.m4a',
    'trackartists': {
        'composer': [
        ],
        'conductor': [
        ],
        'djmixer': [
        ],
        'guest': [
        ],
        'main': [
            {
                'alias': False,
                'name': 'Techno Man'
            },
            {
                'alias': False,
                'name': 'Bass Man'
            }
        ],
        'producer': [
        ],
        'remixer': [
        ]
    },
    'tracknumber': '01',
    'tracktitle': 'Track 1',
    'tracktotal': 2
}

snapshots['test_dump_tracks 1'] = [
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': None,
        'compositiondate': None,
        'descriptors': [
            'Warm',
            'Hot'
        ],
        'discnumber': '01',
        'disctotal': 1,
        'duration_seconds': 120,
        'edition': None,
        'genres': [
            'Techno',
            'Deep House'
        ],
        'id': 't1',
        'labels': [
            'Silk Music'
        ],
        'new': False,
        'originaldate': None,
        'parent_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House'
        ],
        'parent_secondary_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House',
            'Tech House'
        ],
        'release_id': 'r1',
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2023',
        'releasetitle': 'Release 1',
        'releasetype': 'album',
        'secondary_genres': [
            'Rominimal',
            'Ambient'
        ],
        'source_path': '/dummy/r1/01.m4a',
        'trackartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'tracknumber': '01',
        'tracktitle': 'Track 1',
        'tracktotal': 2
    },
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': None,
        'compositiondate': None,
        'descriptors': [
            'Warm',
            'Hot'
        ],
        'discnumber': '01',
        'disctotal': 1,
        'duration_seconds': 240,
        'edition': None,
        'genres': [
            'Techno',
            'Deep House'
        ],
        'id': 't2',
        'labels': [
            'Silk Music'
        ],
        'new': False,
        'originaldate': None,
        'parent_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House'
        ],
        'parent_secondary_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House',
            'Tech House'
        ],
        'release_id': 'r1',
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2023',
        'releasetitle': 'Release 1',
        'releasetype': 'album',
        'secondary_genres': [
            'Rominimal',
            'Ambient'
        ],
        'source_path': '/dummy/r1/02.m4a',
        'trackartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'tracknumber': '02',
        'tracktitle': 'Track 2',
        'tracktotal': 2
    },
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': 'DG-001',
        'compositiondate': None,
        'descriptors': [
            'Wet'
        ],
        'discnumber': '01',
        'disctotal': 1,
        'duration_seconds': 120,
        'edition': 'Deluxe',
        'genres': [
            'Modern Classical'
        ],
        'id': 't3',
        'labels': [
            'Native State'
        ],
        'new': True,
        'originaldate': '2019',
        'parent_genres': [
            'Classical Music',
            'Western Classical Music'
        ],
        'parent_secondary_genres': [
            'Classical Music',
            'Western Classical Music'
        ],
        'release_id': 'r2',
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
                {
                    'alias': False,
                    'name': 'Conductor Woman'
                }
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Violin Woman'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2021',
        'releasetitle': 'Release 2',
        'releasetype': 'album',
        'secondary_genres': [
            'Orchestral'
        ],
        'source_path': '/dummy/r2/01.m4a',
        'trackartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
                {
                    'alias': False,
                    'name': 'Conductor Woman'
                }
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Violin Woman'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'tracknumber': '01',
        'tracktitle': 'Track 1',
        'tracktotal': 1
    },
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': 'DG-002',
        'compositiondate': '1780',
        'descriptors': [
        ],
        'discnumber': '01',
        'disctotal': 1,
        'duration_seconds': 120,
        'edition': None,
        'genres': [
        ],
        'id': 't4',
        'labels': [
        ],
        'new': False,
        'originaldate': None,
        'parent_genres': [
        ],
        'parent_secondary_genres': [
        ],
        'release_id': 'r3',
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2021-04-20',
        'releasetitle': 'Release 3',
        'releasetype': 'album',
        'secondary_genres': [
        ],
        'source_path': '/dummy/r3/01.m4a',
        'trackartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'tracknumber': '01',
        'tracktitle': 'Track 1',
        'tracktotal': 1
    }
]

snapshots['test_dump_tracks_with_matcher 1'] = [
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': None,
        'compositiondate': None,
        'descriptors': [
            'Warm',
            'Hot'
        ],
        'discnumber': '01',
        'disctotal': 1,
        'duration_seconds': 120,
        'edition': None,
        'genres': [
            'Techno',
            'Deep House'
        ],
        'id': 't1',
        'labels': [
            'Silk Music'
        ],
        'new': False,
        'originaldate': None,
        'parent_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House'
        ],
        'parent_secondary_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House',
            'Tech House'
        ],
        'release_id': 'r1',
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2023',
        'releasetitle': 'Release 1',
        'releasetype': 'album',
        'secondary_genres': [
            'Rominimal',
            'Ambient'
        ],
        'source_path': '/dummy/r1/01.m4a',
        'trackartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'tracknumber': '01',
        'tracktitle': 'Track 1',
        'tracktotal': 2
    },
    {
        'added_at': '0000-01-01T00:00:00+00:00',
        'catalognumber': None,
        'compositiondate': None,
        'descriptors': [
            'Warm',
            'Hot'
        ],
        'discnumber': '01',
        'disctotal': 1,
        'duration_seconds': 240,
        'edition': None,
        'genres': [
            'Techno',
            'Deep House'
        ],
        'id': 't2',
        'labels': [
            'Silk Music'
        ],
        'new': False,
        'originaldate': None,
        'parent_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House'
        ],
        'parent_secondary_genres': [
            'Dance',
            'Electronic',
            'Electronic Dance Music',
            'House',
            'Tech House'
        ],
        'release_id': 'r1',
        'releaseartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'releasedate': '2023',
        'releasetitle': 'Release 1',
        'releasetype': 'album',
        'secondary_genres': [
            'Rominimal',
            'Ambient'
        ],
        'source_path': '/dummy/r1/02.m4a',
        'trackartists': {
            'composer': [
            ],
            'conductor': [
            ],
            'djmixer': [
            ],
            'guest': [
            ],
            'main': [
                {
                    'alias': False,
                    'name': 'Techno Man'
                },
                {
                    'alias': False,
                    'name': 'Bass Man'
                }
            ],
            'producer': [
            ],
            'remixer': [
            ]
        },
        'tracknumber': '02',
        'tracktitle': 'Track 2',
        'tracktotal': 2
    }
]
