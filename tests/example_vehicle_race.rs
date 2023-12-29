use std::fmt::Debug;
use collider::database::reckoning::Command;
use collider::transform::{Phase, Read};
use collider::{
    database::{Component, EntityDatabase},
    transform::{Transformation, Write},
    *,
};

#[allow(dead_code)]
#[allow(unused_assignments)]
#[allow(unused_variables)]

#[derive(Debug, Default, Clone)]
pub struct Physics {
    pos: f64,
    vel: f64,
    acc: f64,
}

impl Component for Physics {}
impl Physics {
    fn new() -> Self {
        Default::default()
    }
}

#[derive(Debug, Default, Clone)]
pub struct Wheels {
    friction: f64,
    torque: f64,
    rpm: f64,
    radius: f64,
}
impl Component for Wheels {}

#[derive(Debug, Default, Clone)]
pub struct Chassis {
    weight: f64,
}
impl Component for Chassis {}

#[derive(Debug, Default, Clone)]
pub struct Engine {
    power: f64,
    torque: f64,
    rpm: f64,
    maxrpm: f64,
    throttle: f64,
}
impl Component for Engine {}

#[derive(Debug, Default, Clone)]
pub struct Transmission {
    gears: Vec<f64>,
    current_gear: Option<usize>,
}
impl Component for Transmission {}

#[derive(Debug, Default, Clone)]
pub enum Driver {
    #[default]
    SlowAndSteady,
    PedalToTheMetal,
}
impl Component for Driver {}

#[derive(Debug, Default, Clone)]
pub struct VehicleName {
    name: String,
}
impl Component for VehicleName {}

struct DriveTrain;
impl Transformation for DriveTrain {
    type Data = (Read<Transmission>, Write<Engine>, Write<Wheels>);

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        println!("running drive-train transformation");

        // calculate engine torque & rpm

        for (transmission, engine, wheels) in data {
            if let Some(gear) = transmission.current_gear {
                if let Some(gear_ratio) = transmission.gears.get(gear) {
                    wheels.torque = engine.torque * gear_ratio
                }
            }
        }
        Ok(())
    }
}

struct DriverInput;
impl Transformation for DriverInput {
    type Data = (Read<Driver>, Write<Transmission>, Write<Engine>);

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        println!("running driver transformation");

        for (driver, transmission, engine) in data {
            match driver {
                Driver::SlowAndSteady => match engine.rpm as u64 {
                    0..=4999 => match transmission.current_gear {
                        Some(_) => engine.throttle = 0.4,
                        None => transmission.current_gear = Some(0),
                    },
                    5000.. => {
                        engine.throttle = 0.0;
                        if let Some(gear) = transmission.current_gear {
                            if gear < transmission.gears.len() {
                                transmission.current_gear = Some(gear + 1)
                            }
                        }
                    }
                },
                Driver::PedalToTheMetal => match engine.rpm as u64 {
                    0..=4999 => match transmission.current_gear {
                        Some(_) => engine.throttle = 1.0,
                        None => transmission.current_gear = Some(0),
                    },
                    5000.. => {
                        engine.throttle = 0.0;
                        if let Some(gear) = transmission.current_gear {
                            if gear < transmission.gears.len() {
                                transmission.current_gear = Some(gear + 1)
                            }
                        }
                    }
                },
            }
        }

        Ok(())
    }
}

struct WheelPhysics;
impl Transformation for WheelPhysics {
    type Data = (Write<Wheels>, Read<Chassis>, Write<Physics>);

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        println!("running wheel physics transformation");

        for (wheels, chassis, physics) in data {
            physics.acc =
                wheels.torque / f64::max(wheels.radius, 1.0) / f64::max(chassis.weight, 1.0);
            physics.vel += physics.acc;
            physics.pos += physics.vel;
            wheels.rpm = physics.vel * (60.0 / (2.0 * 3.14159) * wheels.radius);
        }
        Ok(())
    }
}

struct PrintVehicleStatus;
impl Transformation for PrintVehicleStatus {
    type Data = (Read<VehicleName>, Read<Physics>, Read<Engine>);

    fn run(data: transform::Rows<Self::Data>) -> transform::TransformationResult {
        for (name, phys, engine) in data {
            println!(
                "{:14}: {:5}m @ {:5}m/s ({:5}rpm)",
                name.name, phys.pos, phys.vel, engine.rpm
            );
        }

        Ok(())
    }
}

