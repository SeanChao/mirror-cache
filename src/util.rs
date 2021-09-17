use crate::error::Error;
use crate::error::Result;
use crate::metric;
use metrics::increment_counter;
use reqwest::ClientBuilder;
use sled::IVec;
use std::convert::TryInto;

pub fn now() -> i64 {
    chrono::offset::Local::now().timestamp()
}

pub fn now_nanos() -> i64 {
    chrono::offset::Local::now().timestamp_nanos()
}

pub async fn make_request(url: &str) -> Result<reqwest::Response> {
    increment_counter!(metric::CNT_OUT_REQUESTS);
    let client = ClientBuilder::new().build().unwrap();
    let resp = client.get(url).send().await;
    match resp {
        Ok(res) => {
            debug!("outbound request: {:?} {:?}", res.status(), res.headers());
            increment_counter!(metric::CNT_OUT_REQUESTS_SUCCESS);
            Ok(res)
        }
        Err(e) => {
            increment_counter!(metric::CNT_OUT_REQUESTS_FAILURE);
            Err(Error::RequestError(e))
        }
    }
}

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

pub fn ivec_to_u64(ivec: &IVec) -> u64 {
    u64::from_be_bytes(ivec.as_ref().try_into().unwrap())
}

pub fn u64_to_array(u: u64) -> [u8; 8] {
    u.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enough_precision_for_key_usage() {
        let mut set = std::collections::HashSet::new();
        for _ in 0..100 {
            set.insert(now_nanos());
        }
        assert_eq!(set.len(), 100);
    }

    #[test]
    fn ivec_u64_conversion() {
        let n: u64 = 233;
        assert_eq!(ivec_to_u64(&(&u64_to_array(n)).into()), n);
    }
}
