use super::*;

/** A combination of a piece and its coordinates, used to generate a piece's moves.
    This structure implements the `GenMoves` trait, and thus lets you generate moves using a `PiecePosition` instance,
    a `Game` state and a `PartialGame` state.

    ## Example

    ```
    let position = Coords(0, 0, 1, 0);
    let piece = PiecePosition::new(game.get(position).piece().unwrap(), position);

    // This loop will now print all of the moves that the `c1`-knight can make
    for mv in piece.generate_moves(game, &no_partial_game(game)).unwrap() {
        println!("{:?}", mv);
    }
    ```
**/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PiecePosition(Piece, Coords);

impl PiecePosition {
    pub fn new(piece: Piece, coords: Coords) -> PiecePosition {
        PiecePosition(piece, coords)
    }
}

/** An iterator that yields the movements of pawn-like pieces (ie. pawns and brawns), including en-passant.
    It is created by `PiecePosition::generate_moves`.
**/
pub struct PawnIter {
    moves: Vec<Move>,
    state: usize,
}

fn forward(a: Coords, b: Coords, color: bool) -> Coords {
    if color {
        a + b
    } else {
        a - b
    }
}

impl PawnIter {
    /** Creates a new PawnIter; unless you are implementing a new fairy piece, you should use `PiecePosition::generate_moves` **/
    pub fn new<'a, B: Clone + AsRef<Board> + 'a>(
        piece: PiecePosition,
        game: &'a Game,
        partial_game: &'a PartialGame<'a, B>,
    ) -> Option<Self> {
        let mut moves: Vec<Move> = Vec::new();

        // Forward move
        for perm in vec![Coords(0, 0, 0, 1), Coords(1, 0, 0, 0)] {
            if partial_game
                .get_with_game(game, forward(piece.1, perm, piece.0.white))
                .is_blank()
            {
                moves.push(Move::new(
                    game,
                    partial_game,
                    piece.1,
                    forward(piece.1, perm, piece.0.white),
                )?);

                // Kickstart move
                if !piece.0.moved
                    && partial_game
                        .get_with_game(game, forward(piece.1, perm * 2, piece.0.white))
                        .is_blank()
                {
                    moves.push(Move::new(
                        game,
                        partial_game,
                        piece.1,
                        forward(piece.1, perm * 2, piece.0.white),
                    )?);
                }
            }
        }

        // Captures
        for perm in if piece.0.kind == PieceKind::Brawn {
            vec![
                // Brawn's capture moveset
                Coords(0, 0, 1, 1),
                Coords(0, 0, -1, 1),
                Coords(0, -1, 0, 1),
                Coords(1, 0, 0, 1),
                Coords(1, 1, 0, 0),
                Coords(1, -1, 0, 0),
                Coords(1, 0, 1, 0),
                Coords(1, 0, -1, 0),
            ]
        } else {
            vec![
                // Pawn's capture moveset
                Coords(0, 0, 1, 1),
                Coords(0, 0, -1, 1),
                Coords(1, 1, 0, 0),
                Coords(1, -1, 0, 0),
            ]
        } {
            if partial_game
                .get_with_game(game, forward(piece.1, perm, piece.0.white))
                .is_piece_of_color(!piece.0.white)
            {
                moves.push(Move::new(
                    game,
                    partial_game,
                    piece.1,
                    forward(piece.1, perm, piece.0.white),
                )?);
            }
        }

        // En-passant
        if piece.0.can_enpassant() {
            if let Some((x, y)) = partial_game
                .get_board_with_game(game, piece.1.non_physical())?
                .en_passant
            {
                if (x, y) == forward(piece.1, Coords(0, 0, 1, 1), piece.0.white).physical()
                    || (x, y) == forward(piece.1, Coords(0, 0, -1, 1), piece.0.white).physical()
                {
                    moves.push(Move::new(
                        game,
                        partial_game,
                        piece.1,
                        Coords(piece.1.l(), piece.1.t(), x, y),
                    )?);
                }
            }
        }

        Some(PawnIter { moves, state: 0 })
    }
}

impl Iterator for PawnIter {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        if self.state >= self.moves.len() {
            None
        } else {
            self.state += 1;
            Some(self.moves[self.state - 1])
        }
    }
}

/**
    An iterator that yields the move of a ranging piece (ie. Rooks, Bishops, etc.).
    This iterator is created by `PiecePosition::generate_moves`.
**/
pub struct RangingPieceIter<'a, B: Clone + AsRef<Board> + 'a> {
    piece: Piece,
    coords: Coords,
    game: &'a Game,
    partial_game: &'a PartialGame<'a, B>,
    cardinalities: Vec<(isize, isize, isize, isize)>,
    cardinalities_index: usize,
    distance: usize,
}

