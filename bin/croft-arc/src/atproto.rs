//! The PDS and admit HTTP client — the arc's one seam to the network.
//!
//! Every URL and field name here was confirmed against a running PDS on
//! 2026-09-14 (`createSession` → `accessJwt`/`refreshJwt`/`did`/`handle`;
//! `getServiceAuth` → `token`; the endpoint record's five fields) or comes
//! from the app's own XRPC layer (`caps/Xrpc.kt`: resolveHandle via the
//! public AppView, the DID document's `#atproto_pds` service, listRecords).
//! Nothing is inferred.
//!
//! Blocking, because the port's surface is synchronous and the binary owns
//! no runtime; `reqwest::blocking` starts its own.

use std::time::Duration;

use serde_json::Value;

use crate::records::{parse_record, EndpointRecord, ENDPOINT_COLLECTION};
use crate::session::{access_expiry_secs, RefreshFailure, StoredSession};
use crate::ArcError;

/// The public AppView, for handle resolution (unauthenticated).
pub const APPVIEW: &str = "https://public.api.bsky.app";

/// Why `getServiceAuth` did not mint a proof.
#[derive(Debug)]
pub enum ServiceAuthError {
    /// The PDS said the access token is expired: refresh and retry once.
    Expired,
    /// Anything else.
    Other(ArcError),
}

/// The client.
#[derive(Debug)]
pub struct Client {
    http: reqwest::blocking::Client,
}

impl Client {
    /// A client with a sane timeout.
    pub fn new() -> Result<Self, ArcError> {
        let http = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent("croft-arc/0.1")
            .build()
            .map_err(|e| ArcError::Http {
                what: "client",
                reason: e.to_string(),
            })?;
        Ok(Client { http })
    }

