use lofty::prelude::*;
use lofty::tag::ItemKey;
use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <audio_file>", args[0]);
        std::process::exit(1);
    }

    let path = Path::new(&args[1]);
    let tagged_file = lofty::probe::Probe::open(path)
        .expect("Failed to open file")
        .read()
        .expect("Failed to read file");

    let tag = tagged_file.primary_tag().expect("No primary tag found");

    println!("Tag type: {:?}", tag.tag_type());
    println!("Recording Date: {:?}", tag.get_string(&ItemKey::RecordingDate));
    println!("Original Release Date: {:?}", tag.get_string(&ItemKey::OriginalReleaseDate));
    println!("Year: {:?}", tag.year());
    
    // Try various custom fields
    println!("ORIGINALDATE: {:?}", tag.get_string(&ItemKey::Unknown("ORIGINALDATE".to_string())));
    println!("originaldate: {:?}", tag.get_string(&ItemKey::Unknown("originaldate".to_string())));
    println!("COMPOSITIONDATE: {:?}", tag.get_string(&ItemKey::Unknown("COMPOSITIONDATE".to_string())));
    println!("compositiondate: {:?}", tag.get_string(&ItemKey::Unknown("compositiondate".to_string())));
    
    // List all items
    println!("\nAll items:");
    for item in tag.items() {
        println!("  {:?}: {:?}", item.key(), item.value());
    }
}