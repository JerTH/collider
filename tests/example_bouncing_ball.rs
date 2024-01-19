use collider::{*, transform::BoundTransformation};
use collider_core::results::TransformationResult;

#[derive(Debug, Clone, Default)]

// Vec2 is not a component, but it could be
struct Vec2 {
    x: f32,
    y: f32,
}

#[component]
struct Physics {
    pos: Vec2,
    vel: Vec2,
}

#[component]
struct Ball {
    radius: f32,
}

#[selection]
struct BallMovement {
    ball: Read<Ball>,
    phys: Write<Physics>,
}

fn ball_movement(data: BallMovement) -> TransformationResult {
    for ball in data {
        println!("bounce!")
    }

    Ok(())
}

#[test]
fn bouncing_ball() {
    let mut db = EntityDatabase::new();
    let mut phase = Phase::new();
    phase.add_transformation_new(ball_movement);
}
