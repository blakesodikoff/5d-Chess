#[allow(unused_imports)]
use chess5dlib::{game::*, parse::*};
use std::env;
use std::fs::File;
use std::io::prelude::*;
extern crate json;

// TODO: move replay, game analysis, args

fn main() -> std::io::Result<()> {
    // This is a simple example which will take the 40 most promising movesets, sort them by their score and display the 3 best movesets
    let path = env::args().last().unwrap();

    let mut file = File::open(path)?;
    let mut contents = String::new();

    file.read_to_string(&mut contents)?;
    let game = parse(&contents).unwrap();

    println!("{:#?}", game);

    Ok(())
}
