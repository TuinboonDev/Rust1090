use crate::{ ModeSMessage, FindQuery, Value};
use couch_rs::database::Database;
use serde_json::{json, Map};
use std::{collections::HashMap, str};

pub async fn process_data(data: ModeSMessage, db: &mut Database) {
    let mut flight_data = db.get::<Value>("flight_data").await.unwrap();
    let flight_list = flight_data["flight_list"].as_array_mut().unwrap();
    let hex = format!("{:02x}{:02x}{:02x}", data.aa1, data.aa2, data.aa3);

    if !flight_list.contains(&json!(hex)) { 
        flight_list.push(json!(hex));
    }


    let map = json!({});
    let flight = flight_data.as_object_mut().unwrap().entry(hex).or_insert_with(|| map);
    let flight_map = flight.as_object_mut().unwrap();
    if data.msgtype == 17 {
        if data.metype >= 1 && data.metype <= 4 {
            let flight_str = str::from_utf8(&data.flight).unwrap().trim_matches(|c| c == ' ' || c == '\0');
            flight_map.insert("flight".to_string(), json!(flight_str));            
        } else if data.metype >= 9 && data.metype <= 18 {
            flight_map.insert("lat".to_string(), json!(data.raw_latitude));            
            flight_map.insert("lon".to_string(), json!(data.raw_longitude));
            flight_map.insert("alt".to_string(), json!((data.altitude) as f64 / 3.2828));
        }
        if data.metype == 19 {
            if data.mesub == 1 || data.mesub == 2 {
                flight_map.insert("speed".to_string(), json!((data.velocity as f64) * 1.852));            
                flight_map.insert("track".to_string(), json!(data.heading));            
            }
        }
    }
    if data.msgtype == 0 || data.msgtype == 4 || data.msgtype == 20 {
        println!("{} {}", data.altitude, (data.altitude) as f64 / 3.2828);           
        flight_map.insert("alt".to_string(), json!((data.altitude) as f64 / 3.2828));            
    }
    db.bulk_docs(&mut vec![flight_data.clone()]).await.unwrap();


    let mut stats = db.get::<Value>("stats").await.unwrap();
    stats["messages"] = json!(stats["messages"].as_i64().unwrap_or(0) + 1);

    db.bulk_docs(&mut vec![stats.clone()]).await.unwrap();

    let msg_type_key = &format!("stats:msgtype:{}", data.msgtype);
    stats[msg_type_key] = json!(stats[msg_type_key].as_i64().unwrap_or(0) + 1);

    // if data.altitude > 0 {
    //     let altitude_sum_key = "stats:altitude_sum";
    //     let altitude_count_key = "stats:altitude_count";
    //     let _: () = conn.incr(altitude_sum_key, data.altitude).unwrap();
    //     let _: () = conn.incr(altitude_count_key, 1).unwrap();
    // }

}   