use crate::settings::{update_settings, TipRotationSettings};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TipDefinition {
    pub id: &'static str,
    pub category: &'static str,
    pub text: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TipSegment<'a> {
    pub text: &'a str,
    pub shortcut: bool,
}

include!(concat!(env!("OUT_DIR"), "/capture_tips.rs"));

pub fn take_next_persisted_tip() -> Option<TipDefinition> {
    let mut selected = None;
    let result = update_settings(|settings| {
        if settings.show_capture_tips {
            selected = take_next_tip(&mut settings.tip_rotation);
        }
    });
    result.ok().and(selected)
}

pub fn segments(text: &str) -> Vec<TipSegment<'_>> {
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut shortcut = false;
    for (index, character) in text.char_indices() {
        if (character == '{' && !shortcut) || (character == '}' && shortcut) {
            if start < index {
                result.push(TipSegment {
                    text: &text[start..index],
                    shortcut,
                });
            }
            shortcut = character == '{';
            start = index + character.len_utf8();
        }
    }
    if start < text.len() {
        result.push(TipSegment {
            text: &text[start..],
            shortcut,
        });
    }
    result
}

fn take_next_tip(rotation: &mut TipRotationSettings) -> Option<TipDefinition> {
    rotation
        .remaining_ids
        .retain(|id| CAPTURE_TIPS.iter().any(|tip| tip.id == id));
    if rotation.remaining_ids.is_empty() {
        rotation.remaining_ids = CAPTURE_TIPS.iter().map(|tip| tip.id.to_string()).collect();
        shuffle(&mut rotation.remaining_ids);
        if rotation.remaining_ids.len() > 1
            && rotation.remaining_ids.last() == rotation.last_id.as_ref()
        {
            let last = rotation.remaining_ids.len() - 1;
            rotation.remaining_ids.swap(0, last);
        }
    }
    let id = rotation.remaining_ids.pop()?;
    let tip = CAPTURE_TIPS.iter().copied().find(|tip| tip.id == id)?;
    rotation.last_id = Some(id);
    Some(tip)
}

fn shuffle(values: &mut [String]) {
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x5343_5245_454e);
    for index in (1..values.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        values.swap(index, seed as usize % (index + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tips_are_complete_and_unique() {
        assert!(CAPTURE_TIPS.len() >= 30);
        let mut ids = CAPTURE_TIPS.iter().map(|tip| tip.id).collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CAPTURE_TIPS.len());
    }

    #[test]
    fn shortcut_markers_become_underlined_segments() {
        assert_eq!(
            segments("Save with {Ctrl+S}."),
            vec![
                TipSegment {
                    text: "Save with ",
                    shortcut: false
                },
                TipSegment {
                    text: "Ctrl+S",
                    shortcut: true
                },
                TipSegment {
                    text: ".",
                    shortcut: false
                }
            ]
        );
    }

    #[test]
    fn shuffle_bag_does_not_repeat_before_exhaustion() {
        let mut rotation = TipRotationSettings::default();
        let mut ids = Vec::new();
        for _ in 0..CAPTURE_TIPS.len() {
            ids.push(take_next_tip(&mut rotation).unwrap().id);
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CAPTURE_TIPS.len());
    }

    #[test]
    fn a_new_shuffle_cycle_never_repeats_the_previous_tip() {
        for previous in CAPTURE_TIPS {
            let mut rotation = TipRotationSettings {
                remaining_ids: Vec::new(),
                last_id: Some(previous.id.to_string()),
            };
            let next = take_next_tip(&mut rotation).unwrap();
            assert_ne!(next.id, previous.id);
        }
    }
}
