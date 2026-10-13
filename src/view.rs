//! What each screen is sent.
//!
//! Two screens: the host's, which sees everything, and the player's, which
//! sees its own range and nothing else about the question until the host
//! reveals it. The split is here rather than in the page, because a page
//! can be read.

use serde::Serialize;

use crate::game::{Bound, Game, Round, Score, Stats};

#[derive(Serialize)]
pub struct Who<'a> {
    name: &'a str,
    /// Whether they have given a range for the question on now.
    answered: bool,
}

#[derive(Serialize)]
pub struct Answer<'a> {
    name: &'a str,
    low: f64,
    high: f64,
    width: f64,
    /// Whether it contained the answer, once there is one.
    hit: Option<bool>,
}

#[derive(Serialize)]
pub struct Asked<'a> {
    question: &'a str,
    unit: &'a str,
    open: bool,
    revealed: bool,
    answer: Option<f64>,
    asked: u64,
}

#[derive(Serialize)]
pub struct Past<'a> {
    question: &'a str,
    unit: &'a str,
    answer: Option<f64>,
    n: usize,
    hits: Option<usize>,
}

#[derive(Serialize)]
pub struct Host<'a> {
    join: &'a str,
    confidence: u8,
    players: Vec<Who<'a>>,
    round: Option<Asked<'a>>,
    stats: Option<Stats>,
    answers: Vec<Answer<'a>>,
    /// Newest first, the question on now excluded.
    history: Vec<Past<'a>>,
    scores: Vec<Score>,
}

#[derive(Serialize)]
pub struct Player<'a> {
    name: &'a str,
    confidence: u8,
    round: Option<Asked<'a>>,
    /// The range this player gave, which they can see and change while the
    /// question is open.
    mine: Option<Bound>,
    /// How far along the room is, which is all a player is told about
    /// everybody else before the reveal.
    submitted: usize,
    players: usize,
    /// Only once the host has revealed the question.
    stats: Option<Stats>,
    hit: Option<bool>,
    score: Option<Score>,
}

impl Round {
    fn asked(&self, to_host: bool) -> Asked<'_> {
        Asked {
            question: &self.question,
            unit: &self.unit,
            open: self.open,
            revealed: self.revealed,
            // The answer goes to the host whenever there is one, and to
            // everybody else only after the reveal.
            answer: self.answer.filter(|_| to_host || self.revealed),
            asked: self.asked,
        }
    }
}

/// Everything, because the host is the one running the room.
pub fn host(game: &Game) -> Host<'_> {
    let round = game.round();
    let stats = round.and_then(|r| r.stats(&game.players));
    let answers = round
        .map(|round| {
            let mut answers: Vec<Answer<'_>> = game
                .players
                .iter()
                .filter_map(|player| {
                    let bound = round.bounds.get(&player.key)?;
                    Some(Answer {
                        name: &player.name,
                        low: bound.low,
                        high: bound.high,
                        width: bound.high - bound.low,
                        hit: round
                            .answer
                            .map(|answer| bound.low <= answer && answer <= bound.high),
                    })
                })
                .collect();
            answers.sort_by(|a, b| a.low.total_cmp(&b.low));
            answers
        })
        .unwrap_or_default();
    Host {
        join: &game.join,
        confidence: game.confidence,
        players: game
            .players
            .iter()
            .map(|player| Who {
                name: &player.name,
                answered: round.is_some_and(|r| r.bounds.contains_key(&player.key)),
            })
            .collect(),
        round: round.map(|round| round.asked(true)),
        stats,
        answers,
        history: game
            .rounds
            .iter()
            .rev()
            .skip(1)
            .map(|round| Past {
                question: &round.question,
                unit: &round.unit,
                answer: round.answer,
                n: round.bounds.len(),
                hits: round.stats(&game.players).and_then(|stats| stats.hits),
            })
            .collect(),
        scores: game.scores(),
    }
}

