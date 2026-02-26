// Copyright 2015 click2stream, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::error::Error;
use std::fmt::Debug;

use arrow_client::config::usage;
use arrow_client::runtime;

use arrow_client::client::ArrowClient;
use arrow_client::config::Config;

/// Unwrap a given result (if possible) or print the error message and exit
/// the process printing application usage.
fn result_or_usage<T, E>(res: Result<T, E>) -> T
where
    E: Error + Debug,
{
    match res {
        Ok(res) => res,
        Err(err) => {
            println!("ERROR: {}\n", err);
            usage(1);
        }
    }
}

/// Arrow Client main function.
fn main() {
    let args: Vec<String> = std::env::args().collect();

    #[cfg(feature = "discovery")]
    if args.get(1).map(|s| s.as_str()) == Some("--discovery-only") {
        discovery_only_main(&args[2..]);
        return;
    }

    let config = result_or_usage(Config::from_args(std::env::args()));

    let (client, task) = ArrowClient::new(config);

    // forget the client, we want to run the application indefinitely
    std::mem::forget(client);

    runtime::run(task);
}

#[cfg(feature = "discovery")]
fn discovery_only_main(args: &[String]) {
    use std::collections::HashSet;
    use std::path::Path;
    use std::sync::Arc;

    use arrow_client::output;
    use arrow_client::scanner::discovery;
    use arrow_client::utils::logger::stderr::StderrLogger;
    use arrow_client::utils::logger::BoxLogger;

    const RTSP_PATHS_FILE: &str = "/etc/arrow/rtsp-paths";
    const MJPEG_PATHS_FILE: &str = "/etc/arrow/mjpeg-paths";

    let mut discovery_whitelist: Vec<String> = Vec::new();
    let mut rtsp_paths_file = RTSP_PATHS_FILE.to_string();
    let mut mjpeg_paths_file = MJPEG_PATHS_FILE.to_string();
    let mut output_stdout = false;
    let mut output_file: Option<String> = None;
    let mut verbose = false;
    let mut path_delay_ms: u64 = 50;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-D" => {
                i += 1;
                if i < args.len() {
                    discovery_whitelist.push(args[i].clone());
                    i += 1;
                } else {
                    eprintln!("ERROR: -D requires interface name");
                    discovery_only_usage(1);
                }
            }
            "--output" => {
                i += 1;
                if i < args.len() && args[i] == "json" {
                    i += 1;
                }
                output_stdout = true;
            }
            "-v" | "--verbose" => {
                verbose = true;
                i += 1;
            }
            arg => {
                if arg.starts_with("--output-file=") {
                    output_file = Some(arg["--output-file=".len()..].to_string());
                } else if arg.starts_with("--rtsp-paths=") {
                    rtsp_paths_file = arg["--rtsp-paths=".len()..].to_string();
                } else if arg.starts_with("--mjpeg-paths=") {
                    mjpeg_paths_file = arg["--mjpeg-paths=".len()..].to_string();
                } else if arg.starts_with("--path-delay-ms=") {
                    if let Ok(n) = arg["--path-delay-ms=".len()..].parse::<u64>() {
                        path_delay_ms = n;
                    }
                } else if arg == "--help" {
                    discovery_only_usage(0);
                } else {
                    eprintln!("ERROR: unknown argument: \"{}\"", arg);
                    discovery_only_usage(1);
                }
                i += 1;
            }
        }
    }

    if !output_stdout && output_file.is_none() {
        output_stdout = true;
    }

    let logger = BoxLogger::new(StderrLogger::new(false));
    let rtsp_paths = Arc::new(load_paths_file(&rtsp_paths_file).unwrap_or_default());
    let mjpeg_paths = Arc::new(load_paths_file(&mjpeg_paths_file).unwrap_or_default());

    let whitelist: HashSet<String, _> = if discovery_whitelist.is_empty() {
        HashSet::new()
    } else {
        discovery_whitelist.into_iter().collect()
    };
    let discovery_whitelist = Arc::new(whitelist);

    let path_delay = std::time::Duration::from_millis(path_delay_ms);
    let (scan_result, survey_opt) = match discovery::scan_network(
        logger,
        discovery_whitelist,
        rtsp_paths.clone(),
        mjpeg_paths.clone(),
        verbose,
        path_delay,
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ERROR: discovery failed: {}", e);
            std::process::exit(2);
        }
    };

    let discovery_meta = if verbose {
        Some(output::DiscoveryMetadata {
            paths_rtsp_source: rtsp_paths_file,
            paths_rtsp_entries: (*rtsp_paths).clone(),
            paths_mjpeg_source: mjpeg_paths_file,
            paths_mjpeg_entries: (*mjpeg_paths).clone(),
            survey: survey_opt.unwrap_or_default(),
        })
    } else {
        None
    };

    let out_path = output_file.as_deref().map(Path::new);
    if let Err(e) = output::write_discovery_output(&scan_result, output_stdout, out_path, discovery_meta.as_ref()) {
        eprintln!("ERROR: failed to write output: {}", e);
        std::process::exit(3);
    }
}

#[cfg(feature = "discovery")]
fn load_paths_file(path: &str) -> std::io::Result<Vec<String>> {
    use std::io::BufRead;
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut paths = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if !line.starts_with('#') && !line.trim().is_empty() {
            paths.push(line);
        }
    }
    Ok(paths)
}

#[cfg(feature = "discovery")]
fn discovery_only_usage(exit_code: i32) -> ! {
    println!(
        "USAGE: {} --discovery-only [OPTIONS]\n",
        std::env::args().next().unwrap_or_else(|| "arrow-client".into())
    );
    println!("  Run a one-time network discovery and output results as JSON.");
    println!();
    println!("OPTIONS:");
    println!("  -D iface       limit discovery to interface (repeatable)");
    println!("  --output json  write JSON to stdout (default if no --output-file)");
    println!("  --output-file=<path>  write JSON to file");
    println!("  --rtsp-paths=<path>   path to RTSP paths file (default: /etc/arrow/rtsp-paths)");
    println!("  --mjpeg-paths=<path>  path to MJPEG paths file (default: /etc/arrow/mjpeg-paths)");
    println!("  --path-delay-ms=N     delay in ms between path probes per host (default: 50, 0=no throttle)");
    println!("  -v, --verbose  print path counts, host count, and path-check counts to stderr");
    println!("  --help         print this help");
    println!();
    std::process::exit(exit_code);
}
