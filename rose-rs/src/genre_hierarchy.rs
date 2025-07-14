# TODO(CLAUDE): Load from genre_hierarchy.json at compile time in a lazy_static.
TRANSITIVE_PARENT_GENRES: dict[str, list[str]] = { }

# TODO(CLAUDE): Load from genre_hierarchy.json at compile time in a lazy_static.
TRANSITIVE_CHILD_GENRES: dict[str, list[str]] = { }

# TODO(CLAUDE): Load from genre_hierarchy.json at compile time in a lazy_static.
IMMEDIATE_CHILD_GENRES: dict[str, list[str]] = { }
