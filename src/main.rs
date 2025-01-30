mod helper;
use helper::*;
mod types;
use types::*;
mod data;
use data::*;

use std::f32::consts::PI;
use std::fs::read_to_string;
use std::io::{BufRead, BufReader, Cursor};
use std::net::TcpStream;
use redis::{ Commands, Connection };
use dotenv::dotenv;
use std::env;

use couch_rs::types::find::FindQuery;
use std::error::Error;
use serde_json::Value;
use couch_rs::document::DocumentCollection;



#[tokio::main]
async fn main() {
    dotenv().ok();
    let db_host: &str = &format!("http://{}:{}", env::var("DATABASE_IP").unwrap(), env::var("DATABASE_PORT").unwrap());
    let db_name: &str = &env::var("DATABASE_NAME").unwrap();

    let client = couch_rs::Client::new(db_host, &env::var("DATABASE_USER").unwrap(), &env::var("DATABASE_PASS").unwrap()).unwrap();
    let mut db = client.db(db_name).await.unwrap();

    let lines = if cfg!(debug_assertions) {
        // Read from a file in debug mode
        let file_content = read_to_string("data_large.adsb").unwrap();
        let cursor = Cursor::new(file_content);
        BufReader::new(Box::new(cursor) as Box<dyn std::io::Read>)
    } else {
        // Read from a TCP stream in release mode
        let stream = TcpStream::connect(format!("{}:{}", env::var("DUMP_IP").unwrap(), env::var("DUMP_PORT").unwrap())).unwrap();
        BufReader::new(Box::new(stream) as Box<dyn std::io::Read>)
    };

    for line in lines.lines() {
        let line = line.unwrap();
        let data = decomp_data(&line);
        process_data(data, &mut db).await;
    }
}

