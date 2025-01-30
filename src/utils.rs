pub fn cprN(lat: f32, is_odd: i32) -> i32 {
    let mut nl = cprNL(lat) - is_odd;
    if nl < 1 {
        nl = 1;
    }
    return nl;
}

pub fn cprNL(lat: f32) -> i32 {
    let nz = 15.0;
    let a = 1.0 - (std::f32::consts::PI / (2.0 * nz)).cos();
    let b = (std::f32::consts::PI / 180.0 * lat.abs()).cos().powi(2);
    let nl = 2.0 * std::f32::consts::PI / (1.0 - a / b).acos();
    nl.floor() as i32
}

pub fn vec_to_u128(vec: &Vec<u8>) -> u128 {
    vec.iter().fold(0u128, |acc, &b| (acc << 8) | b as u128)
}

pub fn compute_crc(mut msg: u128, generator: u128) -> u32 {
    // TODO
    1
}