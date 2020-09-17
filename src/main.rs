use failure::Error;
use log::{info, warn};
// use failure::{bail, Error};
use serde_json::value::{
    Value,
    // Value::{Bool as VBool, String as VString},
};
use std::thread;
use std::time::Duration;

use kubos_app::{query, ServiceConfig};

static QUERY_TIMEOUT: u64 = 1;

// returns true if this program is run with RBF
fn check_rbf() -> bool {
    let args = std::env::args();
    let mut rbf = false;

    // don't include the program name in the arguments
    let mut arg_iter = args.into_iter();
    arg_iter.next();
    for arg in arg_iter {
        match &arg[..] {
            "-r" => rbf = true,
            _ => panic!(format!("Invalid argument '{}'", arg)),
        }
    }
    rbf
}

// returns true if the system status matches the given arguments
// Let sys_status = "DEPLOYED", if ture, retrun 1, otherwise returns 0
fn check_system_status(response: &Value, sys_status: &str) -> bool {
    let data = response.get("data").and_then(|v| v.get("deploymentStatus"));
    // let mut status = true;
    let status = data
        .and_then(|v| v.get("status"))
        .and_then(|v| match v {
            VString(s) => Some(s == sys_status),
            _ => None,
        })
        .unwrap_or(false);
    status
}

// // returns true if all antenna statuses match the given arguments
// fn check_antenna_status(
//     response: &Value,
//     //   active: bool,
//     // deployed: bool,
//     //   stopped_time: bool,
// ) -> bool {
//     println!("Checking antenna status...");

//     let undeployed: bool = true;

//     let data = response.get("data").and_then(|v| v.get("deploymentStatus"));
//     let mut status = false;
//     for i in 1..5 {
//         status |= data
//             .and_then(|v| v.get(&format!("ant{}NotDeployed", i)))
//             .and_then(|v| match v {
//                 VBool(b) => Some(*b == undeployed),
//                 _ => None,
//             })
//             .unwrap_or(false);
//     }
//     status
// }

// returns true if the system is stowed and not armed
fn check_stowed(ant_service: &ServiceConfig) -> bool {
    println!("Checking the stowed status");

    // let query_initial_status = "query{deployment_status}";

    let query_initial_status = format!(
        r#"
        {{
            deploymentStatus {{
                status,
                sysArmed,
                sysBurnActive,
                ant1Active,
                ant2Active,
                ant3Active,
                ant4Active,
                ant1NotDeployed,
                ant1StoppedTime,
                ant2NotDeployed,
                ant2StoppedTime,
                ant3NotDeployed,
                ant3StoppedTime,
                ant4NotDeployed,
                ant4StoppedTime,
            }}
        }}
    "#,
    );

    match query(
        ant_service,
        // &query_initial_status[..],
        &query_initial_status,
        Some(Duration::from_secs(QUERY_TIMEOUT)),
    ) {
        Ok(msg) => {
            // let mut stowed = check_system_status(&msg, &"STOWED", false, false);
            let deployed = check_system_status(&msg, &"DEPLOYED");
            // stowed &= check_antenna_status(&msg, false, false, false);
            // stowed |= check_antenna_status(&msg);
            println!("If antenna is deployed: {:?}", deployed);
            deployed
        }
        // not sure if panicing is appropriate here
        Err(e) => {
            println!("Failed to contact antenna: {:?}", e);
            false
        }
    }
}

// arms the antenna, returning true on success
fn arm_antenna(ant_service: &ServiceConfig) -> bool {
    // let mutation_arm_antenna = "mutation {arm (state:ARM) {successerrors}}";
    let mutation_arm_antenna = format!(
        r#"
        mutation {{
            arm (state:ARM) {{
                success,
                errors
            }}
        }}
    "#,
    );

    match query(
        ant_service,
        &mutation_arm_antenna,
        Some(Duration::from_secs(QUERY_TIMEOUT)),
    ) {
        // Ok(msg) => check_system_status(&msg, &"STOWED", true, false),
        Ok(_) => true,
        Err(e) => {
            println!("Failed to arm antenna: {:?}", e);
            false
        }
    }
}

