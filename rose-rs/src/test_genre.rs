use rose_rs::genre_hierarchy::get_transitive_parent_genres;

fn main() {
    println!("Electronic parents: {:?}", get_transitive_parent_genres("Electronic"));
    println!("House parents: {:?}", get_transitive_parent_genres("House"));
}