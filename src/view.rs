//! What each screen is sent.
//!
//! The host's view contains everything. A player's view contains their own
//! ranges and nothing else about the question until the host reveals it.
//! The two are built here rather than filtered in the page, because
//! anything sent to a page can be read there.

use serde::Serialize;

use crate::game::{Bound, Change, Game, Pass, Round, Score, Shift, Stage, Stats};

#[derive(Serialize)]
pub struct Who<'a> {
    name: &'a str,
    /// Whether they gave a range in the first pass, and in the second.
    first: bool,
    second: bool,
}

#[derive(Serialize)]
pub struct Answer<'a> {
    name: &'a str,
    low: f64,
    high: f64,
    width: f64,
    /// Whether it contained the answer, where there is one.
    hit: Option<bool>,
}

#[derive(Serialize)]
pub struct Asked<'a> {
    question: &'a str,
    unit: &'a str,
    stage: Stage,
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
    /// How many contained it in the first pass, where there was one.
    hits_before: Option<usize>,
}

#[derive(Serialize)]
pub struct Host<'a> {
    join: &'a str,
    confidence: u8,
    players: Vec<Who<'a>>,
    round: Option<Asked<'a>>,
    /// The first pass, the second pass, and everybody's final ranges.
    before: Option<Stats>,
    after: Option<Stats>,
    standing: Option<Stats>,
    /// The comparison, once there are two passes to compare.
    shift: Option<Shift>,
    changes: Vec<Change>,
    /// Everybody's final range, lowest first.
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
    /// This player's two ranges.
    mine_before: Option<Bound>,
    mine_after: Option<Bound>,
    /// How many have answered the pass being taken. Before the reveal,
    /// this is all a player is told about anybody else.
    submitted: usize,
    players: usize,
    /// Everything from here down is only sent after the reveal.
    standing: Option<Stats>,
    before: Option<Stats>,
    shift: Option<Shift>,
    hit: Option<bool>,
    hit_before: Option<bool>,
    /// How this player's own two answers differed.
    mine_changed: Option<Change>,
    score: Option<Score>,
}

impl Round {
    fn asked(&self, to_host: bool) -> Asked<'_> {
        let revealed = self.stage == Stage::Revealed;
        Asked {
            question: &self.question,
            unit: &self.unit,
            stage: self.stage,
            // The host is sent the answer as soon as there is one.
            // Everybody else is sent it after the reveal.
            answer: self.answer.filter(|_| to_host || revealed),
            asked: self.asked,
        }
    }

    /// How many have answered the pass being taken. Once the question is
    /// over, how many answered it at all.
    fn in_so_far(&self, players: &[crate::game::Player]) -> usize {
        match self.stage {
            Stage::First | Stage::Discussion => self.taken(Pass::First),
            Stage::Second => self.taken(Pass::Second),
            Stage::Revealed => players
                .iter()
                .filter(|p| self.standing(&p.key).is_some())
                .count(),
        }
    }
}

