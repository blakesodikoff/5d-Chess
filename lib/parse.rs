use crate::prelude::{
    Board, Coords, Game, Layer, Physical, Piece, PieceKind, Tile, Time, TimelineInfo, Move, no_partial_game,
};
use crate::gen::{GenMoves, PiecePosition};
use crate::traversal::bubble_down_mut;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::Path;
use std::str::FromStr;

/// Represents a game state
#[derive(Debug, Deserialize)]
struct GameRaw {
    timelines: Vec<TimelineRaw>,
    width: Physical,
    height: Physical,
    active_player: bool,
    initial_board_indices: Vec<f32>,
}

/// Represents an in-game timeline
#[derive(Debug, Deserialize)]
struct TimelineRaw {
    index: f32,
    states: Vec<Vec<usize>>,
    width: u8,
    height: u8,
    begins_at: isize,
    emerges_from: Option<f32>,
}

/** Parses a JSON-encoded game state into a Game instance.
    The JSON has to be generated by [5dchess-notation](https://github.com/adri326/5dchess-notation/).
**/
pub fn parse(raw: &str) -> Option<Game> {
    let game_raw: GameRaw = serde_json::from_str(raw).ok()?;

    let even_timelines = game_raw
        .timelines
        .iter()
        .any(|tl| tl.index == -0.5 || tl.index == 0.5);
    let timelines_white: Vec<TimelineInfo> = game_raw
        .timelines
        .iter()
        .filter(|tl| tl.index >= 0.0)
        .map(|tl| {
            TimelineInfo::new(
                de_layer(tl.index, even_timelines),
                tl.emerges_from
                    .map(|l| (de_layer(l, even_timelines), tl.begins_at - 1)),
                tl.begins_at as Time + tl.states.len() as Time - 1,
                tl.begins_at as Time,
            )
        })
        .collect();
    let timelines_black: Vec<TimelineInfo> = game_raw
        .timelines
        .iter()
        .filter(|tl| tl.index < 0.0)
        .map(|tl| {
            TimelineInfo::new(
                de_layer(tl.index, even_timelines),
                tl.emerges_from
                    .map(|l| (de_layer(l, even_timelines), tl.begins_at as Time - 1)),
                tl.begins_at as Time + tl.states.len() as Time - 1,
                tl.begins_at as Time,
            )
        })
        .collect();

    let mut res = Game::new(
        game_raw.width as Physical,
        game_raw.height as Physical,
        even_timelines,
        timelines_white,
        timelines_black,
    );

    for timeline in game_raw.timelines.iter() {
        let layer: Layer = de_layer(timeline.index, even_timelines);
        for (dt, board_raw) in timeline.states.iter().enumerate() {
            let time: Time = timeline.begins_at as Time + dt as Time;

            // Note that any unknown piece will be interpreted as a blank square
            let pieces: Vec<Tile> = board_raw
                .iter()
                .map(|piece_raw| de_piece(*piece_raw))
                .collect();

            let board: Board = Board::new(
                game_raw.width,
                game_raw.height,
                layer,
                time,
                pieces,
                None,
                None,
            );
            res.insert_board(board);
        }
    }

    // Fill in the "moved" field of the pieces
    for layer in game_raw
        .initial_board_indices
        .iter()
        .map(|l| de_layer(*l, even_timelines))
    {
        let first_board = res.info.get_timeline(layer)?.first_board;
        let coords = (layer, first_board);
        let board_size = game_raw.width as usize * game_raw.height as usize;

        let mut initial_state: Vec<Tile> = Vec::new();

        for index in 0..board_size {
            let piece = res.get(Coords(
                coords.0,
                coords.1,
                (index % game_raw.width as usize) as Physical,
                (index / game_raw.width as usize) as Physical,
            ));
            let piece = match piece {
                Tile::Piece(mut p) => {
                    p.moved = false;
                    Tile::Piece(p)
                }
                Tile::Blank => Tile::Blank,
                Tile::Void => panic!("Void in board!"),
            };
            initial_state.push(piece);
        }

        let initial_board = res.get_board((layer, first_board))?.clone();

        bubble_down_mut(
            &mut res,
            coords,
            |board, _coords, (mut state, previous_board)| {
                // For each piece...
                for index in 0..board_size {
                    let x = (index % game_raw.width as usize) as Physical;
                    let y = (index / game_raw.width as usize) as Physical;

                    if cmp_pieces(board.pieces[index], state[index]) {
                        // If the piece didn't move...
                        board.pieces[index] = match board.pieces[index] {
                            Tile::Piece(mut piece) => {
                                piece.moved = state[index].piece().unwrap().moved; // Set its flag to false
                                Tile::Piece(piece)
                            }
                            x => x,
                        };
                    } else {
                        // If the piece moved...
                        board.pieces[index] = match board.pieces[index] {
                            Tile::Piece(mut piece) => {
                                piece.moved = true; // Set its flag to true

                                // If it is a pawn-like piece, fill in the en_passant field
                                if piece.can_kickstart() {
                                    if piece.white {
                                        if previous_board.get((x, y - 1)).is_empty() {
                                            board.set_en_passant(Some((x, y - 1)))
                                        }
                                    } else {
                                        if previous_board.get((x, y + 1)).is_empty() {
                                            board.set_en_passant(Some((x, y + 1)))
                                        }
                                    }
                                }

                                // If it is a king-like piece, fill in the castle field
                                if piece.can_castle() {
                                    // Left
                                    if x > 1
                                        && !cmp_pieces(
                                            previous_board.get((x - 1, y)),
                                            board.get((x - 1, y)),
                                        )
                                        && !cmp_pieces(
                                            previous_board.get((x - 2, y)),
                                            board.get((x - 2, y)),
                                        )
                                    {
                                        board.set_castle(Some((x - 1, y, x - 2, y)));
                                    } else if x < game_raw.width - 2
                                        && !cmp_pieces(
                                            previous_board.get((x + 1, y)),
                                            board.get((x + 1, y)),
                                        )
                                        && !cmp_pieces(
                                            previous_board.get((x + 2, y)),
                                            board.get((x + 2, y)),
                                        )
                                    {
                                        board.set_castle(Some((x + 1, y, x + 2, y)));
                                    }
                                }

                                Tile::Piece(piece)
                            }
                            x => x,
                        };
                        state[index] = board.pieces[index];
                    }
                }

                (state, board.clone())
            },
            (initial_state, initial_board),
        );
    }

    return Some(res);
}

