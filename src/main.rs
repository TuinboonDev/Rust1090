use core::str;
use std::{ collections::HashMap, io::{ BufRead, BufReader }, net::TcpStream };
use std::time::{ UNIX_EPOCH, SystemTime };
use std::env;

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

fn byte_to_bitvec(bytes: &Vec<u8>) -> Vec<u8> {
    let mut bitvec = Vec::new();
    for byte in bytes {
        for i in 0..8 {
            bitvec.push((byte >> i) & 1);
        }
    }
    bitvec
}

fn crc_check(msg: &mut Vec<u8>, generator: &Vec<u8>) -> bool {
    let mut crc = vec![0; generator.len() - 1];
    let mut msg = msg.clone();
    msg.append(&mut crc);

    let mut msg = byte_to_bitvec(&msg);
    let generator = byte_to_bitvec(generator);

    for i in 0..msg.len() - generator.len() {
        if msg[i] == 1 {
            for j in 0..generator.len() {
                msg[i + j] ^= generator[j];
            }
        }
    }

    msg.iter().all(|&x| x == 0)
}

fn message_len(df: u8) -> usize {
    if df == 16 || df == 17 ||
        df == 19 || df == 20 ||
        df == 21 {
        return 112;
    }
    else {
        return 56;
    }
}

fn checksum(msg: &Vec<u8>, msglen: usize) -> u32 {
    let mut crc = 0;
    let offset = if msglen == 112 { 0 } else { 56 };

    for j in 0..msglen {
        let byte = j / 8;
        let bit =  j % 8;
        let bitmask = 1 << (7 - bit);

        if msg[byte] & bitmask != 0 {
            crc ^= CHECKSUM_TABLE[j + offset];
        }
    }
    return crc;
}

// Useless
// fn fix_singlebit(msg: &Vec<u8>, msglen: usize) -> i32 {
//     1
// }

// fn fix_twobits(msg: &Vec<u8>, msglen: usize) -> i32 {
//     1
// }

fn decode_ac12(msg: &Vec<u8>) -> i32 {
    let q_bit = msg[5] & 1;
    let m_bit = (msg[5] >> 4) & 1;
    println!("{m_bit}, {q_bit}");

    if q_bit != 0 {
        let n: u16 = ((((msg[5] >> 1) & 0x0F) << 4) | ((msg[6] & 0xF0) >> 4)) as u16;
        return (n as i32) * 25 - 1000;
    } else {
        return 0;
    }
}

struct Aircraft {
    altitude: i32,
    odd_cprlat: i32,
    odd_cprlon: i32,
    even_cprlat: i32,
    even_cprlon: i32,
    odd_cprtime: i64,
    even_cprtime: i64,

    lat: f32,
    lon: f32,
}

const CHECKSUM_TABLE: [u32; 112] = [
    0x3935ea, 0x1c9af5, 0xf1b77e, 0x78dbbf, 0xc397db, 0x9e31e9, 0xb0e2f0, 0x587178,
    0x2c38bc, 0x161c5e, 0x0b0e2f, 0xfa7d13, 0x82c48d, 0xbe9842, 0x5f4c21, 0xd05c14,
    0x682e0a, 0x341705, 0xe5f186, 0x72f8c3, 0xc68665, 0x9cb936, 0x4e5c9b, 0xd8d449,
    0x939020, 0x49c810, 0x24e408, 0x127204, 0x093902, 0x049c81, 0xfdb444, 0x7eda22,
    0x3f6d11, 0xe04c8c, 0x702646, 0x381323, 0xe3f395, 0x8e03ce, 0x4701e7, 0xdc7af7,
    0x91c77f, 0xb719bb, 0xa476d9, 0xadc168, 0x56e0b4, 0x2b705a, 0x15b82d, 0xf52612,
    0x7a9309, 0xc2b380, 0x6159c0, 0x30ace0, 0x185670, 0x0c2b38, 0x06159c, 0x030ace,
    0x018567, 0xff38b7, 0x80665f, 0xbfc92b, 0xa01e91, 0xaff54c, 0x57faa6, 0x2bfd53,
    0xea04ad, 0x8af852, 0x457c29, 0xdd4410, 0x6ea208, 0x375104, 0x1ba882, 0x0dd441,
    0xf91024, 0x7c8812, 0x3e4409, 0xe0d800, 0x706c00, 0x383600, 0x1c1b00, 0x0e0d80,
    0x0706c0, 0x038360, 0x01c1b0, 0x00e0d8, 0x00706c, 0x003836, 0x001c1b, 0xfff409,
    0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000,
    0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000, 0x000000
];

const AIS_CHARSET: &[u8] = "#ABCDEFGHIJKLMNOPQRSTUVWXYZ#####_###############0123456789######".as_bytes();

