mod utils;
use utils::*;

use std::env;
use std::fs::read_to_string;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::net::TcpStream;
use std::num::ParseIntError;

use std::str;
use std::time::{SystemTime, UNIX_EPOCH};

const AIS_CHARSET: &[u8] =
    "#ABCDEFGHIJKLMNOPQRSTUVWXYZ#####_###############0123456789######".as_bytes();

fn main() {
    let stream = TcpStream::connect("192.168.178.69:30002").unwrap();
    let lines: BufReader<Box<dyn Read>> =
        BufReader::new(Box::new(stream) as Box<dyn std::io::Read>);

    let mut counter = vec![false; 24];

    let mut even_cprlat: f32 = 0.0;
    let mut even_cprlon: f32 = 0.0;
    let mut even_cprtime: i64 = 0;

    let mut odd_cprlat: f32 = 0.0;
    let mut odd_cprlon: f32 = 0.0;
    let mut odd_cprtime: i64 = 0;

    let mut latitude = 0.0;
    let mut longitude = 0.0;

    let generator: u128 = 0b1111111111111010000001001;

    for line in lines.lines() {
        let line = line.unwrap();
        let mut byte_array = hex_to_bytes(&line);
        let crc = compute_crc(vec_to_u128(&byte_array), generator);

        // ICAO address
        let address =
            byte_array[1] as i32 | ((byte_array[2] as i32) << 8) | ((byte_array[3] as i32) << 16);
        println!("{:X?} {:X?}", &byte_array[1..4], address);

        //println!("{}\n", get_downlink_code(binary_string));

        match byte_array[0] >> 3 {
            0 => {
                println!("Short air-air surveillance: {:?}\n {}", byte_array, line);
                println!("Vertical Status: {}", (byte_array[0] >> 2) & 1); // not sure wheter this is grabbing the right number
                println!("Crosslink Capability: {}", (byte_array[0] >> 1) & 1); // not sure wheter this is grabbing the right number
                if byte_array[0] & 1 != 0 {
                    eprintln!("{}:Error, padding is not 0", line!());
                    break;
                }
                println!("Sensitivity Level: {}", (byte_array[1] >> 5) & 7);
                let padding = (byte_array[1] >> 3) & 3;
                if padding != 0 {
                    eprintln!("{}:Error, Padding is not 0: {:b}", line!(), padding);
                    break;
                }
                let reply_info = ((byte_array[1] & 7) << 1) | ((byte_array[2] & 128) >> 7);
                println!("Reply Information: {}", reply_info);
                println!("{:?}", byte_array);
                let m_bit = byte_array[3] & (1 << 6);
                let q_bit = byte_array[3] & (1 << 4);

                // TODO: make the integer sizes more appropriate
                // TODO: add other mbit and qbit options
                if m_bit == 0 {
                    if q_bit != 0 {
                        let n = ((byte_array[2] as u64 & 31) << 6)
                            | ((byte_array[3] as u64 & 0x80) >> 2)
                            | ((byte_array[3] as u64 & 0x20) >> 1)
                            | (byte_array[3] as u64 & 15);
                        println!("Alt: {}", n as u128 * 25 - 1000)
                    }
                }

                // TODO: add address
            }
            4 => {
                println!("{}", line);
                println!("Flight Status: {}", byte_array[0] & 7);
                let dl_req = (byte_array[1] >> 3) & 31;
                println!("Downlink Request: {}", dl_req);
                let util_msg = ((byte_array[1] & 7) << 3) | byte_array[2] >> 5;
                println!("Utility Message: {}", util_msg);
                let m_bit = byte_array[3] & (1 << 6);
                let q_bit = byte_array[3] & (1 << 4);

                // TODO: make the integer sizes more appropriate
                // TODO: add other mbit and qbit options
                if m_bit == 0 {
                    if q_bit != 0 {
                        let n = ((byte_array[2] as u64 & 31) << 6)
                            | ((byte_array[3] as u64 & 0x80) >> 2)
                            | ((byte_array[3] as u64 & 0x20) >> 1)
                            | (byte_array[3] as u64 & 15);
                        println!("Alt: {}", n as u128 * 25 - 1000)
                    }
                }

                // TODO: add address
            }
            17 => {
                let tc = byte_array[4] >> 3;
                if tc >= 1 && tc <= 4 {
                    let mut callsign = vec![0u8; 9];
                    callsign[0] = AIS_CHARSET[(byte_array[5] >> 2) as usize];
                    callsign[1] =
                        AIS_CHARSET[(((byte_array[5] & 3) << 4) | (byte_array[6] >> 4)) as usize];
                    callsign[2] =
                        AIS_CHARSET[(((byte_array[6] & 15) << 2) | (byte_array[7] >> 6)) as usize];
                    callsign[3] = AIS_CHARSET[(byte_array[7] & 63) as usize];
                    callsign[4] = AIS_CHARSET[(byte_array[8] >> 2) as usize];
                    callsign[5] =
                        AIS_CHARSET[(((byte_array[8] & 3) << 4) | (byte_array[9] >> 4)) as usize];
                    callsign[6] =
                        AIS_CHARSET[(((byte_array[9] & 15) << 2) | (byte_array[10] >> 6)) as usize];
                    callsign[7] = AIS_CHARSET[(byte_array[10] & 63) as usize];
                    callsign[8] = b'\0';
                    println!("{}", str::from_utf8(&callsign).unwrap());
                } else if tc >= 9 && tc <= 18 {
                    let nz = 15;
                    let f_flag = byte_array[6] & (1 << 2);
                    let t_flag = byte_array[6] & (1 << 3);
                    let raw_lat = (((byte_array[6] as i32) & 3) << 15)
                        | ((byte_array[7] as i32) << 7)
                        | ((byte_array[8] as i32) >> 1);
                    let raw_long = (((byte_array[8] as i32) & 1) << 16)
                        | ((byte_array[9] as i32) << 8)
                        | (byte_array[10] as i32);
                    if f_flag != 0 {
                        if raw_lat != 0 {
                            odd_cprlat = raw_lat as f32 / 131072.0;
                        }
                        if raw_long != 0 {
                            odd_cprlon = raw_long as f32 / 131072.0;
                        }
                        odd_cprtime = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;
                    } else {
                        if raw_lat != 0 {
                            even_cprlat = raw_lat as f32 / 131072.0;
                        }
                        if raw_long != 0 {
                            even_cprlon = raw_long as f32 / 131072.0;
                        }
                        even_cprtime = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs() as i64;
                    }

                    if (even_cprtime - odd_cprtime).abs() <= 10 {
                        const AirDlat0: f32 = 360.0 / 60.;
                        const AirDlat1: f32 = 360.0 / 59.;
                        let j = (59.0 * even_cprlat - 60.0 * odd_cprlat + 0.5).floor() as i32;

                        let mut lat_even = AirDlat0 * ((j % 60) as f32 + even_cprlat);
                        let mut lat_odd = AirDlat1 * ((j % 59) as f32 + odd_cprlat);

                        if lat_even >= 270.0 {
                            lat_even -= 360.0;
                        }

                        if lat_odd >= 270.0 {
                            lat_odd -= 360.0;
                        }
                        if cprNL(lat_even) != cprNL(lat_odd) {
                            continue;
                        }

                        if even_cprtime > odd_cprtime {
                            let ni = cprN(lat_even, 0) as f32;
                            let m = (even_cprlon * (cprNL(lat_even) - 1) as f32
                                - odd_cprlon * cprNL(lat_even) as f32
                                + 0.5)
                                .floor();
                            longitude = (360.0 / ni) * (m % ni + even_cprlon);
                            latitude = lat_even;
                        } else {
                            let ni = cprN(lat_odd, 1) as f32;
                            let m = (even_cprlon * (cprNL(lat_odd - 1.0)) as f32
                                - odd_cprlon * cprNL(lat_odd) as f32
                                + 0.5)
                                .floor();
                            longitude = (360.0 / ni) * (m % ni + odd_cprlon);
                            latitude = lat_odd;
                        }

                        if longitude > 180.0 {
                            longitude -= 360.0
                        }

                        println!("lat: {latitude} lon: {longitude}");
                        return;
                    }
                }
            }
            5 => {
                println!("{}", line);
                println!("Flight Status: {}", byte_array[0] & 7);
                let dl_req = (byte_array[1] >> 3) & 31;
                if dl_req != 0 {
                    eprintln!("What the fuck this was unexpected: {}", line!());
                    break;
                }
                println!("Downlink Request: {}", dl_req);
                // TODO: this is just wrong
                let util_msg = ((byte_array[1] & 7) << 3) | byte_array[2] >> 5;
                if util_msg != 0 {
                    eprintln!("What the fuck this was unexpected: {}", line!());
                    break;
                }
                println!("Utility Message: {}", util_msg);

                let a = ((byte_array[3] & 0x80) >> 5)
                    | ((byte_array[2] & 0x02) >> 0)
                    | ((byte_array[2] & 0x08) >> 3);
                let b = ((byte_array[3] & 0x02) << 1)
                    | ((byte_array[3] & 0x08) >> 2)
                    | ((byte_array[3] & 0x20) >> 5);
                let c = ((byte_array[2] & 0x01) << 2)
                    | ((byte_array[2] & 0x04) >> 1)
                    | ((byte_array[2] & 0x10) >> 4);
                let d = ((byte_array[3] & 0x01) << 2)
                    | ((byte_array[3] & 0x04) >> 1)
                    | ((byte_array[3] & 0x10) >> 4);
                // TODO: fix integer types
                println!(
                    "Squawk: {}",
                    a as u32 * 1000 + b as u32 * 100 + c as u32 * 10 + d as u32
                );
            }
            num => {
                if num < 21 {
                } else {
                    eprintln!("Downlink code not recognized: {}", num);
                }
            }
        }
    }
}

fn hex_str_to_u128(hex_str: &str) -> Result<u128, ParseIntError> {
    u128::from_str_radix(&hex_str[1..hex_str.len() - 1], 16)
}

fn chop(data: &str) -> String {
    data[1..data.len() - 1].to_string()
}

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    if hex.len() % 2 != 0 {
        eprintln!("Hex string has an odd length");
        return vec![];
    }

    (1..hex.len() - 1)
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<u8>>()
}