/** Deserializes a layer coordinate serialized as an `f32`-encoded number. **/
fn de_layer(raw: f32, even_timelines: bool) -> Layer {
    if even_timelines && raw < 0.0 {
        (raw.ceil() - 1.0) as Layer
    } else {
        raw.floor() as Layer
    }
}

fn de_str_layer(raw: &str, even_timelines: bool) -> Result<Layer, PGNParseError> {
    if raw == "-0" {
        Ok(-1)
    } else if raw == "+0" {
        Ok(0)
    } else {
        match Layer::from_str(raw) {
            Ok(parsed) => {
                Ok(
                    if parsed < 0 && even_timelines {
                        parsed - 1
                    } else {
                        parsed
                    }
                )
            }
            Err(_) => Err(PGNParseError::InvalidL(raw.to_string()))
        }
    }
}

fn de_time(raw: &str, active_player: bool) -> Result<Time, PGNParseError> {
    match Time::from_str(raw) {
        Ok(t) => Ok((t - 1) * 2 - if active_player { 0 } else { 1 }),
        Err(_) => Err(PGNParseError::InvalidT(raw.to_string())),
    }
}

fn de_x(raw: char) -> Result<Physical, PGNParseError> {
    if raw >= 'a' && raw <= 'w' {
        Ok((raw.to_digit(33).unwrap() - 10) as Physical)
    } else {
        Err(PGNParseError::InvalidX(raw))
    }
}

fn de_y(raw: &str) -> Result<Physical, PGNParseError> {
    match Physical::from_str(raw) {
        Ok(r) => Ok(r - 1),
        Err(_) => Err(PGNParseError::InvalidY(raw.to_string())),
    }
}

fn de_pgn_piece(raw: &str) -> Result<PieceKind, PGNParseError> {
    Ok(match raw {
        "BR" | "W" => PieceKind::Brawn,
        "CK" | "C" => PieceKind::CommonKing,
        "RQ" | "Y" => PieceKind::RoyalQueen,
        "PR" | "S" => PieceKind::Princess,
        "P" => PieceKind::Pawn,
        "R" => PieceKind::Rook,
        "B" => PieceKind::Bishop,
        "U" => PieceKind::Unicorn,
        "D" => PieceKind::Dragon,
        "Q" => PieceKind::Queen,
        "K" => PieceKind::King,
        "N" => PieceKind::Knight,
        _ => return Err(PGNParseError::InvalidPiece(raw.to_string()))
    })
}