fn main() {
    let stream = TcpStream::connect(env::var("DUMP_ADDRESS").unwrap()).unwrap();
    let reader = BufReader::new(Box::new(stream));

    let generator = (0b1111111111111010000001001 as i32).to_be_bytes().to_vec();
    let mut aircraft_map: HashMap<String, Aircraft> = HashMap::new();
    for message in reader.lines() {
        let message = message.unwrap();
        let mut msg = message_to_bytes(&message);

        let df = msg[0] >> 3;

        let msg_len = message_len(df);
        let mut crc = ((msg[(msg_len/8)-3] as u32) << 16) | ((msg[(msg_len/8)-2] as u32) << 8) | (msg[(msg_len/8)-1] as u32);
        let crc2 = checksum(&msg, msg_len);

        let mut error_bit = -1;
        let mut crc_ok = crc == crc2;

        // if !crc_ok && true && (df == 11 || df == 17) {
        //     panic!("CRC error, trying to correct");
        //     error_bit = fix_singlebit(&msg, msg_len);
        //     if error_bit != -1 {
        //         crc = checksum(&msg, msg_len);
        //         crc_ok = true;
        //     } else if false && df == 17 {
        //         error_bit = fix_twobits(&msg, msg_len);
        //         if error_bit != -1 {
        //             crc = checksum(&msg, msg_len);
        //             crc_ok = true;
        //         }
        //     }
        // }

        let tc = msg[4] >> 3;
        let address: u32 = (msg[1] as u32) << 16 | (msg[2] as u32) << 8 | (msg[3] as u32);

        let mut callsign = [0u8; 9];

        match df {
            17 => {
                if tc >= 1 && tc <= 4 {
                    callsign[0] = AIS_CHARSET[(msg[5]>>2) as usize];
                    callsign[1] = AIS_CHARSET[(((msg[5]&3)<<4)|(msg[6]>>4)) as usize];
                    callsign[2] = AIS_CHARSET[(((msg[6]&15)<<2)|(msg[7]>>6)) as usize];
                    callsign[3] = AIS_CHARSET[(msg[7]&63) as usize];
                    callsign[4] = AIS_CHARSET[(msg[8]>>2) as usize];
                    callsign[5] = AIS_CHARSET[(((msg[8]&3)<<4)|(msg[9]>>4)) as usize];
                    callsign[6] = AIS_CHARSET[(((msg[9]&15)<<2)|(msg[10]>>6)) as usize];
                    callsign[7] = AIS_CHARSET[(msg[10]&63) as usize];
                    callsign[8] = b'\0';
                    println!("{:?}", std::str::from_utf8(&callsign).unwrap());
                } else if tc >= 9 && tc <= 18 {
                    if true && !crc_ok {
                        continue;
                    }
                    let f_flag = msg[6] & (1<<2);
                    let t_flag = msg[6] & (1<<3);
                    let altitude = decode_ac12(&msg);
                    println!("Addres: {:06X}, Altitude: {}", address, altitude);
                    let value = Aircraft{ altitude, odd_cprlat: 0, odd_cprlon: 0, even_cprlat: 0, even_cprlon: 0, odd_cprtime: 0, even_cprtime: 0, lat: 0.0, lon: 0.0 };
                    let mut aircraft = aircraft_map.entry(address.to_string()).or_insert(value);
                    let raw_lat = (((msg[6] & 3) as i32) << 15) |
                                ((msg[7] as i32) << 7) |
                                ((msg[8] as i32) >> 1);
                    let raw_lon = (((msg[8] & 1) as i32) << 16) |
                                 ((msg[9] as i32) << 8) |
                                 (msg[10] as i32);

                    if f_flag != 0 {
                        aircraft.odd_cprlat = raw_lat;
                        aircraft.odd_cprlon = raw_lon;
                        aircraft.odd_cprtime = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                    } else {
                        aircraft.even_cprlat = raw_lat;
                        aircraft.even_cprlon = raw_lon;
                        aircraft.even_cprtime = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
                    }

                    if aircraft.even_cprtime - aircraft.odd_cprtime <= 10 {
                        decode_cpr(&mut aircraft);
                        println!("Lat: {}, Lon: {}", aircraft.lat, aircraft.lon);
                    }
                }
            }
            num => {
                if num > 24 {
                    panic!("Ehhm, this should not be possible")
                }
            }
        }
    }
}

fn cprModFunction(a: i32, b: i32) -> i32{
    let mut res = a % b;
    if res < 0 { res += b; }
    return res;
}

