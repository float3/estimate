//! One question, everyone's bounds, and what the room as a whole thought.
//!
//! The host asks a question and everybody gives a range they are
//! `confidence` per cent sure the answer falls in. That is the game: not
//! being right, but knowing how wide to make the range. So the numbers the
//! host is shown are the room's bounds — where it agreed, how far apart it
//! got, who went narrow — and, once the host enters the answer, how many
//! ranges actually contained it.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const MAX_PLAYERS: usize = 200;
pub const MAX_ROUNDS: usize = 200;
pub const MAX_QUESTION: usize = 300;
pub const MAX_UNIT: usize = 24;
pub const MAX_NAME: usize = 32;

/// How sure players are meant to be. Offered rather than free, because the
/// number only means something if everyone in the room uses the same one.
pub const CONFIDENCES: &[u8] = &[50, 80, 90, 98];

/// One person's range for one question.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Bound {
    pub low: f64,
    pub high: f64,
    pub at: u64,
}

impl Bound {
    fn width(&self) -> f64 {
        self.high - self.low
    }

    fn contains(&self, answer: f64) -> bool {
        self.low <= answer && answer <= self.high
    }
}

#[derive(Serialize, Deserialize)]
pub struct Player {
    pub key: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct Round {
    pub question: String,
    /// What the number is in — "years", "km", "€". Shown beside the fields
    /// so that half the room does not answer in thousands.
    pub unit: String,
    /// The truth. The host can type it in before asking or after seeing the
    /// bounds; until it is revealed, nobody else is sent it.
    pub answer: Option<f64>,
    /// Whether players have been shown the answer and the room's numbers.
    pub revealed: bool,
    /// Whether bounds are still being taken.
    pub open: bool,
    pub asked: u64,
    /// By player key.
    pub bounds: HashMap<String, Bound>,
}

#[derive(Serialize, Deserialize)]
pub struct Game {
    /// The key players' links carry. The host's own key is the one this
    /// game is filed under.
    pub join: String,
    pub confidence: u8,
    pub players: Vec<Player>,
    /// Oldest first; the last one is the question the room is on.
    pub rounds: Vec<Round>,
    pub last_seen: u64,
}

impl rooms::Room for Game {
    fn seen(&mut self, now: u64) {
        self.last_seen = now;
    }

    /// A day after anyone last looked. An evening's questions are worth
    /// keeping for the length of the evening and no longer.
    fn expired(&self, now: u64) -> bool {
        self.last_seen + rooms::DAY_MS < now
    }
}

/// Trims, collapses the whitespace inside, and cuts to `max` characters
/// rather than bytes, so that a name in any script survives.
fn tidy(text: &str, max: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

impl Game {
    pub fn new(now: u64) -> Game {
        Game {
            join: rooms::key::new(),
            confidence: 90,
            players: Vec::new(),
            rounds: Vec::new(),
            last_seen: now,
        }
    }

    pub fn round(&self) -> Option<&Round> {
        self.rounds.last()
    }

    fn round_mut(&mut self) -> Result<&mut Round, &'static str> {
        self.rounds.last_mut().ok_or("No question has been asked.")
    }

    pub fn player(&self, key: &str) -> Option<&Player> {
        self.players.iter().find(|p| p.key == key)
    }

    /// Takes a player's name and gives back the key their phone keeps.
    ///
    /// A name already in the room gives back that player's existing key, so
    /// that a phone which lost its key — a closed tab, a cleared browser —
    /// can walk back in. It also means anyone in the room can take over a
    /// name by typing it exactly. In a room where everybody can already see
    /// everybody, that is the right trade; nothing here is worth guarding.
    pub fn join(&mut self, name: &str, now: u64) -> Result<String, &'static str> {
        let name = tidy(name, MAX_NAME);
        if name.is_empty() {
            return Err("Please give a name, so the host knows who is in.");
        }
        if let Some(player) = self.players.iter().find(|p| p.name == name) {
            return Ok(player.key.clone());
        }
        if self.players.len() >= MAX_PLAYERS {
            return Err("This room is full.");
        }
        let key = rooms::key::new();
        self.players.push(Player {
            key: key.clone(),
            name,
        });
        self.last_seen = now;
        Ok(key)
    }

