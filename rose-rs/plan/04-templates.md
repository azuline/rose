# Milestone 4: Templates

## Scope
Path generation from templates with metadata substitution.

## Components
- Jinja2-compatible template engine
- Metadata field access
- Custom filters (sanitize)
- Release and track path generation

## Required Behaviors
- All metadata fields accessible in templates
- Empty fields default to "Unknown"
- Sanitize filter for filesystem safety
- Supports conditional logic
- Maintains path separators correctly
- Special handling for albumartist

## Functions to Implement
From `templates.py`:
- `templates.rs:releasetypefmt`
- `templates.rs:arrayfmt`
- `templates.rs:artistsarrayfmt`
- `templates.rs:artistsfmt`
- `templates.rs:sortorder`
- `templates.rs:lastname`
- `templates.rs:get_environment`
- `templates.rs:evaluate_release_template`
- `templates.rs:evaluate_track_template`
- `templates.rs:get_sample_music`

## Tests to Implement
From `templates_test.py`:
- `templates_test.rs:test_default_templates`
- `templates_test.rs:test_classical`

## Python Tests: 2
## Minimum Rust Tests: 2 (need more comprehensive tests)