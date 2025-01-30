use crate::{
    ModeSMessage, Unit, MODES_CHECKSUM_TABLE, MODES_ICAO_CACHE_LEN, MODES_ICAO_CACHE_TTL,
    MODES_LONG_MSG_BITS,
};

use std::time::{SystemTime, UNIX_EPOCH};
use std::str;

pub fn brute_force_ap(msg: &Vec<u8>, mm: &mut ModeSMessage) -> bool {
    let mut aux: Vec<u8> = vec![0; MODES_LONG_MSG_BITS / 8];
    let msgtype = mm.msgtype;
    let msgbits = mm.msgbits;

    if msgtype == 0
        || msgtype == 4
        || msgtype == 5
        || msgtype == 16
        || msgtype == 20
        || msgtype == 21
        || msgtype == 24
    {
        let last_byte = (msgbits / 8) - 1;

        aux[..(msgbits / 8)].copy_from_slice(&msg[..(msgbits / 8)]);

        let crc = modes_checksum(&aux, msgbits);
        aux[last_byte] ^= (crc & 0xff) as u8;
        aux[last_byte - 1] ^= (crc >> 8 & 0xff) as u8;
        aux[last_byte - 2] ^= (crc >> 16 & 0xff) as u8;

        let addr = (aux[last_byte] as u32)
            | ((aux[last_byte - 1] as u32) << 8)
            | ((aux[last_byte - 2] as u32) << 16);

        if icaoaddress_was_recently_seen(addr, &mut mm.icao_cache.icao_cache) {
            mm.aa1 = aux[last_byte - 2];
            mm.aa2 = aux[last_byte - 1];
            mm.aa3 = aux[last_byte];
            return true;
        }
    }
    return false;
}

pub fn modes_checksum(msg: &Vec<u8>, bits: usize) -> u32 {
    let mut crc: u32 = 0;
    let offset = if bits == 112 { 0 } else { 112 - 56 };

    for j in 0..bits {
        let byte = j / 8;
        let bit = j % 8;
        let bitmask = 1 << (7 - bit);

        if msg[byte] & bitmask != 0 {
            crc ^= MODES_CHECKSUM_TABLE[(j + offset) as usize]
        }
    }
    return crc;
}

pub fn msg_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex.trim_matches(|c: char| !c.is_ascii_hexdigit());

    if hex.len() % 2 != 0 {
        eprintln!("Hex string has an odd length");
        return vec![];
    }

    (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

pub fn get_downlink_code(message: &Vec<u8>) -> u8 {
    message[0] >> 3
}

pub fn modes_message_len_by_id(id: u8) -> usize {
    if id == 16 || id == 17 || id == 19 || id == 20 || id == 21 {
        return 112;
    } else {
        return 56;
    }
}

pub fn fix_single_bit_errors(message: &mut Vec<u8>, bits: usize) -> i8 {
    let mut aux: Vec<u8> = vec![0; MODES_LONG_MSG_BITS / 8];

    for j in 0..bits {
        let byte = j / 8;
        let bitmask = 1 << (7 - (j % 8));

        aux[..(bits / 8)].copy_from_slice(&message[..(bits / 8)]);
        aux[byte] ^= bitmask;

        let crc1 = ((aux[(bits / 8) - 3] as u32) << 16)
            | ((aux[(bits / 8) - 2] as u32) << 8)
            | (aux[(bits / 8) - 1] as u32);
        let crc2 = modes_checksum(&aux, bits);

        if crc1 == crc2 {
            message[..(bits / 8)].copy_from_slice(&aux[..(bits / 8)]);
            return j.try_into().unwrap();
        }
    }
    return -1;
}

pub fn fix_two_bits_errors(message: &mut Vec<u8>, bits: usize) -> i8 {
    let mut aux: Vec<u8> = vec![0; MODES_LONG_MSG_BITS / 8];

    for j in 0..bits {
        let byte1 = j / 8;
        let bitmask1 = 1 << (7 - (j % 8));

        for i in (j + 1)..bits {
            let byte2 = i / 8;
            let bitmask2 = 1 << (7 - (i % 8));

            aux.copy_from_slice(&message[..(bits / 8)]);

            aux[byte1] ^= bitmask1;
            aux[byte2] ^= bitmask2;

            let crc1 = ((aux[(bits / 8) - 3] as u32) << 16)
                | ((aux[(bits / 8) - 2] as u32) << 8)
                | (aux[(bits / 8) - 1] as u32);
            let crc2 = modes_checksum(&aux, bits);

            if crc1 == crc2 {
                message.copy_from_slice(&aux[..(bits / 8)]);
                return (j | (i << 8)).try_into().unwrap();
            }
        }
    }
    return -1;
}

pub fn decode_ac13_field(message: &Vec<u8>, unit: &mut Unit) -> i32 {
    let m_bit = message[3] & (1 << 6);
    let q_bit = message[3] & (1 << 4);

    if m_bit == 0 {
        *unit = Unit::FEET;
        if q_bit != 0 {
            let n = (((message[2] & 31) << 6)
                | ((message[3] & 0x80) >> 2)
                | ((message[3] & 0x20) >> 1)
                | (message[3] & 15)) as i32;

            return n * 25 - 1000;
        } else {
        }
    } else {
        *unit = Unit::METERS;
    }
    return 0;
}

pub fn decode_ac12_field(message: &Vec<u8>, unit: &mut Unit) -> i32 {
    let q_bit = message[5] & 1;

    if q_bit != 0 {
        *unit = Unit::FEET;
        let n = (((message[5] >> 1) << 4) | ((message[6] & 0xF0) >> 4)) as i32;
        return n * 25 - 1000;
    } else {
        return 0;
    }
}

pub fn icaocache_hash_address(a: u32) -> u32 {
    let mut a = a;
    a = ((a >> 16) ^ a).wrapping_mul(0x45d9f3b);
    a = ((a >> 16) ^ a).wrapping_mul(0x45d9f3b);
    a = (a >> 16) ^ a;
    return a & (MODES_ICAO_CACHE_LEN - 1);
}

pub fn add_recently_seen_icaoaddr(addr: u32, icao_cache: &mut Vec<u32>) {
    let h = icaocache_hash_address(addr);
    icao_cache[h as usize * 2] = addr;
    icao_cache[h as usize * 2 + 1] = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as u32;
}

pub fn icaoaddress_was_recently_seen(addr: u32, icao_cache: &mut Vec<u32>) -> bool {
    let h = icaocache_hash_address(addr);
    let a = icao_cache[h as usize * 2];
    let t = icao_cache[h as usize * 2 + 1];

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as u32;

    return a != 0 && a == addr && (time - t) <= MODES_ICAO_CACHE_TTL;
}
