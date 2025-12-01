use bevy::prelude::*;
use std::collections::HashSet;

/// Pointer state snapshot, updated every Wayland dispatch tick.
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
