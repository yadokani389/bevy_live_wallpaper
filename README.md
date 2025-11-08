# Bevy Live Wallpaper

A [Bevy](https://bevyengine.org/) plugin that renders your scene into a Wayland
layer-shell surface or a Windows desktop background.

## Compatibility

| Bevy Version | Crate Version |
| ------------ | ------------- |
| `0.17`       | `0.1.0`       |

## Requirements

- **Wayland**: A compositor that advertises `zwlr_layer_shell_v1` (e.g. Sway, Hyprland,
  River).
- **Windows**: The standard desktop environment.

## Features

The crate provides two platform-specific backends:

- **`wayland`** (feature): For Linux/BSD, integrates with Wayland compositors using the `zwlr_layer_shell_v1` protocol.
- **Windows**: For Windows, integrates with the desktop by parenting the Bevy window to the `WorkerW` wallpaper window.

### Configuration

- On **Windows**, the appropriate backend is selected automatically. No features need to be enabled.
- On **Linux/BSD**, you must enable the `wayland` feature.

```toml
# In your Cargo.toml

# For Windows:
[dependencies]
bevy_live_wallpaper = "0.1.0"

# For Linux/BSD (Wayland):
[dependencies]
bevy_live_wallpaper = { version = "0.1.0", features = ["wayland"] }
```

## Usage

Add the `LiveWallpaperPlugin` to your app. To make your application cross-platform, you will need to use conditional compilation (`#[cfg]`) for platform-specific setup.

- On **Wayland**, you must disable the primary window and add the `LiveWallpaperCamera` component to the camera you want to render.
- On **Windows**, the plugin will automatically find the primary window and parent it to the desktop background. The `LiveWallpaperCamera` component is not used.

Here is a complete, cross-platform example:

```rust
use bevy::prelude::*;
use bevy_live_wallpaper::LiveWallpaperPlugin;

fn main() {
    let mut app = App::new();

    // Platform-specific plugin setup
    #[cfg(target_os = "linux")]
    {
        // On Wayland, we can't have a primary window
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        }));
    }
    #[cfg(not(target_os = "linux"))]
    {
        // On Windows, we use a normal window that gets reparented
        app.add_plugins(DefaultPlugins);
    }

    app.add_plugins(LiveWallpaperPlugin)
        .add_systems(Startup, setup_scene)
        .run();
}

fn setup_scene(mut commands: Commands) {
    // Spawn a camera.
    let mut camera = commands.spawn(Camera2d);

    // On Wayland, it needs the LiveWallpaperCamera component
    // to be picked up by the plugin.
    #[cfg(target_os = "linux")]
    camera.insert(bevy_live_wallpaper::LiveWallpaperCamera);

    // ... spawn your scene entities here ...
    commands.spawn((
        Sprite::from_color(Color::srgb(0.15, 0.4, 0.85), Vec2::splat(1600.0)),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}
```

## Examples

The included examples are cross-platform.

- **Run on Wayland (Linux/BSD):**
  ```sh
  cargo run --features wayland --example 3d_shapes
  # or
  nix run github:yadokani389/bevy_live_wallpaper
  ```

- **Run on Windows:**
  ```sh
  cargo run --example 3d_shapes
  ```

## Credits & References

- [comavius/wayggle-bg](https://github.com/comavius/wayggle-bg): Wayland
  layer-shell setup, event dispatch structure, and general surface lifecycle
  management informed the design of this plugin.
