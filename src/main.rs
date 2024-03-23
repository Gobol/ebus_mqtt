
use std::fs::File;
use std::io::{BufReader, Read};
use std::net::TcpStream;
use std::thread;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ebus::parser::{EbusParser, EbusRequest, EbusResponse};
use log::LogLevel;

use crate::log::*;

mod ebus;
mod log;

const LOG_LEVEL : LogLevel = LogLevel::Info;


/*
    fn match_field() is matching value_hex with field_def

    Field matching syntax:
    *       - any value matches
    ^<hex>  - value starts with <hex>
    <hex>   - value matches exactly
 */

fn match_field(value_hex:&str, field_def:&serde_json::Value) -> bool {
    let field_pattern = field_def.as_str().unwrap();
    let field_len = field_pattern.len();
    // check for all-match
    if field_pattern == "*" {
        return true;
    }
    // check for starts-with
    if field_pattern.starts_with("^") {
        return value_hex.starts_with(&field_pattern[1..]);
    }
    // check for exact match
    for i in 0..field_len {
        if field_pattern.chars().nth(i).unwrap() != '*' && field_pattern.chars().nth(i).unwrap() != value_hex.chars().nth(i).unwrap() {
            return false;
        }
    }
    return true;
}


struct Mapper {
    defs : serde_json::Value,
}

impl Mapper {
    fn new(defs : serde_json::Value) -> Mapper {
        Mapper { defs }
    }

    fn received_telegram(&mut self, req: &EbusRequest, resp: Option<&EbusResponse>) {
        println!("Received telegram {}", req);
        if let Some(r) = resp {
            println!("    `-> Response: {}", r);
        }
        // iterate through all defined circuits
        for circuit in self.defs["circuits"].as_array().unwrap() {
            // println!("    Circuit: {}", circuit["name"].as_str().unwrap());

            // iterate through possible circuit's messages
            for msg in circuit["messages"].as_array().unwrap() {
                // println!("        Message: {}", msg["comment"].as_str().unwrap());

                // check if we've got matching request to message definition
                if match_field(req.src_hex().as_str(), &msg["request_match"]["src"]) &&
                   match_field(req.dest_hex().as_str(), &msg["request_match"]["dst"]) &&
                   match_field(req.pbsb_hex().as_str(), &msg["request_match"]["pbsb"]) &&
                   match_field(req.data_hex().as_str(), &msg["request_match"]["data"]) {
                    // println!("            Matched request <OK>");

                    // ok, let's initialize json object with parsed response data
                    let mut result_js = serde_json::Map::new();

                    // check if we've got "response_map" defined in msg
                    let msgo = msg.as_object().unwrap();
                    let mut field_map: Option<&serde_json::Value> = None;
                    let mut data: Option<&Vec<u8>> = None;

                    if msgo.contains_key("response_map") {
                        // check if we've received a response
                        if let Some(r) = resp {
                            data = Some(r.data());
                            field_map = Some(&msg["response_map"]);
                        }
                    }
                    if msgo.contains_key("request_map") {
                        data = Some(req.data());
                        field_map = Some(&msg["request_map"]);
                    }
                    // check if we've got data and field_map defined
                    if (data == None) || (field_map == None) {
                        continue;
                    }
                    // parse data with field definitions 
                    for field in field_map.unwrap().as_array().unwrap() {
                        let bytes = data.unwrap();
                        let field_name = field["field_name"].as_str().unwrap();
                        let offset = field["field_offset"].as_u64().unwrap();
                        let data_type = field["data_type"].as_str().unwrap();
                        let factor = field["factor"].as_f64().unwrap();
                        let unit = field["unit"].as_str().unwrap();
                        println!{"                Field: {} @{:02x} t={} f={} [{}]", field_name, offset, data_type, factor, unit};
                        match data_type {
                            "u8" => {
                                let val: u8 = bytes[offset as usize] as u8;
                                if factor == 1.0 {
                                    result_js.insert(field_name.to_string(), serde_json::Value::Number(serde_json::Number::from(val)));
                                } else {
                                    let value = val as f64 * factor;
                                    result_js.insert(field_name.to_string(), serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap()));
                                }
                            },
                            "u16le" => {
                                let val: u16 = (bytes[offset as usize] as u16) | ((bytes[offset as usize + 1] as u16) << 8);
                                if factor == 1.0 {
                                    result_js.insert(field_name.to_string(), serde_json::Value::Number(serde_json::Number::from(val)));
                                } else {
                                    let value = val as f64 * factor;
                                    result_js.insert(field_name.to_string(), serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap()));
                                }
                            },                  
                            "u16he" => {
                                let val: u16 = ((bytes[offset as usize] as u16) << 8) | (bytes[offset as usize + 1] as u16);
                                if factor == 1.0 {
                                    result_js.insert(field_name.to_string(), serde_json::Value::Number(serde_json::Number::from(val)));
                                } else {
                                    let value = val as f64 * factor;
                                    result_js.insert(field_name.to_string(), serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap()));
                                }
                            },                 
                            _ => {
                                println!("                Unsupported data type {}", data_type);
                            }
                        }
                    }
                    // print result_js
                    println!("                Result: {}", serde_json::to_string(&serde_json::Value::Object(result_js)).unwrap());
                }
            }
        }
    }
}