fn decomp_data(line: &str) -> ModeSMessage {
    let mut data = ModeSMessage {
        ..Default::default()
    };
    let mut message = msg_to_bytes(&line);
    data.msgtype = get_downlink_code(&message);
    data.msgbits = modes_message_len_by_id(data.msgtype);
    data.crc = ((message[(56 / 8) - 3] as u32) << 16)
        | ((message[(56 / 8) - 2] as u32) << 8)
        | message[(56 / 8) - 1] as u32;
    let crc2 = modes_checksum(&message, data.msgbits);

    data.errorbit = -1;
    data.crcok = data.crc == crc2;

    if !data.crcok && FIX_ERRORS && (data.msgtype == 11 || data.msgtype == 17) {
        let singlebit = fix_single_bit_errors(&mut message, data.msgbits);
        let twobit = fix_two_bits_errors(&mut message, data.msgbits);
        if singlebit != -1 {
            data.crc = modes_checksum(&message, data.msgbits);
            data.errorbit = singlebit;
        } else if AGGRESSIVE && data.msgtype == 17 && twobit != -1 {
            data.crc = modes_checksum(&message, data.msgbits);
            data.errorbit = twobit;
        }
        data.crcok = true;
    }

    data.ca = message[0] & 7;

    data.aa1 = message[1];
    data.aa2 = message[2];
    data.aa3 = message[3];

    data.metype = (message[4] >> 3) as i32;
    data.mesub = (message[4] & 7) as i32;

    data.fs = (message[0] & 7) as i32;
    data.dr = (message[1] >> 3 & 31) as i32;
    data.um = (((message[1] & 7) << 3) | message[2] >> 5) as i32;

    {
        let a = (((message[3] & 0x80) >> 5)
            | ((message[2] & 0x02) >> 0)
            | ((message[2] & 0x08) >> 3)) as u16;
        let b: u16 = (((message[3] & 0x02) << 1)
            | ((message[3] & 0x08) >> 2)
            | ((message[3] & 0x20) >> 5)) as u16;
        let c: u16 = (((message[2] & 0x01) << 2)
            | ((message[2] & 0x04) >> 1)
            | ((message[2] & 0x10) >> 4)) as u16;
        let d: u16 = (((message[3] & 0x01) << 2)
            | ((message[3] & 0x04) >> 1)
            | ((message[3] & 0x10) >> 4)) as u16;
        data.squawka = (a * 1000 + b * 100 + c * 10 + d) as i32;
    }

    if data.msgtype != 11 && data.msgtype != 17 {
        if brute_force_ap(&message, &mut data) {
            data.crcok = true;
        } else {
            data.crcok = false;
        }
    } else {
        if data.crcok && data.errorbit == -1 {
            let addr = ((data.aa1 as u32) << 16) | ((data.aa2 as u32) << 8) | (data.aa3 as u32);
            add_recently_seen_icaoaddr(addr, &mut data.icao_cache.icao_cache);
        }
    }

    if data.msgtype == 0 || data.msgtype == 4 || data.msgtype == 16 || data.msgtype == 20 {
        data.altitude = decode_ac13_field(&message, &mut data.unit);
    }

    if data.msgtype == 17 {
        if data.metype >= 1 && data.metype <= 4 {
            data.aircraft_type = data.metype - 1;
            data.flight[0] = AIS_CHARSET[(message[5] >> 2) as usize];
            data.flight[1] = AIS_CHARSET[(((message[5] & 3) << 4) | (message[6] >> 4)) as usize];
            data.flight[2] = AIS_CHARSET[(((message[6] & 15) << 2) | (message[7] >> 6)) as usize];
            data.flight[3] = AIS_CHARSET[(message[7] & 63) as usize];
            data.flight[4] = AIS_CHARSET[(message[8] >> 2) as usize];
            data.flight[5] = AIS_CHARSET[(((message[8] & 3) << 4) | (message[9] >> 4)) as usize];
            data.flight[6] = AIS_CHARSET[(((message[9] & 15) << 2) | (message[10] >> 6)) as usize];
            data.flight[7] = AIS_CHARSET[(message[10] & 63) as usize];
            data.flight[8] = b'\0';
            println!("In code: {:?}", data.flight)
        } else if data.metype >= 9 && data.metype <= 18 {
            data.fflag = (message[6] & (1 << 2)) as i32;
            data.tflag = (message[6] & (1 << 3)) as i32;
            data.altitude = decode_ac12_field(&message, &mut data.unit);
            data.raw_latitude = (((message[6] as u32 & 3) << 15)
                | ((message[7] as u32) << 7)
                | ((message[8] as u32) >> 1)) as i32;
            data.raw_longitude = (((message[8] as u32 & 1) << 16)
                | ((message[9] as u32) << 8)
                | (message[10] as u32)) as i32;
        } else if data.metype == 19 && data.mesub >= 1 && data.mesub <= 4 {
            if data.mesub == 1 || data.mesub == 2 {
                data.ew_dir = ((message[5] & 4) >> 2) as i32;
                data.ew_velocity = (((message[5] as u32 & 3) << 8) | message[6] as u32) as i32;
                data.ns_dir = ((message[7] & 0x80) >> 7) as i32;
                data.ns_velocity = (((message[7] & 0x7f) << 3) | ((message[8] & 0xe0) >> 5)) as i32;
                data.vert_rate_source = ((message[8] & 0x10) >> 4) as i32;
                data.vert_rate_sign = ((message[8] & 0x8) >> 3) as i32;
                data.vert_rate = (((message[8] & 7) << 6) | ((message[9] & 0xfc) >> 2)) as i32;
                data.velocity = ((data.ns_velocity * data.ns_velocity
                    + data.ew_velocity * data.ew_velocity) as f32)
                    .sqrt() as i32;
                if data.velocity != 0 {
                    let mut ewv: f32 = data.ew_velocity as f32;
                    let mut nsv: f32 = data.ns_velocity as f32;
                    let heading;

                    if data.ew_dir != 0 {
                        ewv *= -1.0;
                    }
                    if data.ns_dir != 0 {
                        nsv *= -1.0;
                    }
                    heading = nsv.atan2(ewv);

                    data.heading = (heading * 360.0 / (PI * 2.0)) as i32;
                    if data.heading < 0 {
                        data.heading += 360;
                    }
                } else {
                    data.heading = 0;
                }
            } else if data.mesub == 3 || data.mesub == 4 {
                data.heading_is_valid = (message[5] & (1 << 2)) as i32;
                data.heading =
                    ((360.0 / 128.0) as i32) * (((message[5] & 3) << 5) | (message[6] >> 3)) as i32;
            }
        }
    }
    data
}
