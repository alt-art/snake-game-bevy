#![warn(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    clippy::perf,
    clippy::str_to_string
)]
#![allow(
    clippy::multiple_crate_versions,
    clippy::needless_pass_by_value,
    clippy::cast_precision_loss
)]

use std::{
    fmt::{Debug, Formatter},
    path::{Path, PathBuf},
    time::Duration,
};

use bevy::{
    asset::io::embedded::EmbeddedAssetRegistry,
    core::FrameCount,
    prelude::*,
    window::{PrimaryWindow, WindowMode}, app::AppExit,
};

use bevy_pixel_camera::{PixelCameraPlugin, PixelZoom};

use rand::prelude::*;

const SPRITE_SIZE: f32 = 16.0;
const TABLE_WIDTH: i32 = 22;
const TABLE_HEIGHT: i32 = 22;
const WALL_WIDTH: i32 = TABLE_WIDTH - 2;
const WALL_HEIGHT: i32 = TABLE_HEIGHT - 2;

fn fullscreen_system(
    keyboard_input: Res<Input<KeyCode>>,
    mut window_query: Query<&mut Window, With<PrimaryWindow>>,
) {
    let mut window = window_query.single_mut();
    if keyboard_input.just_pressed(KeyCode::F) {
        if window.mode == WindowMode::Fullscreen {
            window.mode = WindowMode::Windowed;
        } else {
            window.mode = WindowMode::Fullscreen;
        }
    }
}

fn exit_on_esc_system(keyboard_input: Res<Input<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if keyboard_input.just_pressed(KeyCode::Escape) {
        exit.send(AppExit);
    }
}

macro_rules! embedded_asset {
    ($embedded:ident, $path:expr) => {
        $embedded.insert_asset(
            PathBuf::new(),
            Path::new($path),
            include_bytes!(concat!("../assets/", $path)),
        );
    };
}

struct EmbeddedAssetsPlugin;

impl Plugin for EmbeddedAssetsPlugin {
    fn build(&self, app: &mut App) {
        let embedded = app.world.resource_mut::<EmbeddedAssetRegistry>();
        embedded_asset!(embedded, "sprites.png");
    }
}

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.1607, 0.1647, 0.1686)))
        .add_state::<GameState>()
        .add_plugins((
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Snake Game".to_owned(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
            PixelCameraPlugin,
            EmbeddedAssetsPlugin,
        ))
        .add_systems(Startup, (setup_camera, setup_resources))
        .add_systems(
            OnEnter(GameState::Playing),
            (setup_snake, setup_apple, setup_glass, setup_wall),
        )
        .add_systems(
            Update,
            (
                move_snake,
                draw_snake_sprites,
                draw_apple_sprite,
                tail_collision,
                wall_collision,
            )
                .run_if(in_state(GameState::Playing)),
        )
        .add_systems(PostUpdate, eat_apple.run_if(in_state(GameState::Playing)))
        .add_systems(OnEnter(GameState::GameOver), setup_death_animation)
        .add_systems(
            Update,
            death_animation.run_if(in_state(GameState::GameOver)),
        )
        .add_systems(OnExit(GameState::GameOver), clear_game_scene)
        .add_systems(Update, (fullscreen_system, exit_on_esc_system))
        .run();
}

#[derive(PartialEq, Eq, Hash, Default, States, Debug, Clone, Copy)]
enum GameState {
    #[default]
    Playing,
    GameOver,
}

/// A simple queue implementation that uses a fixed-size array and wraps around.
/// When the queue is full, the oldest value is overwritten.
/// This is used to store the last few directions the player has pressed.
///
/// This is necessary because the player can press two directions in one frame and that would cause the snake to only move one tile instead of two.
///
/// Why not use a `VecDeque`?
///
/// `VecDeque` does not have peeking, which is necessary to check if the player is trying to turn back on itself.
struct Queue<T> {
    head: usize,
    tail: usize,
    data: [Option<T>; 10],
}

impl<T: Copy> Queue<T> {
    /// Creates a new empty queue.
    #[inline]
    const fn new() -> Self {
        Self {
            head: 0,
            tail: 0,
            data: [None; 10],
        }
    }

    /// Pushes a value to the back of the queue.
    /// If the queue is full, the oldest value is overwritten.
    #[inline]
    fn push(&mut self, value: T) {
        self.data[self.tail] = Some(value);
        self.tail = (self.tail + 1) % self.data.len();
    }

