use std::net::TcpStream;
use std::io::{ BufRead, BufReader };
use std::env;
use std::time::{ Instant, Duration };
use std::fs;
use std::sync::Arc;

use couch_rs::types::find::FindQuery;

use geo::{point, HaversineDistance};
use warp::Filter;
use reqwest;
use serde_json::{ json, Value };
use dotenv::dotenv;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::io::{ AsyncBufReadExt, BufReader as AsyncBufReader };

fn message_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex.trim_matches(|c: char| !c.is_ascii_hexdigit());

    if hex.len() % 2 != 0 {
        panic!("Message has odd length");
    }

    (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

const AIS_CHARSET: &[u8] = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ#####_###############0123456789######".as_bytes();

#[tokio::main]
async fn main() {
    dotenv().ok();
    // Use Tokio's TcpStream for asynchronous operations
    let stream = TokioTcpStream::connect(&format!("{}:{}", env::var("DUMP_IP").unwrap(), env::var("DUMP_PORT").unwrap()))
        .await
        .unwrap();
    let reader = AsyncBufReader::new(stream);
    let mut lines = reader.lines();

    let db_host = format!(
        "http://{}:{}",
        env::var("DATABASE_IP").unwrap(),
        env::var("DATABASE_PORT").unwrap()
    );
    let db_name = env::var("DATABASE_NAME").unwrap();

    let client = couch_rs::Client::new(
        &db_host,
        &env::var("DATABASE_USER").unwrap(),
        &env::var("DATABASE_PASS").unwrap(),
    ).unwrap();
    let mut db = client.db(&db_name).await.unwrap();

    let db_shared = Arc::new(db);

    let db_clone = db_shared.clone();
    tokio::spawn(async move {
        let mut lastsec = Instant::now();

        loop {
            if Instant::now() - lastsec >= Duration::from_secs(1) {
                let mut uptime = db_clone.get::<Value>("uptime").await.unwrap_or(json!({}));
                uptime["seconds"] = json!(uptime["seconds"].as_i64().unwrap_or(0) + 1);
                db_clone.bulk_docs(&mut vec![uptime.clone()]).await.unwrap();
                lastsec = Instant::now();
            }
        }
    });

    let db_stats = db_shared.clone();
    tokio::spawn(async move {
        let stats_route = warp::path("stats")
            .and(warp::get())
            .and(warp::any().map(move || db_stats.clone())) 
            .and_then(move |db: Arc<couch_rs::database::Database>| async move { 
                let stats = db.get::<Value>("stats").await.unwrap_or(json!({}));
                let uptime = db.get::<Value>("uptime").await.unwrap_or(json!({}));

                let mut keys = vec![];
                let mut values = vec![];
                if let Some(object) = stats.as_object() {
                    for (key, value) in object {
                        if key.starts_with("msgtype:") {
                            keys.push(key.clone().replace("msgtype:", "DF "));
                            values.push(value.clone());
                        }
                    }
                }

                let messages = stats["messages"].as_f64().unwrap_or(1.0);
                let uptime_secs = uptime["seconds"].as_f64().unwrap_or(1.0);
                let unique_icao = stats["unique_icao"].as_i64().unwrap_or(0);
                let distance = stats["distance"].as_i64().unwrap_or(0);

                let html = fs::read_to_string("www/stats.html").unwrap()
                    .replace("{LABELS_MESSAGES_TYPE}", &format!("{:?}", keys))
                    .replace("{DATA_MESSAGES_TYPE}", &format!("{:?}", values))
                    .replace("{MESSAGES_P_SECOND}", &format!("~{:.3}", messages / uptime_secs))
                    .replace("{UNIQUE_ICAO}", &format!("{:?}", unique_icao))
                    .replace("{TOTAL_MESSAGES}", &format!("{:?}", messages as i32))
                    .replace("{UPTIME}", &format!("{:?}s", uptime_secs as i32))
                    .replace("{DISTANCE}", &format!("{:?}m", distance));

                Ok::<_, warp::Rejection>(warp::reply::html(html))
            });

        let files = warp::path::end().and(warp::fs::dir("www"));
        let routes = files.or(stats_route);

        warp::serve(routes)
            .run(([0, 0, 0, 0], env::var("PORT").unwrap().parse::<u16>().unwrap()))
            .await;
    });

    while let Some(message) = lines.next_line().await.unwrap() {
        let msg = message_to_bytes(&message);

        let df = msg[0] >> 3;
        let tc = msg[4] >> 3;
        let mut callsign = [0u8; 9];

        let address: u32 = (msg[1] as u32) << 16 | (msg[2] as u32) << 8 | (msg[3] as u32);
        let hex_address = format!("{:06X}", address);

        let mut flight_data = db_shared.get::<Value>("flight_data").await.unwrap_or(json!({}));
        let mut stats = db_shared.get::<Value>("stats").await.unwrap_or(json!({}));

        // TODO: Pick up the icao address from other message formats.
        if df == 11 || df == 17 {
            if let Some(flight_list) = flight_data["flight_list"].as_array_mut() {
                if !flight_list.contains(&json!(hex_address)) {
                    stats["unique_icao"] = json!(stats["unique_icao"].as_i64().unwrap_or(0) + 1);
                    flight_list.push(json!(hex_address));
                }
            } else {
                eprintln!("flight_list is not an array");
            }
        }
        if df == 17 {
            let flight_map = flight_data.as_object_mut().unwrap().entry(&hex_address).or_insert(json!({}));

            if tc >= 1 && tc <= 4 {
                if msg.len() < 11 {
                    eprintln!("Not enough data for callsign");
                } else {
                    callsign[0] = AIS_CHARSET[(msg[5]>>2) as usize];
                    callsign[1] = AIS_CHARSET[(((msg[5]&3)<<4)|(msg[6]>>4)) as usize];
                    callsign[2] = AIS_CHARSET[(((msg[6]&15)<<2)|(msg[7]>>6)) as usize];
                    callsign[3] = AIS_CHARSET[(msg[7]&63) as usize];
                    callsign[4] = AIS_CHARSET[(msg[8]>>2) as usize];
                    callsign[5] = AIS_CHARSET[(((msg[8]&3)<<4)|(msg[9]>>4)) as usize];
                    callsign[6] = AIS_CHARSET[(((msg[9]&15)<<2)|(msg[10]>>6)) as usize];
                    callsign[7] = AIS_CHARSET[(msg[10]&63) as usize];
                    callsign[8] = b'_';
                    let callsign = std::str::from_utf8(&callsign)
                        .unwrap()
                        .replace("_", "");
                    flight_map["callsign"] = json!(callsign);
                }
            } else if tc >= 9 && tc <= 18 {
                let response = reqwest::get("https://plane.thijmens.nl/data.json")
                    .await
                    .unwrap();
                let data: Value = response.json().await.unwrap();

                if let Some(aircraft_array) = data.as_array() {
                    for aircraft in aircraft_array {
                        if aircraft["hex"].as_str().unwrap().to_uppercase() == hex_address {
                            flight_map["lat"] = json!(aircraft["lat"]);
                            flight_map["lon"] = json!(aircraft["lon"]);
                            flight_map["alt"] = json!(aircraft["altitude"]);
                            flight_map["track"] = json!(aircraft["track"]);
                            flight_map["speed"] = json!(aircraft["speed"]);

                            let antenne = point!(x: env::var("MY_LAT").unwrap().parse::<f64>().unwrap(), y: env::var("MY_LON").unwrap().parse::<f64>().unwrap());
                            let aircraft_location = point!(x: aircraft["lat"].as_f64().unwrap(), y: aircraft["lon"].as_f64().unwrap());

                            let distance = antenne.haversine_distance(&aircraft_location);

                            if distance > stats["distance"].as_f64().unwrap() {
                                stats["distance"] = json!(distance as i64)
                            }
                        }
                    }
                } else {
                    eprintln!("data.json is not an array");
                }
            }
        }

        db_shared.bulk_docs(&mut vec![flight_data.clone()]).await.unwrap();

        stats["messages"] = json!(stats["messages"].as_i64().unwrap_or(0) + 1);

        let msg_type_key = &format!("msgtype:{}", df);
        stats[msg_type_key] = json!(stats[msg_type_key].as_i64().unwrap_or(0) + 1);

        db_shared.bulk_docs(&mut vec![stats.clone()]).await.unwrap();
    }
}