/** Deserializes a piece serialized as a `usize`-encoded number.
    This list is based on `5dchess-notation`: https://github.com/adri326/5dchess-notation/blob/master/parsers/game.js#L12
**/
pub fn de_piece(raw: usize) -> Tile {
    Tile::Piece(match raw {
        1 => Piece::new(PieceKind::Pawn, true, true),
        2 => Piece::new(PieceKind::Knight, true, true),
        3 => Piece::new(PieceKind::Bishop, true, true),
        4 => Piece::new(PieceKind::Rook, true, true),
        5 => Piece::new(PieceKind::Queen, true, true),
        6 => Piece::new(PieceKind::King, true, true),
        7 => Piece::new(PieceKind::Unicorn, true, true),
        8 => Piece::new(PieceKind::Dragon, true, true),
        9 => Piece::new(PieceKind::Princess, true, true),
        10 => Piece::new(PieceKind::Brawn, true, true),
        11 => Piece::new(PieceKind::CommonKing, true, true),
        12 => Piece::new(PieceKind::RoyalQueen, true, true),

        33 => Piece::new(PieceKind::Pawn, false, true),
        34 => Piece::new(PieceKind::Knight, false, true),
        35 => Piece::new(PieceKind::Bishop, false, true),
        36 => Piece::new(PieceKind::Rook, false, true),
        37 => Piece::new(PieceKind::Queen, false, true),
        38 => Piece::new(PieceKind::King, false, true),
        39 => Piece::new(PieceKind::Unicorn, false, true),
        40 => Piece::new(PieceKind::Dragon, false, true),
        41 => Piece::new(PieceKind::Princess, false, true),
        42 => Piece::new(PieceKind::Brawn, false, true),
        43 => Piece::new(PieceKind::CommonKing, false, true),
        44 => Piece::new(PieceKind::RoyalQueen, false, true),

        _ => return Tile::Blank,
    })
}

// Returns whether or not left.kind == right.kind and left.white == right.white
pub fn cmp_pieces(left: Tile, right: Tile) -> bool {
    match (left, right) {
        (Tile::Piece(l), Tile::Piece(r)) => l.kind == r.kind && l.white == r.white,
        (Tile::Blank, Tile::Blank) => true,
        (Tile::Void, _) | (_, Tile::Void) => panic!("Void in board!"),
        _ => false,
    }
}

/// This module should only be used for testing!
#[allow(dead_code)]
pub mod test {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    pub fn read_and_parse(path: &str) -> Game {
        let file = File::open(path).ok();
        assert!(file.is_some(), "Couldn't open `{}`!", path);
        let mut contents = String::new();

        assert!(
            file.unwrap().read_to_string(&mut contents).is_ok(),
            "Couldn't read `{}`!",
            path
        );

        let res = parse(&contents);
        assert!(res.is_some(), "Couldn't parse `{}`!", path);
        res.unwrap()
    }

    pub fn read_and_parse_opt(path: &str) -> Option<Game> {
        let mut file = File::open(path).ok()?;
        let mut contents = String::new();

        file.read_to_string(&mut contents).ok()?;

        parse(&contents)
    }
}

/// Represents an error encountered while parsing a PGN string
pub enum PGNParseError {
    InvalidHeader(String),
    FENDimensionX(String, Physical, usize),
    FENDimensionY(String, Physical, usize),
    FENUnexpected(String, String),
    InvalidVariant(String),
    InvalidPiece(String),
    SyntaxError(String, String),
    InvalidX(char),
    InvalidY(String),
    InvalidL(String),
    InvalidT(String),
    NoBoard(Layer, Time),
    Ambiguous(PieceKind, Layer, Time, Option<Physical>, Option<Physical>),
}

