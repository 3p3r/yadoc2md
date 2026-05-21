use std::collections::HashMap;

fn main() {
    let mut map: HashMap<String, i32> = HashMap::new();
    map.insert("hello".to_string(), 42);
    for (key, value) in &map {
        println!("{}: {}", key, value);
    }
}
