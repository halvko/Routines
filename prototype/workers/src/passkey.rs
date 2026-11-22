//! Passkey registration and sign-in with `webauthn-rs` (decision 0004),
//! running inside the user's Durable Object. The ceremony state lives in the
//! object's storage between "start" and "finish", and the registered
//! passkeys are stored there too.

use webauthn_rs::prelude::*;
use worker::{Env, Request, Response, Result, State};

fn webauthn(env: &Env) -> Result<Webauthn> {
    let rp_id = env.var("RP_ID").map(|v| v.to_string()).unwrap_or_else(|_| "localhost".into());
    let origin = env
        .var("RP_ORIGIN")
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "http://localhost:8787".into());
    let origin = Url::parse(&origin).map_err(|e| e.to_string())?;
    WebauthnBuilder::new(&rp_id, &origin)
        .map_err(|e| e.to_string())?
        .rp_name("Routines")
        .build()
        .map_err(|e| e.to_string().into())
}

fn err(e: WebauthnError) -> worker::Error {
    e.to_string().into()
}

pub async fn handle(state: &State, env: &Env, user: &str, mut req: Request, step: &str) -> Result<Response> {
    let storage = state.storage();
    let wa = webauthn(env)?;
    let passkeys: Vec<Passkey> = storage.get("passkeys").await?.unwrap_or_default();

    match step {
        "register/start" => {
            let user_id: Uuid = match storage.get("user_uuid").await? {
                Some(id) => id,
                None => {
                    let mut bytes = [0u8; 16];
                    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
                    let id = Uuid::from_bytes(bytes);
                    storage.put("user_uuid", id).await?;
                    id
                }
            };
            let exclude = passkeys.iter().map(|p| p.cred_id().clone()).collect();
            let (challenge, reg) = wa
                .start_passkey_registration(user_id, user, user, Some(exclude))
                .map_err(err)?;
            storage.put("reg_state", reg).await?;
            Response::from_json(&challenge)
        }
        "register/finish" => {
            let credential: RegisterPublicKeyCredential = req.json().await?;
            let Some(reg) = storage.get::<PasskeyRegistration>("reg_state").await? else {
                return Response::error("no registration in progress", 400);
            };
            storage.delete("reg_state").await?;
            match wa.finish_passkey_registration(&credential, &reg) {
                Ok(passkey) => {
                    let mut all = passkeys;
                    all.push(passkey);
                    storage.put("passkeys", &all).await?;
                    Response::from_json(&serde_json::json!({ "registered": all.len() }))
                }
                Err(e) => Response::error(format!("registration failed: {e}"), 400),
            }
        }
        "login/start" => {
            if passkeys.is_empty() {
                return Response::error("no passkeys", 400);
            }
            let (challenge, auth) = wa.start_passkey_authentication(&passkeys).map_err(err)?;
            storage.put("auth_state", auth).await?;
            Response::from_json(&challenge)
        }
        "login/finish" => {
            let credential: PublicKeyCredential = req.json().await?;
            let Some(auth) = storage.get::<PasskeyAuthentication>("auth_state").await? else {
                return Response::error("no sign-in in progress", 400);
            };
            storage.delete("auth_state").await?;
            match wa.finish_passkey_authentication(&credential, &auth) {
                Ok(result) => {
                    // Keep the signature counter and backup state current.
                    let mut all = passkeys;
                    for p in all.iter_mut() {
                        p.update_credential(&result);
                    }
                    storage.put("passkeys", &all).await?;
                    // A real server would now issue an opaque session cookie (0004).
                    Response::from_json(&serde_json::json!({
                        "signedIn": true,
                        "userVerified": result.user_verified(),
                    }))
                }
                Err(e) => Response::error(format!("sign-in failed: {e}"), 401),
            }
        }
        _ => Response::error("not found", 404),
    }
}
