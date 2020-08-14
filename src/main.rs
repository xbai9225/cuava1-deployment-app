use failure::Error;
use std::time::Duration;
use serde_json::value::{
    Value,
    Value::{
        String as VString,
        Bool as VBool
    }
};

use kubos_app::{
    ServiceConfig,
    query
};

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
            _ => panic!(format!("Invalid argument '{}'", arg))
        }
    }
    rbf
}

// returns true if the system status matches the given arguments
fn check_system_status(
    response: &Value,
    sys_status: &str,
    sys_armed: bool,
    sys_burn_active: bool
) -> bool {
    let data = response.get("data")
        .and_then(|v| v.get("deploymentStatus"));
    let mut status = true;
    status &= data
        .and_then(|v| v.get("status"))
        .and_then(|v|
            match v {
                VString(s) => Some(s == sys_status),
                _ => None
            }
        )
        .unwrap_or(false);
    status &= data
        .and_then(|v| v.get("sysArmed"))
        .and_then(|v|
            match v {
                VBool(b) => Some(*b == sys_armed),
                _ => None
            }
        )
        .unwrap_or(false);
    status &= data
        .and_then(|v| v.get("sysBurnActive"))
        .and_then(|v|
            match v {
                VBool(b) => Some(*b == sys_burn_active),
                _ => None
            }
        )
        .unwrap_or(false);
    status
}

// returns true if all antenna statuses match the given arguments
fn check_antenna_status(
    response: &Value,
    active: bool,
    deployed: bool,
    stopped_time: bool
) -> bool {
    let data = response.get("data")
        .and_then(|v| v.get("deploymentStatus"));
    let mut status = true;
    for i in 1..5 {
        status &= data
            .and_then(
                |v| v.get(&format!("ant{}Active", i))
            )
            .and_then(|v|
                match v {
                    VBool(b) => Some(*b == active),
                    _ => None
                }
            )
            .unwrap_or(false);
        status &= data
            .and_then(
                |v| v.get(&format!("ant{}NotDeployed", i))
            )
            .and_then(|v|
                match v {
                    VBool(b) => Some(*b != deployed),
                    _ => None
                }
            )
            .unwrap_or(false);
        status &= data
            .and_then(
                |v| v.get(&format!("ant{}StoppedTime", i))
            )
            .and_then(|v|
                match v {
                    VBool(b) => Some(*b == stopped_time),
                    _ => None
                }
            )
            .unwrap_or(false);
    }
    status
}

// returns true if the system is stowed and not armed
fn check_stowed(ant_service: &ServiceConfig) -> bool {
    let query_initial_status = "query{deployment_status}";
    match query(
        ant_service,
        &query_initial_status[..],
        Some(Duration::from_secs(QUERY_TIMEOUT))
    ) {
        Ok(msg) => {
            let mut stowed = check_system_status(&msg, &"STOWED", false, false);
            stowed &= check_antenna_status(&msg, false, false, false);
            stowed
        },
        // not sure if panicing is appropriate here
        Err(e) => panic!("Failed to contact antenna: {:?}", e)
    }
}

// arms the antenna, returning true on success
fn arm_antenna(ant_service: &ServiceConfig) -> bool {
    let mutation_arm_antenna = "mutation{arm (state:ARM) {successerrors}}";
    match query(
        ant_service,
        &mutation_arm_antenna[..],
        Some(Duration::from_secs(QUERY_TIMEOUT))
    ) {
        Ok(msg) => {
            check_system_status(&msg, &"STOWED", true, false)
        },
        Err(e) => panic!("Failed to contact antenna: {:?}", e)
    }
}

fn deploy_antenna(ant_service: &ServiceConfig) -> bool {
    let mutation_deploy_antenna = "mutation{
        deploy(ant:ALL,force:true, time:10){
          success
        }
      }";
    match query(
        ant_service,
        &mutation_deploy_antenna[..],
        Some(Duration::from_secs(QUERY_TIMEOUT))
    ) {
        Ok(msg) => {
            let mut deployed = check_system_status(
                &msg, &"DEPLOYED", true, false
            );
            deployed &= check_antenna_status(&msg, false, true, false);
            deployed
        },
        Err(e) => panic!("Failed to deploy antenna: {:?}", e)
    }
}

fn reset_antenna(ant_service: &ServiceConfig) -> bool {
    let mutation_reset_antenna = "mutation{
        controlPower(state:RESET){
          success
        }
      }";
    match query(
        ant_service, 
        &mutation_reset_antenna[..], 
        Some(Duration::from_secs(QUERY_TIMEOUT))
    ) {
        Ok(_) => true,
        Err(e) => panic!("Failed to reset antenna: {:?}", e)
    }
}

fn main() -> Result<(), Error> {

    // check if RBF is set
    let rbf = check_rbf();

    // set system time
    // TODO

    // don't proceed if rbf active
    if rbf {
        return Ok(());
    }

    // deploy antenna, stopping if there are any failures along the way
    let ant_service = ServiceConfig::new("isis-ants-service")?;
    let mut process_status;
    
    // check deployment status is stowed before proceeding to 
    process_status = check_stowed(&ant_service);
    if !process_status {
        panic!("Inappropriate state beginning deployment");
    }

    // arm the antenna
    process_status = arm_antenna(&ant_service);
    if !process_status {
        panic!("Failed to arm antenna");
    }

    // deploy the antenna
    process_status = deploy_antenna(&ant_service);
    if !process_status {
        panic!("Failed to deploy antenna");
    }

    // reset antenna
    process_status = reset_antenna(&ant_service);
    if !process_status {
        panic!("Failed to reset antenna");
    }

    Ok(())
}