/// Parses a PGN string, returning the corresponding `Game` instance if possible
pub fn parse_pgn(raw: &str, variants: Option<&Path>) -> Result<Game, PGNParseError> {
    // Remove comments
    let raw = {
        let mut in_comment = false;
        raw.chars()
            .filter(|c| {
                if *c == '{' {
                    in_comment = true;
                }
                if *c == '}' {
                    in_comment = false;
                    return false;
                }
                !in_comment
            })
            .collect::<String>()
    };

    let variant_regexp = Regex::new("^[a-zA-Z \\-]+$").unwrap();
    let mut headers: HashMap<String, String> = HashMap::new();
    let mut fens = Vec::new();

    parse_headers(&raw, &mut headers, &mut fens)?;

    let (mut width, mut height) = get_dimensions(&headers);

    let mut game = Game::new(
        width,
        height,
        false,
        vec![TimelineInfo::new(0, None, 0, 0)],
        vec![],
    );

    // Custom board: parse fen
    if headers
        .get(&String::from("board"))
        .map(|x| x.to_lowercase())
        == Some("custom".to_string())
    {
        for fen in &fens {
            if fen[1] == "+0" || fen[1] == "-0" {
                game.info.even_timelines = true;
            }
        }

        for fen in fens {
            parse_and_insert_fen(fen, &mut game)?;
        }
    } else if variants.is_some()
        && headers
            .get(&String::from("board"))
            .map(|b| variant_regexp.is_match(b))
            == Some(true)
    {
        // Attempt to read the variant from the variants directory

        let variant = headers.get(&String::from("board")).unwrap();
        // Read the directory
        if let Ok(mut files) = fs::read_dir(variants.unwrap()) {
            // Find the directory of the variant
            if files
                .find(|d| {
                    if let Ok(s) = d.as_ref().map(|d| d.path()) {
                        s.file_name()
                            .map(|s| s.to_str())
                            .flatten()
                            .map(|s| s == variant)
                            .unwrap_or(false)
                    } else {
                        false
                    }
                })
                .is_some()
            {
                let mut path = variants.unwrap().join(variant);
                path.push("variant.5dpgn");
                let contents = std::fs::read_to_string(path).unwrap();
                fens = Vec::new();

                parse_headers(&contents, &mut headers, &mut fens)?;

                let (n_width, n_height) = get_dimensions(&headers);
                width = n_width;
                height = n_height;
                game = Game::new(
                    width,
                    height,
                    false,
                    vec![TimelineInfo::new(0, None, 0, 0)],
                    vec![],
                );

                for fen in &fens {
                    if fen[1] == "+0" || fen[1] == "-0" {
                        game.info.even_timelines = true;
                    }
                }

                for fen in fens {
                    parse_and_insert_fen(fen, &mut game)?;
                }

                game.info.recalculate_present();
                parse_moves(&contents, &mut game)?;
            } else {
                return Err(PGNParseError::InvalidVariant(variant.clone()));
            }
        } else {
            return Err(PGNParseError::InvalidVariant(variant.clone()));
        }
    } else {
        unimplemented!();
    }

    println!("{:#?}", headers);

    game.info.recalculate_present();
    parse_moves(&raw, &mut game)?;

    Ok(game)
}

/// Parses a single FEN string (already split by the `:` character) and returns its corresponding `Board` if the FEN is valid
pub fn parse_fen(fen: Vec<&str>, game: &Game) -> Result<Board, PGNParseError> {
    let mut board = Board::new(
        game.width,
        game.height,
        de_str_layer(fen[1], game.info.even_timelines)?,
        (Time::from_str(fen[2]).unwrap() - 1) * 2 + (fen[3] != "w") as Time,
        vec![Tile::Blank; game.width as usize * game.height as usize],
        None,
        None,
    );
    let rows = fen[0].split("/").collect::<Vec<_>>();

    if rows.len() != game.height as usize {
        return Err(PGNParseError::FENDimensionY(
            fen[0].to_string(),
            game.height,
            rows.len(),
        ));
    }

    // for each row...
    for (y, row) in rows.into_iter().enumerate() {
        let y = board.height() as usize - y - 1;
        let mut x = 0;
        let mut skip = String::new();
        // for each char...
        for c in row.chars() {
            if c >= '0' && c <= '9' {
                skip.push(c);
            } else {
                if skip.len() > 0 {
                    x += usize::from_str(&skip).unwrap();
                    skip = String::new();
                }
                let kind = match c {
                    'p' | 'P' => PieceKind::Pawn,
                    'r' | 'R' => PieceKind::Rook,
                    'b' | 'B' => PieceKind::Bishop,
                    'u' | 'U' => PieceKind::Unicorn,
                    'd' | 'D' => PieceKind::Dragon,
                    'q' | 'Q' => PieceKind::Queen,
                    's' | 'S' => PieceKind::Princess,
                    'k' | 'K' => PieceKind::King,
                    'c' | 'C' => PieceKind::CommonKing,
                    'n' | 'N' => PieceKind::Knight,
                    'w' | 'W' => PieceKind::Brawn,
                    'y' | 'Y' => PieceKind::RoyalQueen,
                    '*' => {
                        if let Tile::Piece(ref mut p) =
                            &mut board.pieces[y * game.width as usize + x - 1]
                        {
                            p.moved = false;
                        } else {
                            return Err(PGNParseError::FENUnexpected(
                                String::from("*"),
                                row.to_string(),
                            ));
                        }
                        continue;
                    }
                    x => {
                        return Err(PGNParseError::FENUnexpected(
                            String::from(x),
                            row.to_string(),
                        ))
                    }
                };
                let white = c.is_ascii_uppercase();
                if x >= game.width as usize {
                    return Err(PGNParseError::FENDimensionX(
                        row.to_string(),
                        game.width,
                        x + 1,
                    ));
                }
                let tile = Tile::Piece(Piece::new(kind, white, false));
                board.set((x as Physical, y as Physical), tile);
                x += 1;
            }
        }
        if skip.len() > 0 {
            x += usize::from_str(&skip).unwrap();
        }
        if x != game.width as usize {
            return Err(PGNParseError::FENDimensionX(row.to_string(), game.width, x));
        }
    }

    Ok(board)
}