impl<'a, B: Clone + AsRef<Board> + 'a> RangingPieceIter<'a, B> {
    /** Creates a new RangingPieceIter; unless you are implementing a new fairy piece, you should use `PiecePosition::generate_moves` **/
    pub fn new(
        piece: PiecePosition,
        game: &'a Game,
        partial_game: &'a PartialGame<'a, B>,
        cardinalities: Vec<(isize, isize, isize, isize)>,
    ) -> Self {
        RangingPieceIter {
            piece: piece.0,
            coords: piece.1,
            game,
            partial_game,
            cardinalities,
            cardinalities_index: 0,
            distance: 0,
        }
    }
}

impl<'a, B: Clone + AsRef<Board> + 'a> Iterator for RangingPieceIter<'a, B> {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        if self.cardinalities.len() <= self.cardinalities_index {
            return None;
        }
        let cardinality = self.cardinalities[self.cardinalities_index];
        self.distance += 1;

        let n_coords = self.coords + Coords::from(cardinality) * (self.distance as isize);

        let res = match self.partial_game.get_with_game(self.game, n_coords) {
            Tile::Void => None,
            Tile::Blank => Move::new(self.game, self.partial_game, self.coords, n_coords),
            Tile::Piece(p) => {
                if p.white != self.piece.white {
                    Move::new(self.game, self.partial_game, self.coords, n_coords)
                } else {
                    None
                }
            }
        };

        // Weird thing to enable TCR
        if res.is_some() {
            return res;
        }
        self.distance = 0;
        self.cardinalities_index += 1;
        self.next()
    }
}

/** Iterator that yields the moves of a piece that cannot make ranging moves (ie. knights, royal kings).
    This iterator is created by `PiecePosition::generate_moves`.
**/
pub struct OneStepPieceIter<'a, B: Clone + AsRef<Board> + 'a> {
    piece: Piece,
    coords: Coords,
    game: &'a Game,
    partial_game: &'a PartialGame<'a, B>,
    cardinalities: Vec<(isize, isize, isize, isize)>,
    cardinalities_index: usize,
}

impl<'a, B: Clone + AsRef<Board> + 'a> OneStepPieceIter<'a, B> {
    /** Creates a new OneStepPieceIter; unless you are implementing a new fairy piece, you should use `PiecePosition::generate_moves` **/
    pub fn new(
        piece: PiecePosition,
        game: &'a Game,
        partial_game: &'a PartialGame<'a, B>,
        cardinalities: Vec<(isize, isize, isize, isize)>,
    ) -> Self {
        OneStepPieceIter {
            piece: piece.0,
            coords: piece.1,
            game,
            partial_game,
            cardinalities,
            cardinalities_index: 0,
        }
    }
}

impl<'a, B: Clone + AsRef<Board> + 'a> Iterator for OneStepPieceIter<'a, B> {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        if self.cardinalities.len() <= self.cardinalities_index {
            return None;
        }
        self.cardinalities_index += 1;

        let cardinality = self.cardinalities[self.cardinalities_index - 1];

        let n_coords = self.coords + Coords::from(cardinality);

        let res = match self.partial_game.get_with_game(self.game, n_coords) {
            Tile::Void => None,
            Tile::Blank => Move::new(self.game, self.partial_game, self.coords, n_coords),
            Tile::Piece(p) => {
                if p.white != self.piece.white {
                    Move::new(self.game, self.partial_game, self.coords, n_coords)
                } else {
                    None
                }
            }
        };

        // Weird thing to enable TCR
        if res.is_some() {
            return res;
        }
        self.next()
    }
}

