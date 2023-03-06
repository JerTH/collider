/// A relational entity database which supports fast cacheable queries and quick parallel iteration

//use std::sync::Arc;
//use std::sync::MutexGuard;
//use std::sync::Mutex;
//use std::sync::LazyLock;
//use std::ops::Deref;
//use std::ops::DerefMut;
//use std::collections::hash_map::Entry;
//use std::collections::BTreeSet;
//use std::collections::HashMap;
//use std::any::Any;
//use std::fmt::Debug;
//use std::hash::Hash;

use crate::*;

// Convenience macros
#[macro_use]
mod macros {
    macro_rules! sty {
        ($a:ty) => {
            crate::id::StableTypeId::of::<$a>()
        };
    }
}

#[allow(unused_variables)]
#[test]
fn macros() {
    let b = component!(u32);
    let a = component!(u64);
    assert_ne!(a, b);
}

#[test]
fn stable_ids() {
    let stable_type_ids = vec![
        sty!(i8), sty!(i16), sty!(i32), sty!(i64), sty!(i128),
        sty!(u8), sty!(u16), sty!(u32), sty!(u64), sty!(u128),
        sty!(str), sty!(String)
    ];
    let mut unique = std::collections::HashSet::new();
    assert!(stable_type_ids.into_iter().all(|e| unique.insert(e)))
}

/// Vehicle Example
mod vehicle_example {
    use crate::xform::Rows;

    use super::*;

    #[derive(Debug, Default)]
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

    #[derive(Debug, Default)]
    pub struct Wheels {
        torque: u32,
        rpm: u32,
    }
    impl Component for Wheels {}

    #[derive(Debug)]
    pub struct Chassis {
        weight: u32,
    }
    impl Component for Chassis {}

    #[derive(Debug)]
    pub struct Engine {
        power: u32,
        torque: u32,
        rpm: u32,
        throttle: f32,
    }
    impl Component for Engine {}

    #[derive(Debug)]
    pub struct Transmission {
        gears: Vec<f32>,
        current: Option<f32>,
    }
    impl Component for Transmission {}
    
    struct DriveTrain {}
    impl Transformation for DriveTrain {
        type Data = (Read<Engine>, Read<Transmission>, Write<Wheels>);

        fn run(data: Rows<Self::Data>) -> Result<TransformSuccess, TransformError> {
            for (e, t, w) in data {
                
            }
            Ok(TransformSuccess)
        }
    }
    
    struct Drive {}
    impl Transformation for Drive {
        type Data = (Read<Physics>, Write<Transmission>, Write<Engine>);

        fn run(data: Rows<Self::Data>) -> Result<TransformSuccess, TransformError> {
            for (p, t, e) in data {
                
            }

            Ok(TransformSuccess)
        }        
    }

    struct WheelPhysics {}
    impl Transformation for WheelPhysics {
        type Data = (Read<Wheels>, Write<Physics>);

        fn run(data: Rows<Self::Data>) -> TransformResult {
            for (w, p) in data {
                
            }
            Ok(TransformSuccess)
        }
    }
    
    #[test]
    fn vehicle_example() {
        let mut db = EntityDatabase::new();
        db.register_component_debug_info::<Chassis>();
        db.register_component_debug_info::<Engine>();
        
        let v8_engine = Engine { power: 400, torque: 190, rpm: 5600, throttle: 0.0 };
        let diesel_engine = Engine { power: 300, torque: 650, rpm: 3200, throttle: 0.0 };

        let heavy_chassis = Chassis { weight: 7000, };
        let sport_chassis = Chassis { weight: 2200, };

        let five_speed = Transmission {
            gears: vec![2.95, 1.94, 1.34, 1.00, 0.73],
            current: None,
        };

        let ten_speed = Transmission {
            gears: vec![4.69, 2.98, 2.14, 1.76, 1.52, 1.27, 1.00, 0.85, 0.68, 0.63],
            current: None,
        };

        let sports_car = db.create();
        db.add_component(sports_car, v8_engine).unwrap();
        db.add_component(sports_car, five_speed).unwrap();
        db.add_component(sports_car, sport_chassis).unwrap();
        db.add_component(sports_car, Wheels::default()).unwrap();
        db.add_component(sports_car, Physics::new()).unwrap();

        let pickup_truck = db.create();
        db.add_component(pickup_truck, diesel_engine).unwrap();
        db.add_component(pickup_truck, ten_speed).unwrap();
        db.add_component(pickup_truck, heavy_chassis).unwrap();
        db.add_component(pickup_truck, Wheels::default()).unwrap();
        db.add_component(pickup_truck, Physics::new()).unwrap();

        let mut phase_one = Phase::new();
        phase_one.add_transformation(Drive{});

        phase_one.run_on(&db).unwrap();

        dbg!(&db);
    }

}