fn deploy_antenna(ant_service: &ServiceConfig) -> bool {
    println!("deploying the antenna...");

    // let mutation_deploy_antenna = "mutation{
    //     deploy(ant:ALL,force:true, time:10){
    //       success
    //     }
    //   }";

    let mutation_deploy_antenna = format!(
        r#"
        mutation {{
            deploy(ant:ALL,force:true, time:3){{
                success,
                errors
            }}
        }}
    "#,
    );
    match query(
        ant_service,
        &mutation_deploy_antenna,
        Some(Duration::from_secs(QUERY_TIMEOUT)),
    ) {
        // Ok(msg) => {
        //     let mut deployed = check_system_status(&msg, &"DEPLOYED", true, false);
        //     deployed &= check_antenna_status(&msg, false, true, false);
        //     deployed
        // }
        Ok(_) => true,
        Err(e) => {
            println!("Failed to deploy antenna: {:?}", e);
            false
        }
    }
}

fn reset_antenna(ant_service: &ServiceConfig) -> bool {
    println!("Reseting Antenna...");
    // let mutation_reset_antenna = "mutation{
    //     controlPower(state:RESET){
    //       success
    //     }
    //   }";

    let mutation_reset_antenna = format!(
        r#"
        mutation {{
            controlPower(state:RESET){{
                success,
                errors
            }}
        }}
    "#,
    );
    match query(
        ant_service,
        &mutation_reset_antenna,
        Some(Duration::from_secs(QUERY_TIMEOUT)),
    ) {
        Ok(_) => true,
        Err(e) => {
            println!("Failed to reset antenna: {:?}", e);
            false
        }
    }
}

fn main() -> Result<(), Error> {
    // check if RBF is set
    // let rbf = check_rbf();

    //retry the deployment for 5 times
    let num_retry = 5;

    // set system time
    // TODO

    // // don't proceed if rbf is not active
    // if rbf {
    //     return Ok(());
    // }

    // let mut deploy_status:bool;
    let ant_service = ServiceConfig::new("isis-ants-service")?;
    println!("Initiated the Antenna Service");

    // check if the satellite is still stowed
    let all_deployed = check_stowed(&ant_service);

    if all_deployed {
        // println!("Antenna already deployed");
        return Ok(());
    }

    thread::sleep(Duration::from_secs(1800));

    let mut process_status;
    let mut arm_status;
    let mut deploy_status;
    // let mut annt_deployed = 0;

    // deploy antenna, stopping if there are any failures along the way

    // check deployment status is stowed.
    // stowed_status = check_stowed(&ant_service);
    // if !stowed_status {
    //     // panic!("Inappropriate state beginning deployment");
    //     warn!("Inappropriate state beginning deployment");
    //     println!("Inappropriate state beginning deployment")
    // }

    for i in 0..num_retry {
        println!("Starting deployment sequence");

        println!("Trying the {} th time", i);

        let all_deployed1 = check_stowed(&ant_service);

        if all_deployed1 {
            println!("Antenna already deployed");
            break;
        }

        // arm the antenna
        arm_status = arm_antenna(&ant_service);
        if !arm_status {
            // panic!("Failed to arm antenna");
            warn!("Failed to arm antenna");
            println!("Failed to arm antenna");
        } else {
            println!("Antenna armed!");
        }

        // deploy the antenna
        deploy_status = deploy_antenna(&ant_service);
        if !deploy_status {
            warn!("Failed to deploy antenna");
        } else {
            // annt_deployed = 1;
            info!("Thermal knife activated");
            println!("Thermal Knife activated");
        }

        thread::sleep(Duration::from_secs(100));

        // reset antenna
        process_status = reset_antenna(&ant_service);
        if !process_status {
            // panic!("Failed to reset antenna");
            warn!("Failed to reset antenna");
            println!("Failed to reset antenna");
        }
    }

    Ok(())
}