/// Parses a FEN string using `parse_fen` and appends it to the Game.
/// As there can be no gap in the timelines, empty timelines will be created.
/// Most functions will expect timelines to be non-empty, so you should make sure that these eventually get filled in.
pub fn parse_and_insert_fen(fen: Vec<&str>, game: &mut Game) -> Result<(), PGNParseError> {
    let board = parse_fen(fen, game)?;

    if let Some(ref mut tl) = game.info.get_timeline_mut(board.l) {
        tl.last_board = tl.last_board.max(board.t);
        tl.first_board = tl.first_board.min(board.t);
    } else {
        // create the timeline
        if board.l >= 0 {
            while game.info.timelines_white.len() + 1 < board.l as usize {
                game.info.timelines_white.push(TimelineInfo::new(
                    game.info.timelines_white.len() as Layer,
                    None,
                    0,
                    0,
                ));
            }
            game.info
                .timelines_white
                .push(TimelineInfo::new(board.l, None, board.t, board.t));
        } else {
            while game.info.timelines_black.len() + 2 < (-board.l) as usize {
                game.info.timelines_black.push(TimelineInfo::new(
                    -(game.info.timelines_black.len() as Layer) - 1,
                    None,
                    0,
                    0,
                ));
            }
            game.info
                .timelines_black
                .push(TimelineInfo::new(board.l, None, board.t, board.t));
        }
    }

    // for (i, bb) in board.bitboards.white.iter().enumerate() {
    //     println!("w{:2}: {:#066b}", i, bb);
    // }
    // println!("wr:  {:#066b}", board.bitboards.white_royal);
    // println!("wm:  {:#066b}", board.bitboards.white_movable);
    // for (i, bb) in board.bitboards.black.iter().enumerate() {
    //     println!("b{:2}: {:#066b}", i, bb);
    // }
    // println!("br:  {:#066b}", board.bitboards.black_royal);
    // println!("bm:  {:#066b}", board.bitboards.black_movable);

    println!("{:?}", board);
    game.insert_board(board);
    // println!("{:#?}", game);

    Ok(())
}

pub fn parse_headers<'a>(
    raw: &'a str,
    headers: &mut HashMap<String, String>,
    fens: &mut Vec<Vec<&'a str>>,
) -> Result<(), PGNParseError> {
    let header_regexp = Regex::new("^(\\w+)\\s+\"([^\"]+)\"$").unwrap();
    for line in raw.split("\n") {
        let line = line.trim();
        if line.chars().next() == Some('[') && line.chars().last() == Some(']') {
            if let Some(cap) = header_regexp.captures(&line[1..(line.len() - 1)]) {
                let name = cap.get(1).unwrap().as_str().to_lowercase();
                let value = cap.get(2).unwrap().as_str().to_string();
                headers.insert(name, value);
            } else {
                let fen_parts = line[1..(line.len() - 1)].split(":").collect::<Vec<_>>();
                if fen_parts.len() == 4 {
                    fens.push(fen_parts);
                } else {
                    return Err(PGNParseError::InvalidHeader(line.to_string()));
                }
            }
        }
    }
    Ok(())
}

