use failure::Error;
use log::{info, warn,error};
use serde_json::value::{Value, Value::String as VString};
use std::thread;
use std::time::Duration;

use kubos_app::{query, ServiceConfig};

static QUERY_TIMEOUT: u64 = 30;
static DEPLOYMENT_DELAY: u64 = 1800;
static DEPLOY_INTERVAL: u64 = 60;

// returns true if this program is run with RBF
fn controller_selection() -> bool {
    let args = std::env::args();

    // Default controller: Secondary
    let mut pri_controller = true;

    // don't include the program name in the arguments
    let mut arg_iter = args.into_iter();
    arg_iter.next();
    for arg in arg_iter {
        match &arg[..] {
            "-s" => {
                println! {"Choosing the secondary thermal knife"}
                pri_controller = false
            }
            "-p" => {
                println! {"Choosing the primary thermal knife"}
                pri_controller = true
            }
            _ => panic!(format!("Invalid argument '{}'", arg)),
        }
    }
    pri_controller
}

// returns true if the system status matches the given arguments
// Let sys_status = "DEPLOYED", if ture, retrun 1, otherwise returns 0
fn check_system_status(response: &Value, sys_status: &str) -> bool {
    let data = response.get("deploymentStatus");
    let status = data
        .and_then(|v| v.get("status"))
        .and_then(|v| match v {
            VString(s) => {
                println!("Deployment status:{:?}", s);
                info!("Deployment status, {:?}", s);
                Some(*s == sys_status)
            }
            _ => None,
        })
        .unwrap_or(false);

    status
}

// returns true if the system is stowed and not armed
fn check_stowed(ant_service: &ServiceConfig) -> bool {
    let query_initial_status = format!(
        r#"
        {{
            deploymentStatus {{
                status,
            }}
        }}
    "#,
    );

    match query(
        ant_service,
        &query_initial_status,
        Some(Duration::from_secs(QUERY_TIMEOUT)),
    ) {
        Ok(msg) => {
            let deployed = check_system_status(&msg, &"DEPLOYED");
            deployed
        }

        Err(e) => {
            error!("Failed to contact antenna: {:?}", e);
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
            error!("Failed to arm antenna: {:?}", e);
            false
        }
    }
}

fn deploy_antenna(ant_service: &ServiceConfig) -> bool {
    let mutation_deploy_antenna = format!(
        r#"
        mutation {{
            deploy(ant:ALL,force:true, time:5){{
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
        Ok(_) => true,
        Err(e) => {
            error!("Failed to deploy antenna: {:?}", e);
            false
        }
    }
}

fn reset_antenna(ant_service: &ServiceConfig) -> bool {
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
            error!("Failed to reset antenna: {:?}", e);
            false
        }
    }
}

fn set_primary_knife(ant_service: &ServiceConfig) -> bool {
    let mutation_reset_antenna = format!(
        r#"
        mutation {{
            configureHardware(config:PRIMARY){{
                success,
                errors,
                config
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
            error!("Failed to set the primary heater: {:?}", e);
            false
        }
    }
}

fn set_secondary_knife(ant_service: &ServiceConfig) -> bool {
    let mutation_reset_antenna = format!(
        r#"
        mutation {{
            configureHardware(config:SECONDARY){{
                success,
                errors,
                config
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
            error!("Failed to set the primary heater: {:?}", e);
            false
        }
    }
}

fn main() -> Result<(), Error> {
    // let mut deploy_status:bool;
    let ant_service = ServiceConfig::new("isis-ants-service")?;
    let mut process_status;
    let mut arm_status;
    let mut deploy_status;

    // Thermal knife selection 
    let ctl_sel = controller_selection();

    //retry the deployment for 6 times
    // After 3 times, we will try to change the thermal knife
    let num_retry = 6;

    // Thermal knife selection
    if ctl_sel {
        set_primary_knife(&ant_service);
    } else {
        set_secondary_knife(&ant_service);
    }

    thread::sleep(Duration::from_secs(DEPLOYMENT_DELAY));

    for i in 0..num_retry {
        // check if the satellite is still stowed
        let all_deployed = check_stowed(&ant_service);

        if all_deployed {
            info!("Antenna succefully deployed");
            break;
        }

        // reset antenna
        process_status = reset_antenna(&ant_service);
        println!("Reseting antenna deployment sequence");
        if !process_status {
            warn!("Failed to reset antenna");
        }

        if  i > 2 {
            if !ctl_sel {
                set_primary_knife(&ant_service);
            } else {
                set_secondary_knife(&ant_service);
            }
        }
        
        info!("Starting deployment sequence");
        info!("Trying the {} th time", i+1);
        println!("Trying the {} th time", i+1);

        // arm the antenna
        arm_status = arm_antenna(&ant_service);
        if !arm_status {
            warn!("Failed to arm antenna");
        } else {
            info!("Antenna armed!");
        }

        // deploy the antenna
        deploy_status = deploy_antenna(&ant_service);
        if !deploy_status {
            warn!("Failed to deploy antenna");
        } else {
            info!("Thermal knife activated");
        }

        thread::sleep(Duration::from_secs(DEPLOY_INTERVAL));
    }

    info!("Finished deployment sequence");
    println!("Finished deployment sequence");

    Ok(())
}