/** Iterator combining the different move kinds of all of the pieces. **/
// TODO: castling
pub enum PieceMoveIter<'a, B: Clone + AsRef<Board> + 'a> {
    Pawn(PawnIter),
    Ranging(RangingPieceIter<'a, B>),
    OneStep(OneStepPieceIter<'a, B>),
}

impl<'a, B: Clone + AsRef<Board> + 'a> Iterator for PieceMoveIter<'a, B> {
    type Item = Move;

    fn next(&mut self) -> Option<Move> {
        match self {
            PieceMoveIter::Pawn(i) => i.next(),
            PieceMoveIter::Ranging(i) => i.next(),
            PieceMoveIter::OneStep(i) => i.next(),
        }
    }
}

impl<'a, B: Clone + AsRef<Board> + 'a> GenMoves<'a, PieceMoveIter<'a, B>, B> for PiecePosition {
    /**
        Generates the moves for a single piece, given a partial game state and its complementary game state.
        You should be using this function if you wish to generate the moves of a piece.
    **/
    fn generate_moves(
        &'a self,
        game: &'a Game,
        partial_game: &'a PartialGame<'a, B>,
    ) -> Option<PieceMoveIter<'a, B>> {
        Some(match self.0.kind {
            PieceKind::Pawn | PieceKind::Brawn => {
                PieceMoveIter::Pawn(PawnIter::new(*self, game, partial_game)?)
            }
            PieceKind::Knight => PieceMoveIter::OneStep(OneStepPieceIter::new(
                *self,
                game,
                partial_game,
                PERMUTATIONS[0].clone(),
            )),
            PieceKind::Rook => PieceMoveIter::Ranging(RangingPieceIter::new(
                *self,
                game,
                partial_game,
                PERMUTATIONS[1].clone(),
            )),
            PieceKind::Bishop => PieceMoveIter::Ranging(RangingPieceIter::new(
                *self,
                game,
                partial_game,
                PERMUTATIONS[2].clone(),
            )),
            PieceKind::Unicorn => PieceMoveIter::Ranging(RangingPieceIter::new(
                *self,
                game,
                partial_game,
                PERMUTATIONS[3].clone(),
            )),
            PieceKind::Dragon => PieceMoveIter::Ranging(RangingPieceIter::new(
                *self,
                game,
                partial_game,
                PERMUTATIONS[4].clone(),
            )),
            PieceKind::Princess => PieceMoveIter::Ranging(RangingPieceIter::new(
                *self,
                game,
                partial_game,
                PERMUTATIONS[1]
                    .iter()
                    .chain(PERMUTATIONS[2].iter())
                    .cloned()
                    .collect(),
            )),
            PieceKind::Queen | PieceKind::RoyalQueen => {
                PieceMoveIter::Ranging(RangingPieceIter::new(
                    *self,
                    game,
                    partial_game,
                    PERMUTATIONS[1]
                        .iter()
                        .chain(PERMUTATIONS[2].iter())
                        .chain(PERMUTATIONS[3].iter())
                        .chain(PERMUTATIONS[4].iter())
                        .cloned()
                        .collect(),
                ))
            }
            PieceKind::King | PieceKind::CommonKing => {
                PieceMoveIter::Ranging(RangingPieceIter::new(
                    *self,
                    game,
                    partial_game,
                    PERMUTATIONS[1]
                        .iter()
                        .chain(PERMUTATIONS[2].iter())
                        .chain(PERMUTATIONS[3].iter())
                        .chain(PERMUTATIONS[4].iter())
                        .cloned()
                        .collect(),
                ))
            }
        })
    }

    fn validate_move(&'a self, game: &Game, partial_game: &PartialGame<B>, mv: &Move) -> bool {
        // TODO: a more optimized way of determining whether or not the move is legal
        // I don't want to do this right now
        Self::generate_moves(self, game, partial_game)
            .map(|mut i| i.find(|m| m == mv))
            .flatten()
            .is_some()
    }
}

lazy_static! {
    /// Permutations for the symmetric pieces of the base game
    pub static ref PERMUTATIONS: Vec<Vec<(isize, isize, isize, isize)>> = {
        [
            (
                vec![
                    (2, 1, 0, 0),
                    (1, 2, 0, 0),
                    (0, 2, 1, 0),
                    (0, 1, 2, 0),
                    (0, 0, 2, 1),
                    (0, 0, 1, 2),
                    (1, 0, 0, 2),
                    (2, 0, 0, 1),
                    (2, 0, 1, 0),
                    (1, 0, 2, 0),
                    (0, 2, 0, 1),
                    (0, 1, 0, 2),
                ],
                2,
            ),
            (
                vec![(1, 0, 0, 0), (0, 1, 0, 0), (0, 0, 1, 0), (0, 0, 0, 1)],
                1,
            ),
            (
                vec![
                    (1, 1, 0, 0),
                    (0, 1, 1, 0),
                    (0, 0, 1, 1),
                    (1, 0, 0, 1),
                    (1, 0, 1, 0),
                    (0, 1, 0, 1),
                ],
                2,
            ),
            (
                vec![(0, 1, 1, 1), (1, 0, 1, 1), (1, 1, 0, 1), (1, 1, 1, 0)],
                3,
            ),
            (vec![(1, 1, 1, 1)], 4),
        ]
        .iter()
        .map(|(group, cardinality)| {
            let mut res: Vec<(isize, isize, isize, isize)> =
                Vec::with_capacity(group.len() * 2usize.pow(*cardinality));
            for element in group {
                for perm_index in 0..(2usize.pow(*cardinality)) {
                    let mut perm: Vec<isize> = Vec::with_capacity(4);
                    let mut o = 0usize;
                    for i in vec![element.0, element.1, element.2, element.3] {
                        if i != 0 {
                            perm.push(if (perm_index >> o) % 2 == 1 { -i } else { i });
                            o += 1;
                        } else {
                            perm.push(0);
                        }
                    }
                    res.push((perm[0], perm[1], perm[2], perm[3]));
                }
            }
            res
        })
        .collect()
    };
}