fn get_dimensions(headers: &HashMap<String, String>) -> (Physical, Physical) {
    if let Some(raw_size) = headers.get(&String::from("size")) {
        let v = raw_size.split("x").collect::<Vec<_>>();
        if v.len() == 2 {
            v[0].parse::<Physical>()
                .ok()
                .map(|x| v[1].parse::<Physical>().ok().map(|y| (x, y)))
                .flatten()
                .unwrap_or((8, 8))
        } else {
            (8, 8)
        }
    } else {
        (8, 8)
    }
}

pub fn parse_moves(raw: &str, game: &mut Game) -> Result<(), PGNParseError> {
    let raw = {
        let mut in_header = false;
        raw.chars()
            .filter(|c| {
                if *c == '[' {
                    in_header = true;
                }
                if *c == ']' {
                    in_header = false;
                    return false;
                }
                !in_header
            })
            .collect::<String>()
    };

    let mut partial_game = no_partial_game(game);

    let regex = Regex::new("[ \\t\\n]+").unwrap();
    let regex_turn = Regex::new("^(\\d+)\\.$").unwrap();
    let regex_superphysical = Regex::new(r"^\(\s*L?\s*([+-]?\d+)\s*T\s*(\d+)\s*\)").unwrap();
    let regex_piece = Regex::new(r"^(?:BR|CK|RQ|PR|[YPKNRQDUBSWC])").unwrap();
    let regex_present = Regex::new(r"^\(~T(\d+)\)$").unwrap();
    let regex_timeline = Regex::new(r"^\(>L([+\-]?\d+)\)$").unwrap();
    let regex_jump = Regex::new(r"^([a-w])(\d+)(>>?)(x)?").unwrap();
    let regex_coords = Regex::new(r"^([a-w])(\d+)").unwrap();
    let regex_promotion = Regex::new(r"^=([RBUDQSNC])?").unwrap();
    let regex_nonjump = Regex::new(r"^([a-w])?(\d+)?x?([a-w])(\d)").unwrap();
    let regex_pawn_capture = Regex::new(r"^([a-w])x([a-w])(\d+)").unwrap();

    for token in regex.split(&raw) {
        let mut token = token.trim();
        let base_token = token;
        if token == "" {
            continue;
        } else if token == "/" {
            game.info.active_player = false;
        } else if regex_turn.is_match(token) {
            game.info.active_player = true;
        } else if regex_present.is_match(token) || regex_timeline.is_match(token) {
            continue
        } else {
            let (from_l, from_t) = if let Some(caps) = regex_superphysical.captures(token) {
                token = &token[caps.get(0).unwrap().end()..];
                (
                    de_str_layer(caps.get(1).unwrap().as_str(), game.info.even_timelines)?,
                    de_time(caps.get(2).unwrap().as_str(), game.info.active_player)?,
                )
            } else {
                (0, game.info.get_timeline(0).unwrap().last_board)
            };

            // "Normal" move
            let mv = if let Some(caps) = regex_piece.captures(token) {
                token = &token[caps.get(0).unwrap().end()..];
                let piece = de_pgn_piece(caps.get(0).unwrap().as_str())?;

                // Non-spatial move
                if let Some(caps) = regex_jump.captures(token) {
                    token = &token[caps.get(0).unwrap().end()..];
                    let from_x = de_x(caps.get(1).unwrap().as_str().chars().nth(0).unwrap())?;
                    let from_y = de_y(caps.get(2).unwrap().as_str())?;
                    if let Some(caps) = regex_superphysical.captures(token) {
                        token = &token[caps.get(0).unwrap().end()..];
                        let to_l = de_str_layer(caps.get(1).unwrap().as_str(), game.info.even_timelines)?;
                        let to_t = de_time(caps.get(2).unwrap().as_str(), game.info.active_player)?;

                        if let Some(caps) = regex_coords.captures(token) {
                            token = &token[caps.get(0).unwrap().end()..];
                            let to_x = de_x(caps.get(1).unwrap().as_str().chars().nth(0).unwrap())?;
                            let to_y = de_y(caps.get(2).unwrap().as_str())?;

                            if let Some(caps) = regex_promotion.captures(token) {
                                let _promote_into = de_pgn_piece(caps.get(1).unwrap().as_str()).unwrap_or(PieceKind::Queen);
                                Move::new(game, &partial_game, Coords(from_l, from_t, from_x, from_y), Coords(to_l, to_t, to_x, to_y))
                            } else {
                                Move::new(game, &partial_game, Coords(from_l, from_t, from_x, from_y), Coords(to_l, to_t, to_x, to_y))
                            }
                        } else {
                            return Err(PGNParseError::SyntaxError(token.to_string(), base_token.to_string()));
                        }
                    } else {
                        return Err(PGNParseError::SyntaxError(token.to_string(), base_token.to_string()));
                    }
                } else if let Some(caps) = regex_nonjump.captures(token) {
                    token = &token[caps.get(0).unwrap().end()..];
                    let to_l = from_l;
                    let to_t = from_t;
                    let to_x = de_x(caps.get(3).unwrap().as_str().chars().nth(0).unwrap())?;
                    let to_y = de_y(caps.get(4).unwrap().as_str())?;
                    let from_x = match caps.get(1) {
                        Some(raw) => {
                            let raw = raw.as_str();
                            if raw != "" {
                                Some(de_x(raw.chars().nth(0).unwrap())?)
                            } else {
                                None
                            }
                        }
                        None => None
                    };
                    let from_y = match caps.get(2) {
                        Some(raw) => {
                            let raw = raw.as_str();
                            if raw != "" {
                                Some(de_y(raw)?)
                            } else {
                                None
                            }
                        }
                        None => None
                    };

                    let (from_x, from_y) = if from_x.is_none() || from_y.is_none() {
                        let board = match game.get_board((from_t, from_l)) {
                            Some(b) => b,
                            None => return Err(PGNParseError::NoBoard(from_t, from_l))
                        };

                        let tile = Tile::Piece(Piece::new(piece, game.info.active_player, false));

                        let mut candidates = board.pieces.iter().enumerate().filter(|(i, t)| {
                            let x = (i % game.width as usize) as Physical;
                            let y = (i / game.width as usize) as Physical;
                            if from_x.is_some() && Some(x) != from_x {
                                return false
                            }
                            if from_y.is_some() && Some(y) != from_y {
                                return false
                            }
                            cmp_pieces(**t, tile)
                        }).collect::<Vec<_>>();

                        if candidates.len() == 1 {
                            let x = (candidates[0].0 % game.width as usize) as Physical;
                            let y = (candidates[0].0 / game.width as usize) as Physical;
                            (x, y)
                        } else {
                            candidates.retain(|(i, t)| {
                                let x = (i % game.width as usize) as Physical;
                                let y = (i / game.width as usize) as Physical;
                                let piece_pos = PiecePosition(t.piece().unwrap(), Coords(from_l, from_t, x, y));
                                for mv in piece_pos.generate_moves(game, &partial_game).unwrap() {
                                    if mv.to.1 == Coords(from_t, from_t, to_x, to_y) {
                                        return true
                                    }
                                }
                                false
                            });

                            if candidates.len() == 1 {
                                let x = (candidates[0].0 % game.width as usize) as Physical;
                                let y = (candidates[0].0 / game.width as usize) as Physical;
                                (x, y)
                            } else {
                                return Err(PGNParseError::Ambiguous(piece, from_l, from_t, from_x, from_y))
                            }
                        }
                    } else {
                        (from_x.unwrap(), from_y.unwrap())
                    };

                    if let Some(caps) = regex_promotion.captures(token) {
                        let _promote_into = de_pgn_piece(caps.get(1).unwrap().as_str()).unwrap_or(PieceKind::Queen);
                        Move::new(game, &partial_game, Coords(from_l, from_t, from_x, from_y), Coords(to_l, to_t, to_x, to_y))
                    } else {
                        Move::new(game, &partial_game, Coords(from_l, from_t, from_x, from_y), Coords(to_l, to_t, to_x, to_y))
                    }
                } else {
                    return Err(PGNParseError::SyntaxError(token.to_string(), base_token.to_string()));
                }
            } else if let Some(caps) = regex_pawn_capture.captures(token) {
                token = &token[caps.get(0).unwrap().end()..];
                let from_x = de_x(caps.get(1).unwrap().as_str().chars().nth(0).unwrap())?;
                let to_x = de_x(caps.get(2).unwrap().as_str().chars().nth(0).unwrap())?;
                let to_y = de_y(caps.get(3).unwrap().as_str())?;
                let from_y = if game.info.active_player {
                    to_y - 1
                } else {
                    to_y + 1
                };

                if let Some(caps) = regex_promotion.captures(token) {
                    let _promote_into = de_pgn_piece(caps.get(1).unwrap().as_str()).unwrap_or(PieceKind::Queen);
                    Move::new(game, &partial_game, Coords(from_l, from_t, from_x, from_y), Coords(from_l, from_t, to_x, to_y))
                } else {
                    Move::new(game, &partial_game, Coords(from_l, from_t, from_x, from_y), Coords(from_l, from_t, to_x, to_y))
                }
            } else if let Some(caps) = regex_coords.captures(token) {
                token = &token[caps.get(0).unwrap().end()..];
                let to_x = de_x(caps.get(1).unwrap().as_str().chars().nth(0).unwrap())?;
                let to_y = de_y(caps.get(2).unwrap().as_str())?;
                let from_y = if game.info.active_player {
                    if game.get(Coords(from_l, from_t, to_x, to_y - 1)).piece().map(|p| p.kind == PieceKind::Pawn && p.white).unwrap_or(false) {
                        to_y - 1
                    } else if game.get(Coords(from_l, from_t, to_x, to_y - 2)).piece().map(|p| p.is_pawnlike() && p.can_kickstart() && !p.moved && p.white).unwrap_or(false) {
                        to_y - 2
                    } else {
                        return Err(PGNParseError::Ambiguous(PieceKind::Pawn, from_l, from_t, Some(to_x), None))
                    }
                } else {
                    if game.get(Coords(from_l, from_t, to_x, to_y + 1)).piece().map(|p| p.kind == PieceKind::Pawn && !p.white).unwrap_or(false) {
                        to_y + 1
                    } else if game.get(Coords(from_l, from_t, to_x, to_y + 2)).piece().map(|p| p.is_pawnlike() && p.can_kickstart() && !p.moved && !p.white).unwrap_or(false) {
                        to_y + 2
                    } else {
                        return Err(PGNParseError::Ambiguous(PieceKind::Pawn, from_l, from_t, Some(to_x), None))
                    }
                };

                if let Some(caps) = regex_promotion.captures(token) {
                    let _promote_into = de_pgn_piece(caps.get(1).unwrap().as_str()).unwrap_or(PieceKind::Queen);
                    Move::new(game, &partial_game, Coords(from_l, from_t, to_x, from_y), Coords(from_l, from_t, to_x, to_y))
                } else {
                    Move::new(game, &partial_game, Coords(from_l, from_t, to_x, from_y), Coords(from_l, from_t, to_x, to_y))
                }
            } else {
                return Err(PGNParseError::SyntaxError(token.to_string(), base_token.to_string()));
            };

            println!("{:?}", mv);
        }
    }

    Ok(())
}