    /// Bare handle (a leading `@` dropped) → DID, via the public AppView.
    pub fn resolve_handle(&self, handle: &str) -> Result<String, ArcError> {
        let clean = handle.trim().trim_start_matches('@').to_lowercase();
        let v = self.get_json(
            "resolveHandle",
            &format!(
                "{APPVIEW}/xrpc/com.atproto.identity.resolveHandle?handle={}",
                enc(&clean)
            ),
        )?;
        v.get("did")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ArcError::Resolve {
                who: handle.to_string(),
                reason: "the AppView answered without a did".to_string(),
            })
    }

    /// DID → PDS base URL, from the DID document's `#atproto_pds` service.
    pub fn resolve_pds(&self, did: &str) -> Result<String, ArcError> {
        let doc_url = if let Some(plc) = did.strip_prefix("did:plc:") {
            format!("https://plc.directory/did:plc:{}", enc(plc))
        } else if let Some(web) = did.strip_prefix("did:web:") {
            let host = web.split(':').next().unwrap_or_default();
            format!("https://{host}/.well-known/did.json")
        } else {
            return Err(ArcError::Resolve {
                who: did.to_string(),
                reason: "unsupported DID method".to_string(),
            });
        };
        let doc = self.get_json("did document", &doc_url)?;
        doc.get("service")
            .and_then(Value::as_array)
            .and_then(|services| {
                services.iter().find(|s| {
                    let id = s.get("id").and_then(Value::as_str).unwrap_or_default();
                    let ty = s.get("type").and_then(Value::as_str).unwrap_or_default();
                    id.ends_with("#atproto_pds") || ty == "AtprotoPersonalDataServer"
                })
            })
            .and_then(|s| s.get("serviceEndpoint").and_then(Value::as_str))
            .map(str::to_string)
            .ok_or_else(|| ArcError::Resolve {
                who: did.to_string(),
                reason: "no PDS service in the DID document".to_string(),
            })
    }

    /// `createSession` with an identifier and (app) password.
    pub fn create_session(
        &self,
        pds: &str,
        identifier: &str,
        password: &str,
    ) -> Result<StoredSession, ArcError> {
        let (status, body) = self.post_json(
            &format!("{}/xrpc/com.atproto.server.createSession", base(pds)),
            None,
            &serde_json::json!({ "identifier": identifier, "password": password }),
        )?;
        if status != 200 {
            return Err(ArcError::Pds {
                what: "createSession",
                status,
                body,
            });
        }
        session_from(pds, &body, "createSession")
    }

    /// `refreshSession` with the refresh token. A 4xx naming the token is a
    /// dead session; a network failure or a 5xx is not an answer.
    pub fn refresh_session(&self, s: &StoredSession) -> Result<StoredSession, RefreshFailure> {
        let url = format!("{}/xrpc/com.atproto.server.refreshSession", base(&s.pds));
        let res = self
            .http
            .post(&url)
            .bearer_auth(&s.refresh_jwt)
            .send()
            .map_err(|e| RefreshFailure::Unavailable {
                reason: e.to_string(),
            })?;
        let status = res.status().as_u16();
        let body = res.text().unwrap_or_default();
        match status {
            200 => session_from(&s.pds, &body, "refreshSession").map_err(|e| {
                RefreshFailure::Unavailable {
                    reason: e.to_string(),
                }
            }),
            400 | 401 | 403 => Err(RefreshFailure::Dead {
                error: error_discriminant(&body).unwrap_or_else(|| format!("HTTP {status}")),
            }),
            _ => Err(RefreshFailure::Unavailable {
                reason: format!("HTTP {status}: {body}"),
            }),
        }
    }

    /// `getServiceAuth` on the session's own PDS: the proof the mint verifies.
    pub fn service_auth(
        &self,
        s: &StoredSession,
        aud: &str,
        lxm: &str,
    ) -> Result<String, ServiceAuthError> {
        let url = format!(
            "{}/xrpc/com.atproto.server.getServiceAuth?aud={}&lxm={}",
            base(&s.pds),
            enc(aud),
            enc(lxm)
        );
        let res = self
            .http
            .get(&url)
            .bearer_auth(&s.access_jwt)
            .send()
            .map_err(|e| {
                ServiceAuthError::Other(ArcError::Http {
                    what: "getServiceAuth",
                    reason: e.to_string(),
                })
            })?;
        let status = res.status().as_u16();
        let body = res.text().unwrap_or_default();
        if status == 200 {
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            return v
                .get("token")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .ok_or(ServiceAuthError::Other(ArcError::Pds {
                    what: "getServiceAuth answered without a token",
                    status,
                    body,
                }));
        }
        if error_discriminant(&body).as_deref() == Some("ExpiredToken") {
            return Err(ServiceAuthError::Expired);
        }
        Err(ServiceAuthError::Other(ArcError::Pds {
            what: "getServiceAuth",
            status,
            body,
        }))
    }

    /// One record's value, or `None` if the PDS says it does not exist.
    pub fn get_record(
        &self,
        pds: &str,
        did: &str,
        collection: &str,
        rkey: &str,
    ) -> Result<Option<Value>, ArcError> {
        let url = format!(
            "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey={}",
            base(pds),
            enc(did),
            enc(collection),
            enc(rkey)
        );
        let res = self.http.get(&url).send().map_err(|e| ArcError::Http {
            what: "getRecord",
            reason: e.to_string(),
        })?;
        let status = res.status().as_u16();
        let body = res.text().unwrap_or_default();
        match status {
            200 => Ok(serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| v.get("value").cloned())),
            400 | 404 if error_discriminant(&body).as_deref() == Some("RecordNotFound") => Ok(None),
            _ => Err(ArcError::Pds {
                what: "getRecord",
                status,
                body,
            }),
        }
    }

    /// Write one record into the session's own repo.
    pub fn put_record(
        &self,
        s: &StoredSession,
        collection: &str,
        rkey: &str,
        record: Value,
    ) -> Result<(), ArcError> {
        let (status, body) = self.post_json(
            &format!("{}/xrpc/com.atproto.repo.putRecord", base(&s.pds)),
            Some(&s.access_jwt),
            &serde_json::json!({
                "repo": s.did, "collection": collection, "rkey": rkey, "record": record,
            }),
        )?;
        if status != 200 {
            return Err(ArcError::Pds {
                what: "putRecord",
                status,
                body,
            });
        }
        Ok(())
    }

    /// Delete one record from the session's own repo (a rig leaving a repo as
    /// it found it; the arc itself never deletes).
    pub fn delete_record(
        &self,
        s: &StoredSession,
        collection: &str,
        rkey: &str,
    ) -> Result<(), ArcError> {
        let (status, body) = self.post_json(
            &format!("{}/xrpc/com.atproto.repo.deleteRecord", base(&s.pds)),
            Some(&s.access_jwt),
            &serde_json::json!({ "repo": s.did, "collection": collection, "rkey": rkey }),
        )?;
        if status != 200 {
            return Err(ArcError::Pds {
                what: "deleteRecord",
                status,
                body,
            });
        }
        Ok(())
    }

    /// All of a repo's endpoint records as (rkey, record); malformed ones
    /// (no endpointId) skipped, as the app and the admit both do.
    pub fn list_endpoints(
        &self,
        pds: &str,
        did: &str,
    ) -> Result<Vec<(String, EndpointRecord)>, ArcError> {
        let v = self.get_json(
            "listRecords",
            &format!(
                "{}/xrpc/com.atproto.repo.listRecords?repo={}&collection={ENDPOINT_COLLECTION}",
                base(pds),
                enc(did)
            ),
        )?;
        Ok(v.get("records")
            .and_then(Value::as_array)
            .map(|records| {
                records
                    .iter()
                    .filter_map(|r| {
                        let rkey = r.get("uri")?.as_str()?.rsplit('/').next()?.to_string();
                        let record = parse_record(r.get("value")?)?;
                        Some((rkey, record))
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// `POST /campToken` on the admit; the status and body, never thrown on
    /// status — the mapping is `admit::camp_outcome`'s.
    pub fn camp_token(&self, admit_base: &str, request: &Value) -> Result<(u16, String), ArcError> {
        self.post_json(&format!("{}/campToken", base(admit_base)), None, request)
    }

    fn get_json(&self, what: &'static str, url: &str) -> Result<Value, ArcError> {
        let res = self.http.get(url).send().map_err(|e| ArcError::Http {
            what,
            reason: e.to_string(),
        })?;
        let status = res.status().as_u16();
        let body = res.text().unwrap_or_default();
        if status != 200 {
            return Err(ArcError::Pds { what, status, body });
        }
        serde_json::from_str(&body).map_err(|e| ArcError::Pds {
            what,
            status,
            body: format!("not JSON ({e}): {body}"),
        })
    }

    fn post_json(
        &self,
        url: &str,
        bearer: Option<&str>,
        body: &Value,
    ) -> Result<(u16, String), ArcError> {
        let mut req = self.http.post(url).json(body);
        if let Some(token) = bearer {
            req = req.bearer_auth(token);
        }
        let res = req.send().map_err(|e| ArcError::Http {
            what: "POST",
            reason: format!("{url}: {e}"),
        })?;
        let status = res.status().as_u16();
        Ok((status, res.text().unwrap_or_default()))
    }
}

fn session_from(pds: &str, body: &str, what: &'static str) -> Result<StoredSession, ArcError> {
    let v: Value = serde_json::from_str(body).map_err(|e| ArcError::Pds {
        what,
        status: 200,
        body: format!("not JSON ({e})"),
    })?;
    let field = |k: &str| {
        v.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ArcError::Pds {
                what,
                status: 200,
                body: format!("answered without {k}"),
            })
    };
    let access_jwt = field("accessJwt")?;
    let access_expires_at_secs = access_expiry_secs(&access_jwt).ok_or_else(|| ArcError::Pds {
        what,
        status: 200,
        body: "the access token carries no exp".to_string(),
    })?;
    Ok(StoredSession {
        did: field("did")?,
        handle: field("handle")?,
        pds: pds.to_string(),
        refresh_jwt: field("refreshJwt")?,
        access_jwt,
        access_expires_at_secs,
    })
}

fn error_discriminant(body: &str) -> Option<String> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(str::to_string)
}

fn base(url: &str) -> &str {
    url.trim_end_matches('/')
}

fn enc(s: &str) -> String {
    // Minimal percent-encoding for query values: everything but unreserved.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
