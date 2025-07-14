use crate::rule_parser::{Matcher, Pattern, Tag};
use crate::rules::matches_pattern;

#[test]
fn test_matches_pattern_substring() {
        let pattern = Pattern::new("hello".to_string());
        
        // Basic substring match
        assert!(matches_pattern(&vec!["hello world".to_string()], &pattern, &Tag::TrackTitle).unwrap());
        assert!(matches_pattern(&vec!["say hello".to_string()], &pattern, &Tag::TrackTitle).unwrap());
        assert!(!matches_pattern(&vec!["hi world".to_string()], &pattern, &Tag::TrackTitle).unwrap());
    }
    
    #[test]
    fn test_matches_pattern_strict_start() {
        let mut pattern = Pattern::new("^hello".to_string());
        
        assert!(matches_pattern(&vec!["hello world".to_string()], &pattern, &Tag::TrackTitle).unwrap());
        assert!(!matches_pattern(&vec!["say hello".to_string()], &pattern, &Tag::TrackTitle).unwrap());
    }
    
    #[test]
    fn test_matches_pattern_strict_end() {
        let mut pattern = Pattern::new("world$".to_string());
        
        assert!(matches_pattern(&vec!["hello world".to_string()], &pattern, &Tag::TrackTitle).unwrap());
        assert!(!matches_pattern(&vec!["world hello".to_string()], &pattern, &Tag::TrackTitle).unwrap());
    }
    
    #[test]
    fn test_matches_pattern_case_insensitive() {
        let mut pattern = Pattern::new("hello".to_string());
        pattern.case_insensitive = true;
        
        assert!(matches_pattern(&vec!["HELLO world".to_string()], &pattern, &Tag::TrackTitle).unwrap());
        assert!(matches_pattern(&vec!["Hello World".to_string()], &pattern, &Tag::TrackTitle).unwrap());
    }
    
    #[test]
    fn test_matches_pattern_genre_always_case_insensitive() {
        let pattern = Pattern::new("rock".to_string());
        
        // Genre tags are always case-insensitive
        assert!(matches_pattern(&vec!["Rock".to_string()], &pattern, &Tag::Genre).unwrap());
        assert!(matches_pattern(&vec!["ROCK".to_string()], &pattern, &Tag::Genre).unwrap());
        assert!(matches_pattern(&vec!["rock".to_string()], &pattern, &Tag::Genre).unwrap());
    }
    
    #[test]
    fn test_matches_pattern_multi_value() {
        let pattern = Pattern::new("rock".to_string());
        
        // Any value matching means success for multi-value fields
        assert!(matches_pattern(
            &vec!["pop".to_string(), "rock".to_string(), "jazz".to_string()], 
            &pattern, 
            &Tag::Genre
        ).unwrap());
        
        assert!(!matches_pattern(
            &vec!["pop".to_string(), "jazz".to_string()], 
            &pattern, 
            &Tag::Genre
        ).unwrap());
    }