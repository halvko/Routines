//! Argon2id at OWASP's minimum (m = 19 MiB, t = 2, p = 1), the parameters
//! decision 0004 plans for the password fallback.

use argon2::{Algorithm, Argon2, Params, Version};
use worker::*;

pub fn argon2() -> Result<Response> {
    let params = Params::new(19 * 1024, 2, 1, Some(32)).map_err(|e| e.to_string())?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| e.to_string())?;
    let mut out = [0u8; 32];
    // Timers in Workers only advance across I/O, so this in-request figure
    // can read 0 when deployed; the client measures the round trip too.
    let start = Date::now().as_millis();
    argon
        .hash_password_into(b"correct horse battery staple", &salt, &mut out)
        .map_err(|e| e.to_string())?;
    let ms = Date::now().as_millis() - start;
    Response::from_json(&serde_json::json!({
        "params": "argon2id m=19456 t=2 p=1",
        "inRequestMs": ms,
        "hashPrefix": format!("{:02x}{:02x}{:02x}{:02x}", out[0], out[1], out[2], out[3]),
    }))
}
