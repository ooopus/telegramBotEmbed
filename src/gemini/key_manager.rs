use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct ApiKey {
    key: String,
    disabled_until: Option<DateTime<Utc>>,
    requests: Vec<DateTime<Utc>>, // Track request timestamps
}

#[derive(Debug, Clone)]
pub struct GeminiKeyManager {
    keys: Arc<Mutex<Vec<ApiKey>>>,
    last_used_key_index: Arc<Mutex<usize>>,
    rpm_limit: u32,
    rpd_limit: u32,
}

impl GeminiKeyManager {
    pub fn new(api_keys: Vec<String>, rpm_limit: u32, rpd_limit: u32) -> Self {
        let keys = api_keys
            .into_iter()
            .map(|key| ApiKey {
                key,
                disabled_until: None,
                requests: Vec::new(),
            })
            .collect();
        Self {
            keys: Arc::new(Mutex::new(keys)),
            last_used_key_index: Arc::new(Mutex::new(0)),
            rpm_limit,
            rpd_limit,
        }
    }

    pub fn get_key(&self) -> Result<String> {
        let mut keys_guard = self.keys.lock().unwrap();
        let now = Utc::now();
        let one_day_ago = now - chrono::Duration::days(1);

        // First, iterate and update status of all keys (re-enable, prune old requests)
        for api_key in keys_guard.iter_mut() {
            // Re-enable keys disabled by 429 errors if the time has passed.
            if let Some(disabled_until) = api_key.disabled_until {
                if now >= disabled_until {
                    api_key.disabled_until = None;
                    log::info!(
                        "Re-enabling API key ending in ...{}",
                        api_key.key.chars().rev().take(4).collect::<String>()
                    );
                }
            }
            // Prune request timestamps older than 24 hours to keep the list small.
            api_key.requests.retain(|&t| t > one_day_ago);
        }

        let mut last_idx = self.last_used_key_index.lock().unwrap();
        if keys_guard.is_empty() {
            return Err(anyhow!("No API keys configured."));
        }

        let start_idx = (*last_idx + 1) % keys_guard.len();
        let one_minute_ago = now - chrono::Duration::minutes(1);

        // Find the next available key using a round-robin approach.
        for i in 0..keys_guard.len() {
            let idx = (start_idx + i) % keys_guard.len();

            let is_usable = {
                let api_key = &keys_guard[idx];
                if api_key.disabled_until.is_some() {
                    false
                } else if api_key.requests.len() >= self.rpd_limit as usize {
                    false
                } else {
                    let requests_in_last_minute = api_key
                        .requests
                        .iter()
                        .filter(|&&t| t > one_minute_ago)
                        .count();
                    requests_in_last_minute < self.rpm_limit as usize
                }
            };

            if is_usable {
                // Key is available. Update its state and return it.
                let api_key = &mut keys_guard[idx];
                api_key.requests.push(now);
                *last_idx = idx;
                return Ok(api_key.key.clone());
            }
        }

        Err(anyhow!(
            "All API keys are currently rate-limited or disabled."
        ))
    }

    pub fn disable_key(&self, key_to_disable: &str) {
        let mut keys = self.keys.lock().unwrap();
        if let Some(api_key) = keys.iter_mut().find(|k| k.key == key_to_disable) {
            // Disable the key until midnight UTC of the next day.
            let now = Utc::now();
            let tomorrow = (now.date_naive() + chrono::Duration::days(1))
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let tomorrow_utc = DateTime::<Utc>::from_naive_utc_and_offset(tomorrow, Utc);
            api_key.disabled_until = Some(tomorrow_utc);
            log::warn!(
                "Disabling API key ending in ...{} until {}",
                api_key.key.chars().rev().take(4).collect::<String>(),
                tomorrow_utc
            );
        }
    }
}
