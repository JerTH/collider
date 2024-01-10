use std::any::type_name;
use std::ops::Rem;
use std::collections::HashMap;
use std::fmt::Debug;

use collider::indexes::spatial::{Spatial, Nearby};
use collider::transform::{Read, Phase};
use collider::{*, transform::{Transformation, Write}};

use crate::indexes::spatial::SpatialIndex;

fn trace() {
    println!("enabling tracing");
    std::env::set_var("RUST_BACKTRACE", "1");

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .compact()
        .with_max_level(tracing::Level::DEBUG)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[test]
pub fn rocket_launch() {
    trace();

    // Create a new database
    let mut db = EntityDatabase::new();
    
    // Setup Indexes

    // Enables the [SpatialIndex], and associates it with the [Physics] component
    let spatial_index = SpatialIndex::new(1000.0);
    let _transform = db.enable_index::<SpatialIndex<Physics>>(spatial_index);

    // Create our first entity, this one will be a mission controller that will
    // tell our rocket(s) when to launch
    let mission_control = db.create().unwrap();
    db.add_component(mission_control, MissionController::new()).unwrap();
    db.add_component(mission_control, Radio::default()).unwrap();

    // Create our rocket(s), give them some random performance values
    let mut rockets = Vec::new();
    for i in 0..3u64 {
        let rand: f64 = (0..8).fold(3029487435683079979u64, |a, j: u64| 
              (a.overflowing_mul(a).0)
            ^ (j.overflowing_mul(j).0.overflowing_mul(j).0)
            ^ (i.overflowing_mul(a).0.overflowing_mul(i).0.overflowing_mul(j).0)
        ).rem(1000) as f64 / 1000.0;

        let rocket = db.create().unwrap();
        db.add_component(rocket, Physics::default()).unwrap();
        db.add_component(rocket, FuelTank::new(100.0, 100.0, 0.5)).unwrap();
        db.add_component(rocket, Avionics::default()).unwrap();
        db.add_component(rocket, Radio::default()).unwrap();
        db.add_component(rocket, MainEngine { efficiency: rand, max_thrust: 900.0 * rand + 100.0, throttle: 0.0, mass: 10.0  }).unwrap();
        rockets.push(rocket)
    }

    // Setup our transformation phase. When a phase is run on a database, its
    // transformations are analyzed and anything that can run in parallel
    // automatically does so, but only the contents of one phase are run at
    // a time. This means that if for example you want UI to be guaranteed to render
    // after everything else, you can simply put any UI rendering transformations
    // into a separate phase and run it after everything else.
    let mut rocket_sim = Phase::new();
    rocket_sim.add_transformation(RadioSystem {});
    rocket_sim.add_transformation(RocketSystem {});
    rocket_sim.add_transformation(PhysicsSystem {});
    rocket_sim.add_transformation(MissionControlSystem {});
    rocket_sim.add_transformation(RocketAvionicsSystem {});
    
    let mut loops = 0;
    loop {
        loops += 1;

        rocket_sim.run_on(&db).unwrap();

        if loops > 5 {
            break;
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Physics {
    altitude: f64,
    velocity: f64,
    acceleration: f64,
    net_mass: f64,
    net_force: f64,
}
impl Component for Physics {}

impl Spatial for Physics {
    type Vector = (f64, f64, f64);
    type Scalar = f64;

    fn position(&self) -> Self::Vector { (0.0, 0.0, self.altitude) }
    fn size_radius(&self) -> Self::Scalar { 0.0 }
}

#[derive(Default, Debug, Clone)]
pub struct FuelTank {
    max_capacity: f64,
    cur_capacity: f64,
    dry_mass: f64,
    wet_mass: f64,
}
impl Component for FuelTank {}

impl FuelTank {
    pub fn new(fuel_amount: f64, dry_mass: f64, fuel_mass: f64) -> Self {
        Self {
            max_capacity: fuel_amount,
            cur_capacity: fuel_amount,
            dry_mass,
            wet_mass: dry_mass + (fuel_mass * fuel_amount),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MainEngine {
    efficiency: f64,
    max_thrust: f64,
    throttle: f64,
    mass: f64,
}

impl Component for MainEngine {}

/// Override [Default] for components where certain fields
/// default value must be set to something sensible to avoid crashes
/// or instability. In this case a mass of zero is physically
/// impossible and will cause division by zero, so we set it to 1.0
impl Default for MainEngine {
    fn default() -> Self {
        Self {
            efficiency: 0.85,
            max_thrust: 400.0,
            throttle: Default::default(),
            mass: 1.0,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AvionicsState {
    #[default]
    PreLaunch,
    Launch,
    Abort,
}

#[derive(Default, Debug, Clone)]
pub struct Avionics {
    state: AvionicsState
}

impl Component for Avionics {}

#[derive(Default, Debug, Clone)]
pub struct MissionController {
    countdown: u32,
    launched: bool,
}

impl Component for MissionController {}

impl MissionController {
    fn new() -> Self {
        Self {
            countdown: 3,
            launched: false,
        }
    }
}

#[derive(Default, Debug, Clone)]
enum RadioSignal {
    Message(String),

    #[default]
    Noise,
}

#[derive(Default, Debug, Clone)]
pub struct Radio {
    ch: usize,
    tx: RadioSignal,
    rx: RadioSignal,
}

impl Component for Radio {}

struct PhysicsSystem {}
impl Transformation for PhysicsSystem {
    type Data = Write<Physics>;

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        for (physics,) in data {
            physics.acceleration = physics.net_force / physics.net_mass;
            physics.velocity = f64::max(0.0, physics.velocity + physics.acceleration);
            physics.altitude = f64::max(0.0, physics.altitude + physics.velocity);
            physics.net_force = 0.0;
        }
        Ok(())
    }
}

struct RocketSystem {}
impl Transformation for RocketSystem {
    type Data = (Read<Avionics>, Write<FuelTank>, Write<MainEngine>, Write<Physics>);

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        for (avionics, fueltank, engine, physics) in data {
            if avionics.state == AvionicsState::PreLaunch {
                continue;
            }
            
            let fuel_available = if fueltank.cur_capacity > 0.0 {
                let fuel_available = f64::max(0.0, f64::min(1.0 / engine.efficiency, fueltank.cur_capacity));
                fueltank.cur_capacity -= fuel_available;
                fuel_available
            } else {
                0.0
            };

            let thrust_now = fuel_available * engine.max_thrust * engine.throttle;
            let fuel_mass = (fueltank.wet_mass - fueltank.dry_mass) * (fueltank.cur_capacity / fueltank.max_capacity);
            physics.net_mass = f64::max(1.0, engine.mass + (fueltank.dry_mass + fuel_mass));
            physics.net_force = thrust_now;
        }
        Ok(())
    }
}

struct RocketAvionicsSystem {}
impl Transformation for RocketAvionicsSystem {
    type Data = (Write<Radio>, Write<Avionics>, Write<MainEngine>, Read<Physics>);

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        for (radio, avionics, engine, _physics) in data {
            match &radio.rx {
                RadioSignal::Message(message) => {
                    match message.as_str() {
                        "Launch!" => {
                            if avionics.state != AvionicsState::Launch {
                                //println!("Rocket: Guidance is internal!");
                            }

                            avionics.state = AvionicsState::Launch;
                            engine.throttle = 1.0;
                        },
                        "Abort!" => {
                            if avionics.state != AvionicsState::Abort {
                                //println!("Rocket: Abort Mission! Abort Mission!!!");
                            }

                            avionics.state = AvionicsState::Abort;
                            engine.throttle = 0.0;
                        },
                        _ => {
                            // Control, please repeat last?
                        }
                    }
                },
                RadioSignal::Noise => {
                    continue;
                },
            }

            if avionics.state == AvionicsState::Launch {
                //radio.tx = RadioSignal::Message(format!("current altitude: {}", physics.altitude))
            }
        }
        Ok(())
    }
}

struct MissionControlSystem {}
impl Transformation for MissionControlSystem {
    type Data = (Write<MissionController>, Write<Radio>);

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        for (control, radio) in data {
            radio.ch = 0;
            if control.countdown > 0 {
                println!("{}!", control.countdown);
                radio.tx = RadioSignal::Noise;
                control.countdown -= 1;
            } else {
                if !control.launched {
                    println!("Mission Control: Launch!");
                    control.launched = true;
                    radio.tx = RadioSignal::Message(String::from("Launch!"));
                }
            }

            match &radio.rx {
                RadioSignal::Message(_) => {
                    radio.rx = RadioSignal::Noise;
                },
                RadioSignal::Noise => {
                    continue;
                },
            }
        }
        Ok(())
    }
}

struct RadioSystem {}
impl Transformation for RadioSystem {
    type Data = Write<Radio>;

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {       
        let mut channels: HashMap<usize, RadioSignal> = Default::default();

        for (radio,) in &data {
            if let RadioSignal::Message(message) = &radio.tx {
                channels.insert(radio.ch, RadioSignal::Message(message.clone()));
            }
            radio.tx = RadioSignal::Noise;
        }

        for (radio,) in &data {
            if let Some(signal) = channels.get(&radio.ch) {
                radio.rx = signal.clone();
            }
        }

        Ok(())
    }
}
