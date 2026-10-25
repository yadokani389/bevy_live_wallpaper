use std::collections::HashSet;

use bevy::prelude::*;

/// Pointer state snapshot, updated by the active backend.
#[derive(Resource, Clone, Debug, Default)]
pub struct WallpaperPointerState {
    /// Last observed pointer sample across all outputs.
    pub last: Option<PointerSample>,
}

#[derive(Clone, Debug, Default)]
pub struct PointerSample {
    /// Backend-specific output/monitor identifier (per backend, best-effort).
    /// `None` when the pointer is not over any known output.
    pub output: Option<u32>,
    /// Global logical position (surface local + output offset).
    pub position: Vec2,
    /// Delta from the previous sample in global logical coordinates.
    pub delta: Vec2,
    pub last_button: Option<PointerButton>,
    /// Buttons currently held down.
    pub pressed: HashSet<MouseButton>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerButton {
    pub button: Option<MouseButton>,
    pub pressed: bool,
}

#[cfg(any(feature = "x11", target_os = "windows", test))]
pub(crate) fn detect_last_button(
    previous: Option<&HashSet<MouseButton>>,
    current: &HashSet<MouseButton>,
) -> Option<PointerButton> {
    let empty = HashSet::new();
    let previous = previous.unwrap_or(&empty);
    [(current, previous, true), (previous, current, false)]
        .into_iter()
        .find_map(|(held, other, pressed)| {
            held.difference(other)
                .min_by_key(|button| match **button {
                    MouseButton::Left => (0, 0),
                    MouseButton::Right => (1, 0),
                    MouseButton::Middle => (2, 0),
                    MouseButton::Back => (3, 0),
                    MouseButton::Forward => (4, 0),
                    MouseButton::Other(code) => (5, code),
                })
                .map(|button| PointerButton {
                    button: Some(*button),
                    pressed,
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presses_precede_releases_and_have_stable_priority() {
        let both = HashSet::from([MouseButton::Right, MouseButton::Left]);
        assert_eq!(
            detect_last_button(None, &both),
            Some(PointerButton {
                button: Some(MouseButton::Left),
                pressed: true,
            })
        );
        assert_eq!(detect_last_button(Some(&both), &both), None);
        assert_eq!(
            detect_last_button(Some(&both), &HashSet::new()),
            Some(PointerButton {
                button: Some(MouseButton::Left),
                pressed: false,
            })
        );
        assert_eq!(
            detect_last_button(
                Some(&HashSet::from([MouseButton::Left])),
                &HashSet::from([MouseButton::Middle])
            ),
            Some(PointerButton {
                button: Some(MouseButton::Middle),
                pressed: true,
            })
        );
        assert_eq!(
            detect_last_button(
                None,
                &HashSet::from([MouseButton::Other(9), MouseButton::Other(3),])
            ),
            Some(PointerButton {
                button: Some(MouseButton::Other(3)),
                pressed: true
            })
        );
    }
}
