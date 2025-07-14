# Milestone 6: Rule Parser

## Scope
Parse Rose's DSL for metadata transformations.

## Components
- Tokenizer for rule syntax
- Parser for matchers and actions
- AST representation
- Error reporting

## Required Behaviors
- Supports quoted strings with escapes
- Regex patterns with delimiters
- Boolean operators (AND, OR, NOT)
- Actions: replace, add, delete, sed, split
- Comments starting with #
- Multi-line support with backslash

## Functions to Implement
From `rule_parser.py`:
- `rule_parser.rs:take`
- `rule_parser.rs:escape`
- `rule_parser.rs:stringify_tags`
- `rule_parser.rs:Rule::parse` (main parser method)
- `rule_parser.rs:Matcher::parse`
- `rule_parser.rs:Action::parse`
- `rule_parser.rs:Pattern::parse`

## Tests to Implement
From `rule_parser_test.py`:
- `rule_parser_test.rs:test_rule_str`
- `rule_parser_test.rs:test_rule_parse_matcher`
- `rule_parser_test.rs:test_rule_parse_action`
- `rule_parser_test.rs:test_rule_parsing_end_to_end_1`
- `rule_parser_test.rs:test_rule_parsing_end_to_end_2`
- `rule_parser_test.rs:test_rule_parsing_end_to_end_3`
- `rule_parser_test.rs:test_rule_parsing_multi_value_validation`
- `rule_parser_test.rs:test_rule_parsing_defaults`
- `rule_parser_test.rs:test_parser_take`

## Python Tests: 9 (plus parameterized tests)
## Minimum Rust Tests: 9