    pub fn set_confidence(&mut self, confidence: u8) -> Result<(), &'static str> {
        if !CONFIDENCES.contains(&confidence) {
            return Err("That is not one of the confidence levels.");
        }
        self.confidence = confidence;
        Ok(())
    }

    /// Asks the next question. The one before it stops taking bounds but
    /// keeps everything it was given.
    pub fn ask(
        &mut self,
        question: &str,
        unit: &str,
        answer: Option<f64>,
        now: u64,
    ) -> Result<(), &'static str> {
        let question = tidy(question, MAX_QUESTION);
        if question.is_empty() {
            return Err("A question needs some text.");
        }
        if self.rounds.len() >= MAX_ROUNDS {
            return Err("That is as many questions as one room holds.");
        }
        if answer.is_some_and(|answer| !answer.is_finite()) {
            return Err("That answer is not a number.");
        }
        if let Some(round) = self.rounds.last_mut() {
            round.open = false;
        }
        self.rounds.push(Round {
            question,
            unit: tidy(unit, MAX_UNIT),
            answer,
            revealed: false,
            open: true,
            asked: now,
            bounds: HashMap::new(),
        });
        Ok(())
    }

    pub fn set_open(&mut self, open: bool) -> Result<(), &'static str> {
        self.round_mut()?.open = open;
        Ok(())
    }

    pub fn set_answer(&mut self, answer: Option<f64>) -> Result<(), &'static str> {
        if answer.is_some_and(|answer| !answer.is_finite()) {
            return Err("That answer is not a number.");
        }
        self.round_mut()?.answer = answer;
        Ok(())
    }

    /// Shows the room what it thought. Stops taking bounds at the same
    /// time: a range typed after the answer is on the wall is not a guess.
    pub fn reveal(&mut self) -> Result<(), &'static str> {
        let round = self.round_mut()?;
        round.open = false;
        round.revealed = true;
        Ok(())
    }

    /// Takes one player's range. Bounds the wrong way round are swapped
    /// rather than refused — the fields get filled in either order.
    pub fn bound(
        &mut self,
        player: &str,
        low: f64,
        high: f64,
        now: u64,
    ) -> Result<(), &'static str> {
        if !low.is_finite() || !high.is_finite() {
            return Err("Both bounds need to be numbers.");
        }
        if self.player(player).is_none() {
            return Err("This link does not belong to anyone in the room.");
        }
        let round = self.round_mut()?;
        if !round.open {
            return Err("This question is closed.");
        }
        let (low, high) = if low <= high {
            (low, high)
        } else {
            (high, low)
        };
        round
            .bounds
            .insert(player.to_string(), Bound { low, high, at: now });
        Ok(())
    }
}

/// A number and whose it was.
#[derive(Serialize)]
pub struct Extreme {
    pub name: String,
    pub value: f64,
}

/// What the room thought, which is the part the host reads out.
#[derive(Serialize)]
pub struct Stats {
    pub n: usize,
    /// The middle of the room: the median lower bound and the median upper.
    pub median_low: f64,
    pub median_high: f64,
    pub mean_low: f64,
    pub mean_high: f64,
    /// The furthest anybody went in either direction.
    pub lowest: Extreme,
    pub highest: Extreme,
    /// The least and the most sure range in the room.
    pub widest: Extreme,
    pub tightest: Extreme,
    pub median_width: f64,
    /// How many ranges contained the answer, once there is one.
    pub hits: Option<usize>,
}