fn main() {
    let mut ebus_ip = "";
    let mut ebus_port = 0;
    let mut mqtt_ip: &str = "";
    let mut mqtt_port: i32 = 0; 
    let mut mqtt_user: &str = "";
    let mut mqtt_pass: &str = "";
    let mut mqtt_topic: &str = "";

    // load config.json file 
    let cfg : serde_json::Value = serde_json::from_reader(File::open("./config.json").expect("Failed to open config.json")).unwrap();

    if cfg.as_object().unwrap().contains_key("ebus") {
        ebus_ip = cfg["ebus"]["host"].as_str().unwrap();
        ebus_port = cfg["ebus"]["port"].as_u64().unwrap() as i32;
    } else {
        ebus_ip = "192.168.2.45";
        ebus_port = 9999;
    }

    if cfg.as_object().unwrap().contains_key("mqtt") {
        mqtt_ip = cfg["mqtt"]["host"].as_str().unwrap();
        mqtt_port = cfg["mqtt"]["port"].as_i64().unwrap() as i32;
        mqtt_user = cfg["mqtt"]["user"].as_str().unwrap();
        mqtt_pass = cfg["mqtt"]["pass"].as_str().unwrap();
        mqtt_topic = cfg["mqtt"]["topic"].as_str().unwrap();
    } else {
        logI("No MQTT configuration found in config.json");
    }

    let filename = "./ariston.json";

    // Open the file in read-only mode with buffer.
    let file = File::open(filename).expect("Failed to open file");
    let reader = BufReader::new(file);

    // Read the JSON contents of the file as untyped
    let u : serde_json::Value = serde_json::from_reader(reader).unwrap();
    let mut mapper: Mapper = Mapper::new(u.clone());
    println!("{:?}", u);
    println!("Loaded comm definitions from file {}", filename);
    println!("     Appliance: {}", u["appliance"].as_str().unwrap());
    println!("     Bus: {}", u["bus"].as_str().unwrap());
    
    // Create a TCP stream
    let mut stream = TcpStream::connect(format!("{}:{}", ebus_ip, ebus_port)).expect("Failed to connect");

    // Create a flag to indicate when to stop receiving data
    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();

    // Spawn a thread to receive and print data
    let handle = thread::spawn(move || {
        let mut buffer = [0; 1024];
        let mut parser = EbusParser::new(move |a,b| { mapper.received_telegram(a,b) });
        while running_clone.load(Ordering::Relaxed) {
            match stream.read(&mut buffer) {
                Ok(n) if n > 0 => {
                    parser.feed(&buffer[0..n], n);
                    // for i in 0..n {
                    //     print!("{:02X} ", buffer[i]);
                    // }
                    // println!();
                }
                Ok(_) => break,
                Err(_) => break,
            }
        }
    });

    // Wait for a keypress to stop receiving data
    let _ = std::io::stdin().read(&mut [0u8]).unwrap();

    // Set the flag to stop receiving data
    running.store(false, Ordering::Relaxed);

    // Wait for the receiving thread to finish
    let _ = handle.join();
}