use clap::{Arg, command};
use colored::*;
use indexmap::IndexMap;
use serde_json::Value;
use std::error::Error;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

fn command_matches() -> clap::ArgMatches {
    command!()
        .name("Temperature Checker")
        .version("1.0.2")
        .about("A useful Rust script to keep an eye on device's temperatures. It includes information about the CPU, each CORE and the GPU.")
        .arg(
            Arg::new("time")
                .short('t')
                .long("time")
                .value_name("TIME")
                .help("Sets the time interval for checking temperature")
                .required(false)
                .default_value("1"),
        )
        .arg(
            Arg::new("delta-times")
                .short('d')
                .long("delta-times")
                .value_name("DELTA")
                .help("See the difference between the current fetch and the previous one [the lower the better]")
                .required(false)
                .num_args(0),
        )
        .get_matches()
}

fn _print_type_of<T>(_: &T) {
    println!("{}", std::any::type_name::<T>());
}

fn handle_temps(temps: &Value, map: &mut IndexMap<String, f64>) {
    if let Some(cpu_temp) = temps["coretemp-isa-0000"]["Package id 0"]["temp1_input"].as_f64() {
        map.insert("CPU\t(°C)".to_string(), cpu_temp);
    }
    if let Some(acpi_temp) = temps["acpitz-acpi-0"]["temp1"]["temp1_input"].as_f64() {
        map.insert("ACPI\t(°C)".to_string(), acpi_temp);
    }
    if let Some(core) = temps["coretemp-isa-0000"].as_object() {
        for (key, value) in core.iter() {
            if !key.starts_with("Core") {
                continue;
            }
            if let Some(core_num_str) = key.split_whitespace().nth(1) {
                if let Ok(index) = core_num_str.parse::<i32>() {
                    let temp_key = format!("temp{}_input", index + 2);
                    if let Some(temp) = value.get(&temp_key).and_then(|v| v.as_f64()) {
                        map.insert(format!("Core#{}\t(°C)", index), temp);
                    }
                }
            }
        }
    }
}

fn get_temps(map: &mut IndexMap<String, f64>) -> Result<(), Box<dyn Error>> {
    let output = Command::new("sensors").arg("-j").output()?;

    if !output.status.success() {
        return Err(Box::from("Failed to fetch temperatures"));
    }

    let temps = String::from_utf8_lossy(&output.stdout);
    let temps: Value = serde_json::from_str(&temps)?;
    
    handle_temps(&temps, map);

    let nvidia = Command::new("nvidia-smi")
        .args(["--query-gpu=temperature.gpu", "--format=csv,noheader"])
        .output();

    if let Ok(nvidia) = nvidia {
        let gpu_temp = String::from_utf8_lossy(&nvidia.stdout)
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0);
    
        map.insert("GPU\t(°C)".to_string(), gpu_temp);
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let matches: clap::ArgMatches = command_matches();
    let interval: f64 = matches
        .get_one::<String>("time")
        .unwrap()
        .parse::<f64>()
        .unwrap_or(1.0);

    if interval < 1.0 {
        eprintln!("Error: Time interval must be greater than 1 second.");
        std::process::exit(1);
    }

    let delta: bool =
        matches.value_source("delta-times") == Some(clap::parser::ValueSource::CommandLine);

    println!("Fetching temperatures, please wait...");
    let mut map: IndexMap<String, f64> = IndexMap::new();
    let mut prev_map: IndexMap<String, f64> = IndexMap::new();
    loop {
        map.clear();
        print!("\x1B[2J\x1B[H");
        print!(
            "Fetching temperatures every {}s... ({})",
            interval,
            chrono::Local::now().format("%a %b %d %Y %H:%M:%S")
        );

        get_temps(&mut map)?;

        let mut output = String::from("\nPress Ctrl+C to exit");
        for (key, value) in map.iter() {
            let temp = if *value < 50.0 {
                format!("{:.1}", value).green()
            } else if *value < 70.0 {
                format!("{:.1}", value).yellow()
            } else {
                format!("{:.1}", value).red()
            };

            let t_stdout = if delta && !prev_map.is_empty() {
                let tmp: f64 = *value - prev_map.get(key).unwrap();
                let delta_fmt: ColoredString = if tmp < 0.0 {
                    format!("({:+.1})", tmp).green()
                } else if tmp == 0.0 {
                    format!("({:+.1})", tmp).black()
                } else if tmp > 0.0 && tmp < 5.0 {
                    format!("({:+.1})", tmp).yellow()
                } else {
                    format!("({:+.1})", tmp).red()
                };
                format!("\n{}\t>>>\t{} {}", key, temp, delta_fmt)
            } else {
                format!("\n{}\t>>>\t{}", key, temp)
            };

            output.push_str(&t_stdout);
        }
        if delta {
            prev_map = map.clone();
        }
        println!("{}", output);
        sleep(Duration::from_secs_f64(interval));
    }
}
