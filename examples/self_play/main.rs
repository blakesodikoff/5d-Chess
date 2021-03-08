use chess5dlib::parse::test::read_and_parse_opt;
use chess5dlib::{
    prelude::*,
    gen::*,
    mate::*,
    eval::*,
    eval::value::PieceValues,
    tree::*,
};
// use rand::{Rng, prelude::SliceRandom};
// use std::fs::read_dir;
use std::time::{Duration, Instant};
// use std::path::Path;
// use std::borrow::Cow;

// const DEPTH: usize = 3;
const MAX_BRANCHES: usize = 2;
const MAX_TIMELINES: usize = 4;
const TIMEOUT: u64 = 15;
const POOL_SIZE: usize = 128;
const MAX_POOL_SIZE: usize = 10000;
const N_THREADS: u32 = 14;

fn main() {
    let mut game = read_and_parse_opt("tests/games/brawns-empty.json").unwrap();

    for turn in 0..60 {
        if let Some((node, value)) = iddfs_bl_schedule(&game, MAX_BRANCHES, Some(Duration::new(TIMEOUT, 0)), PieceValues::default(), POOL_SIZE, MAX_POOL_SIZE, N_THREADS) {
            let new_partial_game = {
                let partial_game = no_partial_game(&game);
                node.path[0].generate_partial_game(&game, &partial_game).expect("Couldn't generate partial game!").flatten()
            };
            new_partial_game.apply(&mut game);


            println!("{:?}", value);

            println!("turn {}: {}", turn, node.path[0]);

            if game.info.len_timelines() > MAX_TIMELINES {
                break
            }
        }
    }
}