fn cprNLFunction(mut lat: f32) -> i32 {
    if lat < 0.0 { lat = -lat; }
    if lat < 10.47047130 { return 59; }
    if lat < 14.82817437 { return 58; }
    if lat < 18.18626357 { return 57; }
    if lat < 21.02939493 { return 56; }
    if lat < 23.54504487 { return 55; }
    if lat < 25.82924707 { return 54; }
    if lat < 27.93898710 { return 53; }
    if lat < 29.91135686 { return 52; }
    if lat < 31.77209708 { return 51; }
    if lat < 33.53993436 { return 50; }
    if lat < 35.22899598 { return 49; }
    if lat < 36.85025108 { return 48; }
    if lat < 38.41241892 { return 47; }
    if lat < 39.92256684 { return 46; }
    if lat < 41.38651832 { return 45; }
    if lat < 42.80914012 { return 44; }
    if lat < 44.19454951 { return 43; }
    if lat < 45.54626723 { return 42; }
    if lat < 46.86733252 { return 41; }
    if lat < 48.16039128 { return 40; }
    if lat < 49.42776439 { return 39; }
    if lat < 50.67150166 { return 38; }
    if lat < 51.89342469 { return 37; }
    if lat < 53.09516153 { return 36; }
    if lat < 54.27817472 { return 35; }
    if lat < 55.44378444 { return 34; }
    if lat < 56.59318756 { return 33; }
    if lat < 57.72747354 { return 32; }
    if lat < 58.84763776 { return 31; }
    if lat < 59.95459277 { return 30; }
    if lat < 61.04917774 { return 29; }
    if lat < 62.13216659 { return 28; }
    if lat < 63.20427479 { return 27; }
    if lat < 64.26616523 { return 26; }
    if lat < 65.31845310 { return 25; }
    if lat < 66.36171008 { return 24; }
    if lat < 67.39646774 { return 23; }
    if lat < 68.42322022 { return 22; }
    if lat < 69.44242631 { return 21; }
    if lat < 70.45451075 { return 20; }
    if lat < 71.45986473 { return 19; }
    if lat < 72.45884545 { return 18; }
    if lat < 73.45177442 { return 17; }
    if lat < 74.43893416 { return 16; }
    if lat < 75.42056257 { return 15; }
    if lat < 76.39684391 { return 14; }
    if lat < 77.36789461 { return 13; }
    if lat < 78.33374083 { return 12; }
    if lat < 79.29428225 { return 11; }
    if lat < 80.24923213 { return 10; }
    if lat < 81.19801349 { return 9; }
    if lat < 82.13956981 { return 8; }
    if lat < 83.07199445 { return 7; }
    if lat < 83.99173563 { return 6; }
    if lat < 84.89166191 { return 5; }
    if lat < 85.75541621 { return 4; }
    if lat < 86.53536998 { return 3; }
    if lat < 87.00000000 { return 2; }
    else { return 1; }
}

fn cprNFunction(lat: f32, isodd: i32) -> i32 {
    let mut nl = cprNLFunction(lat) - isodd;
    if nl < 1 { nl = 1; }
    return nl;
}

fn cprDlonFunction(lat: f32, isodd: i32) -> f32 {
    return 360.0 / cprNFunction(lat, isodd) as f32;
}

fn decode_cpr(aircraft: &mut Aircraft) {
    const AirDlat0: f32 = 360.0 / 60.0;
    const AirDlat1: f32 = 360.0 / 59.0;
    let lat0: f32 = aircraft.even_cprlat as f32;
    let lat1: f32 = aircraft.odd_cprlat as f32;
    let lon0: f32 = aircraft.even_cprlon as f32;
    let lon1: f32 = aircraft.odd_cprlon as f32;

    let j: i32 = (((59.0*lat0 - 60.0*lat1) / 131072.0) + 0.5).floor() as i32;
    let mut rlat0: f32 = AirDlat0 * (cprModFunction(j,60) as f32 + lat0 / 131072.0);
    let mut rlat1: f32 = AirDlat1 * (cprModFunction(j,59) as f32 + lat1 / 131072.0);

    if rlat0 >= 270.0 { rlat0 -= 360.0; }
    if rlat1 >= 270.0 { rlat1 -= 360.0; }

    if cprNLFunction(rlat0) != cprNLFunction(rlat1) { return }

    if aircraft.even_cprtime > aircraft.odd_cprtime {
        let ni = cprNFunction(rlat0,0);
        let m = ((((lon0 * (cprNLFunction(rlat0)-1) as f32) -
                        (lon1 * cprNLFunction(rlat0) as f32)) / 131072.0) + 0.5).floor() as i32;
        aircraft.lon = cprDlonFunction(rlat0,0) * (cprModFunction(m,ni) as f32 +lon0/131072.0);
        aircraft.lat = rlat0;
    } else {
        let ni = cprNFunction(rlat1,1);
        let m = ((((lon0 * (cprNLFunction(rlat1)-1) as f32) -
                        (lon1 * cprNLFunction(rlat1) as f32)) / 131072.0) + 0.5).floor() as i32;
        aircraft.lon = cprDlonFunction(rlat1,1) * (cprModFunction(m,ni) as f32 +lon1/131072.0);
        aircraft.lat = rlat1;
    }
    if aircraft.lon > 180.0 { aircraft.lon -= 360.0; }
}