    /// Pops a value from the front of the queue.
    /// If the queue is empty, `None` is returned.
    #[inline]
    fn pop(&mut self) -> Option<T> {
        let value = self.data[self.head];
        if value.is_none() {
            value?;
        }
        self.data[self.head] = None;
        self.head = (self.head + 1) % self.data.len();
        value
    }

    /// Returns the value at the front of the queue without removing it.
    /// If the queue is empty, `None` is returned.
    #[inline]
    const fn peek(&self) -> Option<T> {
        self.data[self.head]
    }
}

impl<T: Debug> Debug for Queue<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.data.iter()).finish()
    }
}

impl<T: Copy> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Resource)]
struct Zoom(i32);

#[derive(Resource, Default)]
struct KeyboardDirection(Queue<SnakeDirection>);

#[derive(Resource)]
struct TextureAtlasHandle(Handle<TextureAtlas>);

#[derive(Component)]
struct Apple {
    x: i32,
    y: i32,
}

#[derive(Component)]
struct Wall {
    x: i32,
    y: i32,
}

#[derive(Component)]
struct Glass;

#[derive(Clone, Copy, PartialEq, Debug)]
enum SnakeDirection {
    Up = 2,
    Down = 3,
    Right = 4,
    Left = 5,
}

#[derive(Component)]
struct Snake {
    x: i32,
    y: i32,
    direction: SnakeDirection,
    tail: Vec<Entity>,
}

#[derive(Component)]
struct Tail {
    x: i32,
    y: i32,
}

enum TailSprite {
    Horizontal = 6,
    Vertical = 7,
    DownRight = 8,
    DownLeft = 9,
    UpRight = 10,
    UpLeft = 11,
    TailEndLeft = 12,
    TailEndUp = 13,
    TailEndDown = 14,
    TailEndRight = 15,
}

#[derive(Component)]
struct AnimationTimer(Timer);

#[derive(Component)]
struct MoveTimer(Timer);

impl Default for MoveTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(0.3, TimerMode::Repeating))
    }
}

enum WallSprite {
    TopBottom = 16,
    Left = 17,
    Right = 18,
    TopLeft = 19,
    TopRight = 20,
    BottomLeft = 21,
    BottomRight = 22,
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2dBundle {
            transform: Transform::from_xyz(
                (TABLE_WIDTH as f32 * SPRITE_SIZE) / 2.0,
                (TABLE_HEIGHT as f32 * SPRITE_SIZE) / 2.0,
                0.0,
            ),
            ..Default::default()
        },
        PixelZoom::Fixed(2),
    ));
}

fn setup_resources(
    mut commands: Commands,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
    asset_server: Res<AssetServer>,
) {
    let texture_handle = asset_server.load("embedded://sprites.png");
    let texture_atlas = TextureAtlas::from_grid(
        texture_handle,
        Vec2::new(SPRITE_SIZE, SPRITE_SIZE),
        26,
        1,
        None,
        None,
    );
    let texture_atlas_handle = texture_atlases.add(texture_atlas);
    commands.insert_resource(FrameCount(0));
    commands.insert_resource(KeyboardDirection::default());
    commands.insert_resource(TextureAtlasHandle(texture_atlas_handle));
    commands.insert_resource(Zoom(2));
    commands.spawn(AnimationTimer(Timer::from_seconds(
        0.1,
        TimerMode::Repeating,
    )));
    commands.spawn(MoveTimer::default());
}

fn setup_glass(mut commands: Commands, texture_atlas_handle: Res<TextureAtlasHandle>) {
    let texture_atlas_handle = &texture_atlas_handle.0;
    (0..TABLE_WIDTH * TABLE_HEIGHT).for_each(|i| {
        let x = i % TABLE_WIDTH;
        let y = i / TABLE_HEIGHT;
        commands.spawn((
            SpriteSheetBundle {
                texture_atlas: texture_atlas_handle.clone(),
                transform: Transform::from_translation(Vec3::new(
                    ((x) as f32) * SPRITE_SIZE,
                    ((y) as f32) * SPRITE_SIZE,
                    -100.0,
                )),
                ..Default::default()
            },
            Glass,
        ));
    });
}

