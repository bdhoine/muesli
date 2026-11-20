//! Speaker echo: without headphones the microphone also hears the call from the speakers, so the
//! same words come in twice, once as "Others" (system audio) and once as "You" (microphone).
//! The system audio is the clean copy; the microphone's copy is dropped.

use std::collections::HashMap;

use crate::models::TranscriptSegment;

/// Seconds a "You" segment may lie outside an "Others" segment and still be its echo.
const SLACK: f64 = 2.0;
/// Share of a "You" segment's words that must also be in the overlapping "Others" segments.
const SHARED: f64 = 0.6;
/// Shorter segments ("Hello.", "Okay, thanks") are echoes only when every word matches and the
/// timing does too, since both sides may really say them.
const SHORT: usize = 3;
const SHORT_SLACK: f64 = 0.5;

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'').filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// Whether `mic` repeats what the `others` segments around it say.
fn is_echo(mic: &TranscriptSegment, others: &[&TranscriptSegment]) -> bool {
    let mine = words(&mic.text);
    if mine.is_empty() {
        return false;
    }
    let (slack, needed) = if mine.len() < SHORT { (SHORT_SLACK, 1.0) } else { (SLACK, SHARED) };
    let mut theirs: HashMap<String, usize> = HashMap::new();
    for o in others.iter().filter(|o| o.start - slack <= mic.end && mic.start <= o.end + slack) {
        for w in words(&o.text) {
            *theirs.entry(w).or_default() += 1;
        }
    }
    let shared = mine
        .iter()
        .filter(|w| match theirs.get_mut(*w) {
            Some(n) if *n > 0 => {
                *n -= 1;
                true
            }
            _ => false,
        })
        .count();
    shared as f64 >= needed * mine.len() as f64
}

/// Removes "You" segments that echo "Others". Returns whether anything was removed.
pub fn drop_echoes(transcript: &mut Vec<TranscriptSegment>) -> bool {
    let echoes: Vec<bool> = {
        let others: Vec<&TranscriptSegment> = transcript.iter().filter(|s| s.speaker == "Others").collect();
        transcript.iter().map(|s| s.speaker == "You" && is_echo(s, &others)).collect()
    };
    let before = transcript.len();
    let mut keep = echoes.iter().map(|e| !e);
    transcript.retain(|_| keep.next().unwrap_or(true));
    transcript.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(speaker: &str, start: f64, end: f64, text: &str) -> TranscriptSegment {
        TranscriptSegment { start, end, speaker: speaker.into(), text: text.into() }
    }

    #[test]
    fn the_microphone_copy_of_the_call_is_dropped() {
        let mut t = vec![
            seg("Others", 1620.0, 1624.0, "for the pages i assume if we have to make it happen in three sprints"),
            seg("You", 1620.0, 1624.5, "yet for the pages i assume if we have to make it happen in three sprints"),
            seg("You", 1647.0, 1650.0, "much with everything included."),
            seg("Others", 1647.0, 1650.0, "notch with everything included."),
        ];
        assert!(drop_echoes(&mut t));
        assert!(t.iter().all(|s| s.speaker == "Others"), "{t:?}");
    }

    #[test]
    fn an_echo_split_differently_still_matches() {
        let mut t = vec![
            seg("Others", 10.0, 18.0, "well let's plan for it in our planning meeting but yeah it needs everyone"),
            seg("You", 10.5, 13.0, "well let's plan for it"),
            seg("You", 13.0, 18.5, "in our planning meeting but yeah it needs everyone"),
        ];
        assert!(drop_echoes(&mut t));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn short_echoes_need_every_word_at_the_same_moment() {
        let mut t = vec![
            seg("Others", 36.0, 38.0, "Alright."),
            seg("You", 36.0, 38.0, "Alright."),
            seg("Others", 38.0, 40.0, "Hello."),
            seg("You", 38.0, 40.0, "Hello?"),
        ];
        assert!(drop_echoes(&mut t));
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn real_answers_and_short_words_stay() {
        let mut t = vec![
            seg("Others", 0.0, 4.0, "can you take the testing on windows"),
            seg("You", 4.0, 6.0, "yes I will take the testing"),
            seg("Others", 30.0, 31.0, "okay thanks"),
            seg("You", 32.0, 33.0, "okay thanks"),
            seg("You", 30.0, 31.0, "yes"),                                   // the same moment, other words
            seg("You", 100.0, 104.0, "can you take the testing on windows"), // much later: not an echo
        ];
        assert!(!drop_echoes(&mut t));
        assert_eq!(t.len(), 6);
    }
}
