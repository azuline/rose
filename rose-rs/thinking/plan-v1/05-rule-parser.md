# Milestone 5: Rule Parser

## Overview
This milestone implements the DSL (Domain Specific Language) parser for Rose's rule system. The parser converts text rules like `artist:BLACKPINK artist:='Blackpink'` into structured matchers and actions.

## Dependencies
- regex (for pattern matching and sed operations)
- Standard library only for parsing

## Grammar Overview

The rule syntax:
```
rule = matcher actions
matcher = field:pattern [boolean_ops more_matchers]
actions = field:action [more_actions]
boolean_ops = "and" | "or" | "not"
pattern = value | /regex/ | value1,value2 | "quoted value"
action = :='value' | :='' | +:'value' | /find/replace/flags | :split
```

## Implementation Guide (`src/rule_parser.rs`)

### 1. Token Types

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Field(String),
    Colon,
    Equals,
    Plus,
    Slash,
    Value(String),
    Regex(String),
    Comma,
    And,
    Or,
    Not,
    LeftParen,
    RightParen,
}
```

### 2. Tokenizer Implementation

```rust
pub fn tokenize(input: &str) -> Result<Vec<Token>> {
    todo!()
}
```

Tokenization rules:

1. **Whitespace**: Skip spaces and tabs

2. **Single character tokens**:
   - `:` -> Token::Colon
   - `=` -> Token::Equals
   - `+` -> Token::Plus
   - `,` -> Token::Comma
   - `(` -> Token::LeftParen
   - `)` -> Token::RightParen

3. **Regex patterns** (start with `/`):
   - Read until closing `/`
   - Handle escaped slashes `\/`
   - Store content without slashes as Token::Regex

4. **Quoted strings** (start with `"`):
   - Read until closing `"`
   - Handle escaped quotes `\"`
   - Store content without quotes as Token::Value

5. **Keywords and identifiers**:
   - Read alphanumeric + underscore + hyphen
   - Check if it's a keyword: "and", "or", "not"
   - If followed by `:`, it's Token::Field
   - Otherwise Token::Value

6. **Special escapes in unquoted values**:
   - `\,` -> comma in value
   - `\^` -> caret (for start anchor)
   - `\$` -> dollar (for end anchor)

### 3. AST Types

```rust
#[derive(Debug, Clone)]
pub enum Matcher {
    Tag { field: String, pattern: Pattern },
    Release(Box<Matcher>),
    Track(Box<Matcher>),
    And(Box<Matcher>, Box<Matcher>),
    Or(Box<Matcher>, Box<Matcher>),
    Not(Box<Matcher>),
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Exact(String),
    Regex(Regex),
    List(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum Action {
    Replace { field: String, value: String },
    Add { field: String, value: String },
    Delete { field: String },
    DeleteTag { field: String },
    Split { field: String, delimiter: String },
    Sed { field: String, find: Regex, replace: String, flags: SedFlags },
}

#[derive(Debug, Clone, Default)]
pub struct SedFlags {
    pub global: bool,
    pub case_insensitive: bool,
}
```

### 4. Parser Implementation

```rust
pub fn parse_rule(input: &str) -> Result<(Matcher, Vec<Action>)> {
    todo!()
}
```

Parsing steps:

1. **Tokenize input**
2. **Parse matcher** (everything before actions)
3. **Parse actions** (identified by `:=`, `:+`, etc.)
4. **Return tuple of (matcher, actions)**

### 5. Matcher Parsing

Key patterns to handle:

1. **Simple field matcher**: `artist:BLACKPINK`
   - Field + colon + value/pattern

2. **List matcher**: `artist:foo,bar`
   - Field + colon + comma-separated values

3. **Regex matcher**: `artist:/Black.*/`
   - Field + colon + regex pattern

4. **Boolean operations**: `artist:foo and title:bar`
   - Parse left matcher, operator, right matcher
   - Handle precedence (and > or)

5. **Special anchors**: `title:^Start` or `title:End$`
   - `^` at start means match beginning
   - `$` at end means match end
   - `^text$` means exact match

### 6. Action Parsing

Action patterns:

1. **Replace**: `field:='new value'`
   - Field + colon + equals + value

2. **Delete**: `field:=''`
   - Field + colon + equals + empty

3. **Delete tag**: `field:`
   - Field + colon (no value)

4. **Add**: `field+:'value'`
   - Field + plus + colon + value

5. **Split**: `field/'delimiter'`
   - Field + slash + delimiter + slash

6. **Sed**: `field/find/replace/flags`
   - Field + slash + regex + slash + replacement + slash + flags
   - Flags: g (global), i (case insensitive)

## Test Implementation Guide (`src/rule_parser_test.rs`)

### Basic Tests

#### `test_rule_str`
- Test string representation of rules
- Verify round-trip parsing

#### `test_rule_parse_matcher`
- Test various matcher patterns
- Verify AST structure

#### `test_rule_parse_action`
- Test action parsing
- Verify all action types

### End-to-End Tests

These test complete rule parsing:

1. `test_rule_parsing_end_to_end_1`
   - Input: `tracktitle:Track-delete`
   - Matcher: Tag match on "Track"
   - Action: Delete

2. `test_rule_parsing_end_to_end_2_*`
   - Test superstrict patterns with `^` and `$`
   - `^Track` - starts with
   - `Track$` - ends with
   - `^Track$` - exact match

3. `test_rule_parsing_end_to_end_3_*`
   - Test complex rules with multiple fields
   - Test multiple actions

### Edge Cases

#### `test_rule_parsing_multi_value_validation`
- Test validation of multi-value patterns
- Some contexts don't allow multiple values

#### `test_rule_parsing_defaults`
- Test default values for optional components

#### `test_parser_take`
- Test parser state management
- Ensure clean parsing

## Parsing Algorithm Details

### Tokenizer State Machine

States:
1. Normal - reading regular characters
2. InQuotes - inside quoted string
3. InRegex - inside regex pattern
4. Escaped - after backslash

### Parser Precedence

1. Parentheses (highest)
2. NOT
3. AND
4. OR (lowest)

### Error Handling

Common errors to handle:
- Unclosed quotes
- Unclosed regex
- Invalid escape sequences
- Missing field before colon
- Invalid action syntax
- Empty patterns

## Important Implementation Details

1. **Regex Compilation**: Compile regex patterns during parsing, not execution

2. **String Escaping**: Handle all escape sequences correctly

3. **Whitespace**: Be flexible with whitespace between tokens

4. **Error Messages**: Include position information in parse errors

5. **Pattern Anchors**: Convert `^` and `$` to appropriate regex

6. **Case Sensitivity**: Patterns are case-sensitive by default

## Validation Checklist

- [ ] All 12 tests pass
- [ ] Complex boolean expressions parse correctly
- [ ] All action types are supported
- [ ] Error messages are helpful
- [ ] No panic on malformed input
- [ ] Regex patterns compile successfully