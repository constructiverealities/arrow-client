// Copyright 2017 click2stream, Inc.
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

//! Discovery output: write scan results as JSON to stdout and/or a file.
//! Output shape matches the service table JSON for compatibility.

#![cfg(feature = "discovery")]

use std::io;
use std::io::Write;
use std::path::Path;

use json::JsonValue;

use crate::net::raw::ether::MacAddr;
use crate::scanner::discovery::SurveyData;
use crate::scanner::result::ScanResult;
use crate::svc_table::Service;

/// Metadata for the discovery block (paths sources + survey). When present, included as "discovery" in JSON.
pub struct DiscoveryMetadata {
    pub paths_rtsp_source: String,
    pub paths_rtsp_entries: Vec<String>,
    pub paths_mjpeg_source: String,
    pub paths_mjpeg_entries: Vec<String>,
    pub survey: SurveyData,
}

fn utc_timestamp_sec() -> i64 {
    time::now_utc().to_timespec().sec
}

/// Build the same JSON shape as the service table: `{"services": [...], "discovery": {...}?}`.
fn scan_result_to_json(result: &ScanResult, discovery: Option<&DiscoveryMetadata>) -> JsonValue {
    let default_mac = MacAddr::zero();
    let default_address = std::net::SocketAddr::from(([0, 0, 0, 0], 0));
    let last_seen = utc_timestamp_sec();

    let services: Vec<JsonValue> = result
        .services()
        .enumerate()
        .map(|(i, svc)| service_to_json_element((i + 1) as u16, svc, last_seen, default_mac, default_address))
        .collect();

    let mut root = object! {
        "services" => services
    };

    if let Some(d) = discovery {
        root["discovery"] = discovery_block_to_json(d);
    }

    root
}

fn discovery_block_to_json(d: &DiscoveryMetadata) -> JsonValue {
    let paths = object! {
        "rtsp" => object! {
            "source" => d.paths_rtsp_source.as_str(),
            "entries" => JsonValue::Array(d.paths_rtsp_entries.iter().map(|s| JsonValue::String(s.clone())).collect())
        },
        "mjpeg" => object! {
            "source" => d.paths_mjpeg_source.as_str(),
            "entries" => JsonValue::Array(d.paths_mjpeg_entries.iter().map(|s| JsonValue::String(s.clone())).collect())
        }
    };

    let mut survey_obj = JsonValue::new_object();
    for (addr, entries) in &d.survey {
        let arr = JsonValue::Array(
            entries
                .iter()
                .map(|(path, result)| {
                    object! {
                        "path" => path.as_str(),
                        "result" => result.as_str()
                    }
                })
                .collect(),
        );
        survey_obj[addr.as_str()] = arr;
    }

    object! {
        "paths" => paths,
        "survey" => survey_obj
    }
}

fn service_to_json_element(
    id: u16,
    svc: &Service,
    last_seen: i64,
    default_mac: MacAddr,
    default_address: std::net::SocketAddr,
) -> JsonValue {
    let svc_type = svc.service_type();
    let mac = svc.mac().unwrap_or(default_mac);
    let address = svc.address().unwrap_or(default_address);
    let path = svc.path().unwrap_or("");

    object! {
        "id" => id,
        "svc_type" => svc_type.code(),
        "mac" => format!("{}", mac),
        "address" => format!("{}", address),
        "path" => path,
        "static_svc" => false,
        "last_seen" => last_seen,
        "active" => true
    }
}

/// Write discovery result as JSON to the given sinks.
/// - If `to_stdout` is true, writes the same JSON to stdout.
/// - If `output_file` is `Some(path)`, writes the same JSON to that file.
/// - If `discovery` is `Some`, the JSON includes a "discovery" block (paths + survey).
pub fn write_discovery_output(
    result: &ScanResult,
    to_stdout: bool,
    output_file: Option<&Path>,
    discovery: Option<&DiscoveryMetadata>,
) -> io::Result<()> {
    let json = scan_result_to_json(result, discovery);
    let bytes = json.dump();

    if to_stdout {
        io::stdout().write_all(bytes.as_bytes())?;
    }

    if let Some(path) = output_file {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }

    Ok(())
}