fn median(sorted: &[f64]) -> f64 {
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn sorted_by<T: Copy>(values: impl Iterator<Item = T>, of: impl Fn(T) -> f64) -> Vec<T> {
    let mut values: Vec<T> = values.collect();
    values.sort_by(|a, b| of(*a).total_cmp(&of(*b)));
    values
}

impl Round {
    /// The bounds with the names attached, in the order people joined.
    fn named<'a>(&'a self, players: &'a [Player]) -> Vec<(&'a str, Bound)> {
        players
            .iter()
            .filter_map(|p| self.bounds.get(&p.key).map(|b| (p.name.as_str(), *b)))
            .collect()
    }

    pub fn stats(&self, players: &[Player]) -> Option<Stats> {
        let named = self.named(players);
        let n = named.len();
        if n == 0 {
            return None;
        }
        let lows = sorted_by(named.iter().map(|(_, b)| b.low), |low| low);
        let highs = sorted_by(named.iter().map(|(_, b)| b.high), |high| high);
        let widths = sorted_by(named.iter().map(|(_, b)| b.width()), |width| width);
        let by_low = sorted_by(named.iter().copied(), |(_, b)| b.low);
        let by_high = sorted_by(named.iter().copied(), |(_, b)| b.high);
        let by_width = sorted_by(named.iter().copied(), |(_, b)| b.width());
        let at = |name: &str, value: f64| Extreme {
            name: name.to_string(),
            value,
        };
        Some(Stats {
            n,
            median_low: median(&lows),
            median_high: median(&highs),
            mean_low: lows.iter().sum::<f64>() / n as f64,
            mean_high: highs.iter().sum::<f64>() / n as f64,
            lowest: at(by_low[0].0, by_low[0].1.low),
            highest: at(by_high[n - 1].0, by_high[n - 1].1.high),
            widest: at(by_width[n - 1].0, by_width[n - 1].1.width()),
            tightest: at(by_width[0].0, by_width[0].1.width()),
            median_width: median(&widths),
            hits: self
                .answer
                .map(|answer| named.iter().filter(|(_, b)| b.contains(answer)).count()),
        })
    }
}

/// How one player has done over the evening.
#[derive(Serialize)]
pub struct Score {
    pub name: String,
    /// Questions answered.
    pub answered: usize,
    /// Of those, the ones with a known answer, which are the only ones a
    /// hit rate can be counted over.
    pub scored: usize,
    pub hits: usize,
    /// Questions where their range came in narrower than the room's median
    /// one. Widths are not comparable between questions; this is.
    pub tighter: usize,
}

impl Game {
    /// Everyone's evening, best hit rate first, and the narrower ranges
    /// ahead on a tie — a hit rate is easy to come by if every answer is
    /// "between zero and a billion".
    pub fn scores(&self) -> Vec<Score> {
        self.scores_over(|_| true)
    }

    /// The same, counting only the questions the host has revealed. This is
    /// the one a player is sent: a tally over a question still open would
    /// tell them whether their own range had hit, which is the answer.
    pub fn scores_revealed(&self) -> Vec<Score> {
        self.scores_over(|round| round.revealed)
    }