/// The host's view, which holds everything about the question.
pub fn host(game: &Game) -> Host<'_> {
    let round = game.round();
    let answers = round
        .map(|round| {
            let mut answers: Vec<Answer<'_>> = game
                .players
                .iter()
                .filter_map(|player| {
                    let bound = round.standing(&player.key)?;
                    Some(Answer {
                        name: &player.name,
                        low: bound.low,
                        high: bound.high,
                        width: bound.width(),
                        hit: round.answer.map(|answer| bound.contains(answer)),
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
                first: round.is_some_and(|r| r.before.contains_key(&player.key)),
                second: round.is_some_and(|r| r.after.contains_key(&player.key)),
            })
            .collect(),
        round: round.map(|round| round.asked(true)),
        before: round.and_then(|r| r.stats(&game.players, Pass::First)),
        after: round.and_then(|r| r.stats(&game.players, Pass::Second)),
        standing: round.and_then(|r| r.standing_stats(&game.players)),
        shift: round.and_then(|r| r.shift(&game.players)),
        changes: round.map(|r| r.changes(&game.players)).unwrap_or_default(),
        answers,
        history: game
            .rounds
            .iter()
            .rev()
            .skip(1)
            .map(|round| {
                let standing = round.standing_stats(&game.players);
                Past {
                    question: &round.question,
                    unit: &round.unit,
                    answer: round.answer,
                    n: standing.as_ref().map_or(0, |stats| stats.n),
                    hits: standing.and_then(|stats| stats.hits),
                    hits_before: round
                        .shift(&game.players)
                        .and_then(|shift| shift.hits_before),
                }
            })
            .collect(),
        scores: game.scores(),
    }
}

/// One player's own view. `key` is theirs.
pub fn player<'a>(game: &'a Game, key: &str) -> Option<Player<'a>> {
    let player = game.player(key)?;
    let round = game.round();
    let revealed = round.is_some_and(|r| r.stage == Stage::Revealed);
    let shown = round.filter(|_| revealed);
    let mine_before = round.and_then(|r| r.before.get(key)).copied();
    let mine_after = round.and_then(|r| r.after.get(key)).copied();
    let answer = round.and_then(|r| r.answer).filter(|_| revealed);
    let standing = round.and_then(|r| r.standing(key));
    Some(Player {
        name: &player.name,
        confidence: game.confidence,
        round: round.map(|round| round.asked(false)),
        mine_before,
        mine_after,
        submitted: round.map_or(0, |r| r.in_so_far(&game.players)),
        players: game.players.len(),
        standing: shown.and_then(|r| r.standing_stats(&game.players)),
        before: shown.and_then(|r| r.stats(&game.players, Pass::First)),
        shift: shown.and_then(|r| r.shift(&game.players)),
        hit: match (answer, standing) {
            (Some(answer), Some(bound)) => Some(bound.contains(answer)),
            _ => None,
        },
        hit_before: match (answer, mine_before) {
            (Some(answer), Some(bound)) => Some(bound.contains(answer)),
            _ => None,
        },
        mine_changed: shown.and_then(|r| {
            r.changes(&game.players)
                .into_iter()
                .find(|change| change.name == player.name)
        }),
        // The revealed questions only. A tally that included the open
        // question would say whether this player's own range had hit.
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
        assert!(view.standing.is_some());
        assert_eq!(view.hit, Some(false), "1 to 2 does not contain 1234567");
        assert_eq!(view.score.map(|s| s.scored), Some(1));
    }

    #[test]
    fn a_player_is_sent_their_own_two_ranges_and_what_changed() {
        let (mut game, key) = room();
        game.move_to(Stage::Second).unwrap();
        game.bound(&key, 1_000_000.0, 2_000_000.0, 1002).unwrap();

        let view = player(&game, &key).unwrap();
        assert_eq!(view.mine_before.map(|b| b.high), Some(2.0));
        assert_eq!(view.mine_after.map(|b| b.high), Some(2_000_000.0));
        assert!(
            view.mine_changed.is_none(),
            "what changed waits for the reveal, like everything else"
        );

        game.reveal().unwrap();
        let view = player(&game, &key).unwrap();
        let change = view.mine_changed.expect("both ranges are in");
        assert_eq!(change.before.high, 2.0);
        assert_eq!(change.after.low, 1_000_000.0);
        assert_eq!(view.hit, Some(true), "the second range caught it");
        assert_eq!(view.hit_before, Some(false), "the first did not");
    }

    #[test]
    fn the_host_sees_both_passes_apart_and_where_the_room_ended_up() {
        let (mut game, key) = room();
        let other = game.join("Bo", 1000).unwrap();
        game.bound(&other, 10.0, 20.0, 1001).unwrap();
        game.move_to(Stage::Second).unwrap();
        game.bound(&key, 5.0, 6.0, 1002).unwrap();

        let view = host(&game);
        assert_eq!(view.before.as_ref().map(|s| s.n), Some(2));
        assert_eq!(view.after.as_ref().map(|s| s.n), Some(1), "only Ada again");
        assert_eq!(
            view.standing.as_ref().map(|s| s.n),
            Some(2),
            "Bo's first range still stands for Bo"
        );
        assert_eq!(view.answers.len(), 2);
        assert_eq!(view.answers[0].name, "Ada", "5 to 6 is the lowest");
        assert!(view.players[0].second);
        assert!(!view.players[1].second);
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
        assert!(view.players.iter().all(|who| who.first));
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
        assert!(view.mine_before.is_none());
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