impl fmt::Debug for PGNParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PGNParseError::InvalidHeader(s) => write!(f, "Invalid header: `{}`", s),
            PGNParseError::FENDimensionX(s, expected, got) => write!(
                f,
                "Invalid FEN width dimension in `{}`: expected {}, got {}",
                s, expected, got
            ),
            PGNParseError::FENDimensionY(s, expected, got) => write!(
                f,
                "Invalid FEN height dimension in `{}`: expected {}, got {}",
                s, expected, got
            ),
            PGNParseError::FENUnexpected(s, t) => {
                write!(f, "Invalid token in FEN: '{}' in `{}`", t, s)
            }
            PGNParseError::InvalidVariant(v) => {
                write!(f, "Invalid variant: `{}`", v)
            }
            PGNParseError::InvalidPiece(p) => {
                write!(f, "Invalid piece: `{}`", p)
            }
            PGNParseError::SyntaxError(p, s) => {
                write!(f, "Syntax error: at `{}` in `{}`", p, s)
            }
            PGNParseError::InvalidX(x) => {
                write!(f, "Invalid X coordinate: `{}`", x)
            }
            PGNParseError::InvalidY(y) => {
                write!(f, "Invalid Y coordinate: `{}`", y)
            }
            PGNParseError::InvalidT(t) => {
                write!(f, "Invalid T coordinate: `{}`", t)
            }
            PGNParseError::InvalidL(l) => {
                write!(f, "Invalid L coordinate: `{}`", l)
            }
            PGNParseError::NoBoard(l, t) => {
                write!(f, "No board currently at `{}`:`{}`!", l, t)
            }
            PGNParseError::Ambiguous(_p, l, t, x, y) => {
                write!(f, "Ambiguous on `{}`:`{}` ({},{})!", l, t, x.map(|x| format!("{}", x)).unwrap_or(String::from("?")), y.map(|y| format!("{}", y)).unwrap_or(String::from("?")))
            }
        }
    }
}