    fn scores_over(&self, counted: impl Fn(&Round) -> bool) -> Vec<Score> {
        let mut scores: Vec<Score> = self
            .players
            .iter()
            .map(|player| {
                let mut score = Score {
                    name: player.name.clone(),
                    answered: 0,
                    scored: 0,
                    hits: 0,
                    tighter: 0,
                };
                for round in self.rounds.iter().filter(|round| counted(round)) {
                    let Some(bound) = round.bounds.get(&player.key) else {
                        continue;
                    };
                    score.answered += 1;
                    if let Some(answer) = round.answer {
                        score.scored += 1;
                        if bound.contains(answer) {
                            score.hits += 1;
                        }
                    }
                    if let Some(stats) = round.stats(&self.players)
                        && bound.width() < stats.median_width
                    {
                        score.tighter += 1;
                    }
                }
                score
            })
            .filter(|score| score.answered > 0)
            .collect();
        let rate = |score: &Score| {
            if score.scored == 0 {
                -1.0
            } else {
                score.hits as f64 / score.scored as f64
            }
        };
        scores.sort_by(|a, b| {
            rate(b)
                .total_cmp(&rate(a))
                .then(b.tighter.cmp(&a.tighter))
                .then(a.name.cmp(&b.name))
        });
        scores
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> (Game, Vec<String>) {
        let mut game = Game::new(1000);
        let keys = ["Ada", "Bo", "Cy", "Di"]
            .iter()
            .map(|name| game.join(name, 1000).expect("the room has space"))
            .collect();
        (game, keys)
    }

    #[test]
    fn the_room_gets_its_middle_its_edges_and_its_extremes() {
        let (mut game, keys) = room();
        game.ask("How tall is the Frauenkirche?", "m", Some(99.0), 1000)
            .unwrap();
        let ranges = [(10.0, 40.0), (20.0, 60.0), (30.0, 90.0), (100.0, 120.0)];
        for (key, (low, high)) in keys.iter().zip(ranges) {
            game.bound(key, low, high, 1001).unwrap();
        }
        let stats = game.round().unwrap().stats(&game.players).unwrap();
        assert_eq!(stats.n, 4);
        assert_eq!(stats.median_low, 25.0, "lows 10 20 30 100");
        assert_eq!(stats.median_high, 75.0, "highs 40 60 90 120");
        assert_eq!(stats.mean_low, 40.0);
        assert_eq!(stats.mean_high, 77.5);
        assert_eq!(
            (stats.lowest.name.as_str(), stats.lowest.value),
            ("Ada", 10.0)
        );
        assert_eq!(
            (stats.highest.name.as_str(), stats.highest.value),
            ("Di", 120.0)
        );
        assert_eq!(
            (stats.widest.name.as_str(), stats.widest.value),
            ("Cy", 60.0),
            "widths 30 40 60 20"
        );
        assert_eq!(
            (stats.tightest.name.as_str(), stats.tightest.value),
            ("Di", 20.0)
        );
        assert_eq!(stats.median_width, 35.0);
        assert_eq!(stats.hits, Some(0), "99 is above every range but Di's");
    }

    #[test]
    fn a_range_that_ends_on_the_answer_counts_as_containing_it() {
        let (mut game, keys) = room();
        game.ask("When?", "", Some(50.0), 1000).unwrap();
        game.bound(&keys[0], 50.0, 80.0, 1001).unwrap(); // starts on it
        game.bound(&keys[1], 10.0, 50.0, 1001).unwrap(); // ends on it
        game.bound(&keys[2], 50.0, 50.0, 1001).unwrap(); // is it
        game.bound(&keys[3], 51.0, 80.0, 1001).unwrap(); // misses by one
        let stats = game.round().unwrap().stats(&game.players).unwrap();
        assert_eq!(stats.hits, Some(3));
    }

    #[test]
    fn bounds_the_wrong_way_round_are_swapped_rather_than_refused() {
        let (mut game, keys) = room();
        game.ask("How many?", "", None, 1000).unwrap();
        game.bound(&keys[0], 90.0, 10.0, 1001).unwrap();
        let bound = game.round().unwrap().bounds[&keys[0]];
        assert_eq!((bound.low, bound.high), (10.0, 90.0));
    }

    #[test]
    fn a_closed_question_takes_nothing_more() {
        let (mut game, keys) = room();
        game.ask("How many?", "", None, 1000).unwrap();
        game.bound(&keys[0], 1.0, 2.0, 1001).unwrap();
        game.reveal().unwrap();
        assert!(game.bound(&keys[0], 1.0, 1000.0, 1002).is_err());
        let bound = game.round().unwrap().bounds[&keys[0]];
        assert_eq!(bound.high, 2.0, "the range stands as it was when it closed");
    }

    #[test]
    fn asking_again_closes_the_last_question_and_keeps_its_answers() {
        let (mut game, keys) = room();
        game.ask("First", "", None, 1000).unwrap();
        game.bound(&keys[0], 1.0, 2.0, 1001).unwrap();
        game.ask("Second", "", None, 2000).unwrap();
        assert_eq!(game.rounds.len(), 2);
        assert!(!game.rounds[0].open);
        assert_eq!(game.rounds[0].bounds.len(), 1);
        assert!(game.rounds[1].open);
        assert!(game.rounds[1].bounds.is_empty());
    }

    #[test]
    fn a_name_already_in_the_room_walks_back_in_as_itself() {
        let (mut game, keys) = room();
        assert_eq!(game.join("  Ada  ", 2000).unwrap(), keys[0]);
        assert_eq!(game.players.len(), 4);
        assert_ne!(game.join("ada", 2000).unwrap(), keys[0], "another name");
        assert_eq!(game.players.len(), 5);
    }

    #[test]
    fn a_nameless_player_is_turned_away() {
        let mut game = Game::new(1000);
        assert!(game.join("   ", 1000).is_err());
    }

    #[test]
    fn nothing_but_a_question_is_asked() {
        let mut game = Game::new(1000);
        assert!(game.ask("  ", "", None, 1000).is_err());
        assert!(game.set_open(true).is_err(), "there is no question yet");
        assert!(game.round().is_none());
    }

    #[test]
    fn the_scoreboard_counts_hits_and_who_went_narrow() {
        let (mut game, keys) = room();
        game.ask("One", "", Some(50.0), 1000).unwrap();
        game.bound(&keys[0], 40.0, 60.0, 1001).unwrap(); // hit, width 20
        game.bound(&keys[1], 0.0, 1000.0, 1001).unwrap(); // hit, width 1000
        game.bound(&keys[2], 0.0, 10.0, 1001).unwrap(); // miss, width 10
        game.ask("Two", "", None, 2000).unwrap();
        game.bound(&keys[0], 1.0, 2.0, 2001).unwrap();

        let scores = game.scores();
        assert_eq!(
            scores.len(),
            3,
            "Di answered nothing and is not on the board"
        );
        assert_eq!(scores[0].name, "Ada");
        assert_eq!(
            (scores[0].answered, scores[0].scored, scores[0].hits),
            (2, 1, 1)
        );
        assert_eq!(scores[0].tighter, 0, "20 is the median width, not under it");
        assert_eq!(scores[1].name, "Bo", "same hit rate, so by name");
        assert_eq!(scores[1].hits, 1);
        assert_eq!(scores[2].name, "Cy", "a miss comes last");
        assert_eq!(scores[2].tighter, 1);
    }

    #[test]
    fn a_tally_of_the_revealed_questions_only_counts_those() {
        let (mut game, keys) = room();
        game.ask("One", "", Some(50.0), 1000).unwrap();
        game.bound(&keys[0], 40.0, 60.0, 1001).unwrap();
        assert!(
            game.scores_revealed().is_empty(),
            "nothing is revealed yet, so there is nothing to tally"
        );
        game.reveal().unwrap();
        assert_eq!(game.scores_revealed()[0].hits, 1);

        game.ask("Two", "", Some(10.0), 2000).unwrap();
        game.bound(&keys[0], 1.0, 2.0, 2001).unwrap();
        assert_eq!(game.scores_revealed()[0].scored, 1, "the open one is out");
        assert_eq!(game.scores()[0].scored, 2, "the host sees both");
    }

    #[test]
    fn a_question_nobody_has_answered_has_no_numbers() {
        let (mut game, _) = room();
        game.ask("One", "", None, 1000).unwrap();
        assert!(game.round().unwrap().stats(&game.players).is_none());
    }

    #[test]
    fn the_confidence_level_is_one_of_the_offered_ones() {
        let mut game = Game::new(1000);
        assert!(game.set_confidence(90).is_ok());
        assert!(game.set_confidence(73).is_err());
        assert_eq!(game.confidence, 90);
    }

    #[test]
    fn a_name_is_cut_to_length_in_characters() {
        let mut game = Game::new(1000);
        let key = game.join(&"ä".repeat(MAX_NAME + 10), 1000).unwrap();
        assert_eq!(game.player(&key).unwrap().name.chars().count(), MAX_NAME);
    }
}