#[test]
fn vehicle_example() {
    std::env::set_var("RUST_BACKTRACE", "1");

    let mut db = EntityDatabase::new();

    // Define some components from data, these could be loaded from a file
    let v8_engine = Engine {
        power: 400.0,
        torque: 190.0,
        rpm: 0.0,
        maxrpm: 5600.0,
        throttle: 0.0,
    };
    let diesel_engine = Engine {
        power: 300.0,
        torque: 650.0,
        rpm: 0.0,
        maxrpm: 3200.0,
        throttle: 0.0,
    };
    let economy_engine = Engine {
        power: 103.0,
        torque: 90.0,
        rpm: 0.0,
        maxrpm: 6000.0,
        throttle: 0.0,
    };

    let heavy_chassis = Chassis { weight: 7000.0 };
    let sport_chassis = Chassis { weight: 2200.0 };
    let cheap_chassis = Chassis { weight: 3500.0 };

    let five_speed = Transmission {
        gears: vec![2.95, 1.94, 1.34, 1.00, 0.73],
        current_gear: None,
    };

    let ten_speed = Transmission {
        gears: vec![4.69, 2.98, 2.14, 1.76, 1.52, 1.27, 1.00, 0.85, 0.68, 0.63],
        current_gear: None,
    };

    // Build the entities from the components we choose
    // This can be automated from data
    let sports_car = db.create().unwrap();
    db.add_component(sports_car, five_speed.clone()).unwrap();
    db.add_component(sports_car, v8_engine.clone()).unwrap();

    db.add_component(sports_car, sport_chassis).unwrap();
    db.add_component(sports_car, Wheels::default()).unwrap();
    db.add_component(sports_car, Physics::new()).unwrap();
    db.add_component(sports_car, Driver::PedalToTheMetal)
        .unwrap();

    let pickup_truck = db.create().unwrap();
    db.add_component(pickup_truck, v8_engine).unwrap();
    db.add_component(pickup_truck, ten_speed).unwrap();
    db.add_component(pickup_truck, heavy_chassis).unwrap();
    db.add_component(pickup_truck, Wheels::default()).unwrap();
    db.add_component(pickup_truck, Physics::new()).unwrap();
    db.add_component(pickup_truck, Driver::SlowAndSteady)
        .unwrap();

    // Let's swap the engine in the truck for something more heavy duty
    // Entities can only ever have a single component of a given type
    db.add_component(pickup_truck, diesel_engine).unwrap();

    let economy_car = db.create().unwrap();
    db.add_component(economy_car, economy_engine.clone())
        .unwrap();
    db.add_component(economy_car, cheap_chassis.clone())
        .unwrap();
    db.add_component(economy_car, five_speed.clone()).unwrap();

    // Lets name the 3 vehicles
    db.add_component(
        sports_car,
        VehicleName {
            name: String::from("Sports Car"),
        },
    )
    .unwrap();
    db.add_component(
        pickup_truck,
        VehicleName {
            name: String::from("Pickup Truck"),
        },
    )
    .unwrap();
    db.add_component(
        economy_car,
        VehicleName {
            name: String::from("Economy Car"),
        },
    )
    .unwrap();

    // Create a simulation phase. It is important to note that things
    // that happen in a single phase are unordered. If it is important
    // for a certain set of transformations to happen before or after
    // another set of transformations, you must break them into distinct
    // phases. Each phase will run sequentially, and each transformation
    // within a phase will (try to) run in parallel
    let mut race = Phase::new();
    race.add_transformation(DriveTrain);
    race.add_transformation(WheelPhysics);
    race.add_transformation(DriverInput);
    race.add_transformation(PrintVehicleStatus);

    // The simulation loop. Here we can see that, fundamentally, the
    // simulation is nothing but a set of transformations on our
    // dataset run over and over. By adding more components and
    // transformations to the simulation we expand its capabilities
    // while automatically leveraging parallelism
    let mut loops = 0;
    loop {
        race.run_on(&db).unwrap();

        // Here we allow the database to communicate back with the
        // simulation loop through commands
        while let Some(command) = db.query_commands() {
            match command {
                Command::Quit => break,
            }
        }

        if loops < 3 {
            loops += 1;
        } else {
            println!("Exiting!");
            break;
        }
    }
}