fn setup_wall(mut commands: Commands, texture_atlas_handle: Res<TextureAtlasHandle>) {
    let texture_atlas_handle = &texture_atlas_handle.0;
    (0..WALL_WIDTH * WALL_HEIGHT).for_each(|i| {
        let x = i % WALL_WIDTH;
        let y = i / WALL_HEIGHT;
        if x == 0 || x == WALL_WIDTH - 1 || y == 0 || y == WALL_HEIGHT - 1 {
            commands.spawn((
                Wall { x: x + 1, y: y + 1 },
                SpriteSheetBundle {
                    texture_atlas: texture_atlas_handle.clone(),
                    transform: Transform::from_translation(Vec3::new(
                        ((x + 1) as f32) * SPRITE_SIZE,
                        ((y + 1) as f32) * SPRITE_SIZE,
                        0.0,
                    )),
                    sprite: if x == 0 {
                        if y == 0 {
                            TextureAtlasSprite::new(WallSprite::BottomLeft as usize)
                        } else if y == WALL_HEIGHT - 1 {
                            TextureAtlasSprite::new(WallSprite::TopLeft as usize)
                        } else {
                            TextureAtlasSprite::new(WallSprite::Left as usize)
                        }
                    } else if x == WALL_WIDTH - 1 {
                        if y == 0 {
                            TextureAtlasSprite::new(WallSprite::BottomRight as usize)
                        } else if y == WALL_HEIGHT - 1 {
                            TextureAtlasSprite::new(WallSprite::TopRight as usize)
                        } else {
                            TextureAtlasSprite::new(WallSprite::Right as usize)
                        }
                    } else {
                        TextureAtlasSprite::new(WallSprite::TopBottom as usize)
                    },
                    ..Default::default()
                },
            ));
        }
    });
}

fn setup_apple(mut commands: Commands, texture_atlas_handle: Res<TextureAtlasHandle>) {
    let texture_atlas_handle = &texture_atlas_handle.0;
    commands.spawn((
        SpriteSheetBundle {
            texture_atlas: texture_atlas_handle.clone(),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            sprite: TextureAtlasSprite::new(1),
            ..Default::default()
        },
        Apple {
            x: thread_rng().gen_range(2..WALL_WIDTH - 1),
            y: thread_rng().gen_range(2..WALL_HEIGHT - 1),
        },
    ));
}

fn setup_snake(mut commands: Commands, texture_atlas_handle: Res<TextureAtlasHandle>) {
    let texture_atlas_handle = &texture_atlas_handle.0;

    let tail_entities = (1..=3)
        .map(|i| {
            let texture_atlas_handle = texture_atlas_handle.clone();
            commands
                .spawn((
                    Tail {
                        x: -i + TABLE_WIDTH / 2,
                        y: TABLE_HEIGHT / 2,
                    },
                    SpriteSheetBundle {
                        texture_atlas: texture_atlas_handle,
                        transform: Transform::from_translation(Vec3 {
                            x: 0.0,
                            y: 0.0,
                            z: -(i as f32),
                        }),
                        ..Default::default()
                    },
                ))
                .id()
        })
        .collect();

    commands.spawn((
        SpriteSheetBundle {
            texture_atlas: texture_atlas_handle.clone(),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..Default::default()
        },
        Snake {
            x: TABLE_WIDTH / 2,
            y: TABLE_HEIGHT / 2,
            direction: SnakeDirection::Right,
            tail: tail_entities,
        },
    ));
}

