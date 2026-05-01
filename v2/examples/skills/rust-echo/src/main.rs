use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("read stdin");
    let output = if input.trim().is_empty() {
        "{}"
    } else {
        input.trim()
    };
    println!("{output}");
}
