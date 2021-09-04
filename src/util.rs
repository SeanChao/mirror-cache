use crate::error::Error;
use crate::error::Result;
use crate::metric;
use metrics::increment_counter;
use reqwest::ClientBuilder;

pub fn now() -> i64 {
    chrono::offset::Local::now().timestamp()
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

#[cfg(test)]
mod tests {}