fn move_snake(
    keyboard_input: Res<Input<KeyCode>>,
    mut keyboard_direction: ResMut<KeyboardDirection>,
    mut snake_query: Query<&mut Snake>,
    mut tail_query: Query<&mut Tail>,
    mut move_timer_query: Query<&mut MoveTimer>,
    time: Res<Time>,
) {
    let mut timer = move_timer_query.single_mut();
    let mut snake = snake_query.single_mut();
    if keyboard_input.just_pressed(KeyCode::Up)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Down)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Up)
    {
        keyboard_direction.0.push(SnakeDirection::Up);
    }
    if keyboard_input.just_pressed(KeyCode::Down)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Down)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Up)
    {
        keyboard_direction.0.push(SnakeDirection::Down);
    }
    if keyboard_input.just_pressed(KeyCode::Left)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Left)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Right)
    {
        keyboard_direction.0.push(SnakeDirection::Left);
    }
    if keyboard_input.just_pressed(KeyCode::Right)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Left)
        && keyboard_direction.0.peek() != Some(SnakeDirection::Right)
    {
        keyboard_direction.0.push(SnakeDirection::Right);
    }

    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    if let Some(direction) = keyboard_direction.0.pop() {
        if !(snake.direction == SnakeDirection::Up && direction == SnakeDirection::Down
            || snake.direction == SnakeDirection::Down && direction == SnakeDirection::Up
            || snake.direction == SnakeDirection::Left && direction == SnakeDirection::Right
            || snake.direction == SnakeDirection::Right && direction == SnakeDirection::Left)
        {
            snake.direction = direction;
        }
    }

    let mut prev_snake_x = snake.x;
    let mut prev_snake_y = snake.y;
    match snake.direction {
        SnakeDirection::Up => snake.y += 1,
        SnakeDirection::Down => snake.y -= 1,
        SnakeDirection::Left => snake.x -= 1,
        SnakeDirection::Right => snake.x += 1,
    }
    for entity in &snake.tail {
        let mut tail = tail_query.get_mut(*entity).unwrap();
        let prev_tail_x = tail.x;
        let prev_tail_y = tail.y;
        tail.x = prev_snake_x;
        tail.y = prev_snake_y;
        prev_snake_x = prev_tail_x;
        prev_snake_y = prev_tail_y;
    }
}