/// One player's own view. `key` is theirs.
pub fn player<'a>(game: &'a Game, key: &str) -> Option<Player<'a>> {
    let player = game.player(key)?;
    let round = game.round();
    let mine = round.and_then(|r| r.bounds.get(key)).copied();
    let revealed = round.is_some_and(|r| r.revealed);
    let answer = round.and_then(|r| r.answer).filter(|_| revealed);
    Some(Player {
        name: &player.name,
        confidence: game.confidence,
        round: round.map(|round| round.asked(false)),
        mine,
        submitted: round.map_or(0, |r| r.bounds.len()),
        players: game.players.len(),
        stats: round
            .filter(|r| r.revealed)
            .and_then(|r| r.stats(&game.players)),
        hit: match (answer, mine) {
            (Some(answer), Some(bound)) => Some(bound.low <= answer && answer <= bound.high),
            _ => None,
        },
        // Only the revealed questions: a tally over the one still open
        // would say whether this player's own range had hit.
        score: game
            .scores_revealed()
            .into_iter()
            .find(|score| score.name == player.name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A number that could not appear in the view by accident.
    const ANSWER: f64 = 1_234_567.0;

    fn room() -> (Game, String) {
        let mut game = Game::new(1000);
        let key = game.join("Ada", 1000).unwrap();
        game.ask("How many?", "people", Some(ANSWER), 1000).unwrap();
        game.bound(&key, 1.0, 2.0, 1001).unwrap();
        (game, key)
    }

    fn json<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).expect("the view serializes")
    }

    #[test]
    fn the_answer_does_not_reach_a_player_before_the_reveal() {
        let (mut game, key) = room();
        let before = json(&player(&game, &key).unwrap());
        assert!(
            !before.contains("1234567"),
            "the answer was sent out early: {before}"
        );
        assert!(json(&host(&game)).contains("1234567"), "the host has it");

        game.reveal().unwrap();
        assert!(json(&player(&game, &key).unwrap()).contains("1234567"));
    }

    #[test]
    fn nor_does_what_anybody_else_said() {
        let (mut game, key) = room();
        let other = game.join("Bo", 1000).unwrap();
        game.bound(&other, 500.0, 600.0, 1001).unwrap();
        let view = player(&game, &key).unwrap();
        assert_eq!(view.submitted, 2, "only the count");
        assert_eq!(view.players, 2);
        let text = json(&view);
        assert!(!text.contains("Bo"), "another player's name: {text}");
        assert!(!text.contains("500"), "another player's range: {text}");
    }

    #[test]
    fn a_players_own_tally_waits_for_the_reveal_too() {
        let (game, key) = room();
        let view = player(&game, &key).unwrap();
        assert!(
            view.score.is_none(),
            "a tally over an open question says whether the range hit"
        );
    }

    #[test]
    fn the_room_is_shown_once_it_is_revealed() {
        let (mut game, key) = room();
        game.reveal().unwrap();
        let view = player(&game, &key).unwrap();
        assert!(view.stats.is_some());
        assert_eq!(view.hit, Some(false), "1 to 2 does not contain 1234567");
        assert_eq!(view.score.map(|s| s.scored), Some(1));
    }

    #[test]
    fn the_host_sees_the_ranges_lowest_first() {
        let (mut game, _) = room();
        let other = game.join("Bo", 1000).unwrap();
        game.bound(&other, -5.0, 0.0, 1001).unwrap();
        let view = host(&game);
        assert_eq!(view.answers.len(), 2);
        assert_eq!(view.answers[0].name, "Bo");
        assert_eq!(view.answers[0].hit, Some(false));
        assert_eq!(view.players.len(), 2);
        assert!(view.players.iter().all(|who| who.answered));
    }

    #[test]
    fn a_room_with_no_question_in_it_still_renders() {
        let mut game = Game::new(1000);
        let key = game.join("Ada", 1000).unwrap();
        let view = host(&game);
        assert!(view.round.is_none());
        assert!(view.answers.is_empty());
        assert!(view.history.is_empty());
        let view = player(&game, &key).unwrap();
        assert!(view.round.is_none());
        assert!(view.mine.is_none());
    }

    #[test]
    fn a_key_that_is_not_in_the_room_has_no_view() {
        let (game, _) = room();
        assert!(player(&game, "nobody").is_none());
    }

    #[test]
    fn the_history_holds_every_question_but_the_one_on_now() {
        let (mut game, key) = room();
        game.ask("Second", "", None, 2000).unwrap();
        game.bound(&key, 3.0, 4.0, 2001).unwrap();
        let view = host(&game);
        assert_eq!(view.round.map(|r| r.question), Some("Second"));
        assert_eq!(view.history.len(), 1);
        assert_eq!(view.history[0].question, "How many?");
        assert_eq!(view.history[0].hits, Some(0));
    }
}
