use crate::error::Error;
use crate::error::Result;
use crate::metric;
use metrics::increment_counter;
use reqwest::ClientBuilder;

pub fn now() -> i64 {
    chrono::offset::Local::now().timestamp()
}

pub fn split_dirs(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(idx) => {
            let (l, r) = path.split_at(idx);
            (l, &r[1..])
        }
        None => ("", path),
    }
}

pub fn rewrite_upstream(input: &str, from: &str, to: &str) -> String {
    input.replace(from, to)
}

pub fn pypi_index_rewrite(input: &str, base_url: &str) -> String {
    rewrite_upstream(
        input,
        "https://files.pythonhosted.org/",
        &format!("{}/{}", base_url, "pypi/"),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_dirs_two_level() {
        let s = "awesome/cache";
        let (l, r) = split_dirs(s);
        assert_eq!(l, "awesome");
        assert_eq!(r, "cache");
    }

    #[test]
    fn split_dirs_one_level() {
        let s = "42";
        let (l, r) = split_dirs(s);
        assert_eq!(l, "");
        assert_eq!(r, "42");
    }

    #[test]
    fn split_dirs_multi_level() {
        let s = "dead/beef/bread";
        let (l, r) = split_dirs(s);
        assert_eq!(l, &s[0..9]);
        assert_eq!(r, &s[10..]);
    }

    #[test]
    fn rewrite_upstream_no_change() {
        let src = "oh";
        assert_eq!(
            rewrite_upstream(src, "https://pypi.org/", "http://hacked/"),
            "oh"
        );
    }

    #[test]
    fn rewrite_upstream_all() {
        let src = "Index: https://pypi.org/ab/cd/efg\nhttps://pypi.org/";
        assert_eq!(
            rewrite_upstream(src, "https://pypi.org/", "http://hacked/"),
            "Index: http://hacked/ab/cd/efg\nhttp://hacked/"
        );
    }

    #[test]
    fn pypi_index_rewrite_all() {
        let src = "https://files.pythonhosted.org/packages/8c/e6/83";
        assert_eq!(
            pypi_index_rewrite(src, "https://lemon"),
            "https://lemon/pypi/packages/8c/e6/83"
        );
    }
}