fn draw_snake_sprites(
    mut snake_query: Query<(&Snake, &mut Transform, &mut TextureAtlasSprite)>,
    mut tail_query: Query<(&Tail, &mut Transform, &mut TextureAtlasSprite), Without<Snake>>,
) {
    let (snake, mut transform, mut sprite) = snake_query.single_mut();
    transform.translation.x = (snake.x as f32) * SPRITE_SIZE;
    transform.translation.y = (snake.y as f32) * SPRITE_SIZE;
    let mut prev_tail_x = snake.x;
    let mut prev_tail_y = snake.y;
    let entities = &snake.tail;
    for i in 0..entities.len() {
        let (next_tail_x, next_tail_y) = if i + 1 < entities.len() {
            if let Ok((next_tail, _, _)) = tail_query.get(entities[i + 1]) {
                (next_tail.x, next_tail.y)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        sprite.index = snake.direction as usize;

        if let Ok((tail, mut transform, mut sprite)) = tail_query.get_mut(entities[i]) {
            transform.translation.x = (tail.x as f32) * SPRITE_SIZE;
            transform.translation.y = (tail.y as f32) * SPRITE_SIZE;
            if i == entities.len() - 1 {
                match (prev_tail_x - tail.x, prev_tail_y - tail.y) {
                    (0, 1) => sprite.index = TailSprite::TailEndUp as usize,
                    (0, -1) => sprite.index = TailSprite::TailEndDown as usize,
                    (1, 0) => sprite.index = TailSprite::TailEndRight as usize,
                    (-1, 0) => sprite.index = TailSprite::TailEndLeft as usize,
                    _ => (),
                }
            } else {
                match (
                    prev_tail_x - tail.x,
                    prev_tail_y - tail.y,
                    next_tail_x - tail.x,
                    next_tail_y - tail.y,
                ) {
                    (0, 1, 0, -1) | (0, -1, 0, 1) => sprite.index = TailSprite::Vertical as usize,
                    (1, 0, -1, 0) | (-1, 0, 1, 0) => sprite.index = TailSprite::Horizontal as usize,
                    (1, 0, 0, 1) | (0, 1, 1, 0) => sprite.index = TailSprite::UpRight as usize,
                    (-1, 0, 0, 1) | (0, 1, -1, 0) => sprite.index = TailSprite::UpLeft as usize,
                    (1, 0, 0, -1) | (0, -1, 1, 0) => sprite.index = TailSprite::DownRight as usize,
                    (-1, 0, 0, -1) | (0, -1, -1, 0) => sprite.index = TailSprite::DownLeft as usize,
                    _ => (),
                }
            }
            prev_tail_x = tail.x;
            prev_tail_y = tail.y;
        }
    }
}

fn draw_apple_sprite(mut apple_query: Query<(&Apple, &mut Transform)>) {
    let (apple, mut transform) = apple_query.single_mut();
    transform.translation.x = (apple.x as f32) * SPRITE_SIZE;
    transform.translation.y = (apple.y as f32) * SPRITE_SIZE;
}

fn eat_apple(
    mut commands: Commands,
    mut snake_query: Query<&mut Snake>,
    mut apple_query: Query<(&Apple, Entity)>,
    tail_query: Query<&Tail>,
    texture_atlas_handle: Res<TextureAtlasHandle>,
    mut move_timer_query: Query<&mut MoveTimer>,
) {
    let mut snake = snake_query.single_mut();
    let (apple, entity) = apple_query.single_mut();
    if snake.x == apple.x && snake.y == apple.y {
        commands.entity(entity).despawn();
        let mut tail = snake.tail.clone();
        let texture_atlas = &texture_atlas_handle.0;
        let last_tail = tail_query.get(*tail.last().unwrap()).unwrap();
        tail.push(
            commands
                .spawn((
                    Tail {
                        x: last_tail.x,
                        y: last_tail.y,
                    },
                    SpriteSheetBundle {
                        texture_atlas: texture_atlas.clone(),
                        transform: Transform::from_translation(Vec3 {
                            x: 0.0,
                            y: 0.0,
                            z: -(snake.tail.len() as f32),
                        }),
                        ..Default::default()
                    },
                ))
                .id(),
        );
        snake.tail = tail;
        commands.spawn((
            SpriteSheetBundle {
                texture_atlas: texture_atlas.clone(),
                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                sprite: TextureAtlasSprite::new(1),
                ..Default::default()
            },
            Apple {
                x: thread_rng().gen_range(2..WALL_WIDTH),
                y: thread_rng().gen_range(2..WALL_HEIGHT),
            },
        ));
        let mut timer = move_timer_query.single_mut();
        let duration = timer.0.duration().as_secs_f32() * 0.95;
        timer.0.set_duration(Duration::from_secs_f32(duration));
    }
}

fn tail_collision(
    mut snake_query: Query<&Snake>,
    tail_query: Query<&Tail>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    let snake = snake_query.single_mut();
    if tail_query.iter().any(|t| t.x == snake.x && t.y == snake.y) {
        game_state.set(GameState::GameOver);
    }
}

fn wall_collision(
    mut snake_query: Query<&Snake>,
    wall_query: Query<&Wall>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    let snake = snake_query.single_mut();
    if wall_query.iter().any(|w| w.x == snake.x && w.y == snake.y) {
        game_state.set(GameState::GameOver);
    }
}

fn setup_death_animation(
    mut tail_query: Query<&mut TextureAtlasSprite, With<Tail>>,
    snake_query: Query<Entity, With<Snake>>,
    mut commands: Commands,
) {
    for mut sprite in &mut tail_query {
        sprite.index = 23;
    }
    for snake_entity in snake_query.iter() {
        commands.entity(snake_entity).despawn();
    }
}

fn death_animation(
    mut tail_query: Query<&mut TextureAtlasSprite, With<Tail>>,
    mut animation_timer_query: Query<&mut AnimationTimer>,
    mut move_timer_query: Query<&mut MoveTimer>,
    mut game_state: ResMut<NextState<GameState>>,
    time: Res<Time>,
) {
    let mut timer = animation_timer_query.single_mut();
    if timer.0.tick(time.delta()).just_finished() {
        for mut sprite in &mut tail_query {
            if sprite.index < 25 {
                sprite.index += 1;
            } else {
                let mut timer = move_timer_query.single_mut();
                timer.0.set_duration(Duration::from_secs_f32(0.3));
                game_state.set(GameState::Playing);
            }
        }
    }
}

fn clear_game_scene(
    wall_query: Query<Entity, With<Wall>>,
    apple_query: Query<Entity, With<Apple>>,
    glass_query: Query<Entity, With<Glass>>,
    tail_query: Query<Entity, With<Tail>>,
    snake_query: Query<Entity, With<Snake>>,
    mut commands: Commands,
) {
    for wall_entity in wall_query.iter() {
        commands.entity(wall_entity).despawn();
    }
    for apple_entity in apple_query.iter() {
        commands.entity(apple_entity).despawn();
    }
    for glass_entity in glass_query.iter() {
        commands.entity(glass_entity).despawn();
    }
    for tail_entity in tail_query.iter() {
        commands.entity(tail_entity).despawn();
    }
    for snake_entity in snake_query.iter() {
        commands.entity(snake_entity).despawn();
    }
}
